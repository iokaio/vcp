# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# Qualification-only Windows runner. All child output is untrusted data.
param([Parameter(Mandatory)][string]$Config, [string]$ProfileName)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') { throw 'Native x64 Windows required' }
function Read-Bounded([string]$Path, [long]$Maximum) {
    $stream = [IO.FileStream]::new($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -gt $Maximum) { throw 'File exceeds byte ceiling' }
        $bytes = [byte[]]::new([int]$stream.Length)
        $stream.ReadExactly($bytes)
        return ,$bytes
    } finally { $stream.Dispose() }
}
$request = [Text.UTF8Encoding]::new($false, $true).GetString((Read-Bounded $Config 262144)) | ConvertFrom-Json
if ($request.schema -ne 1 -or $request.node_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
    $request.bootstrap_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
    $request.memory_bytes -lt 67108864 -or $request.memory_bytes -gt 1073741824 -or
    $request.output_limit -lt 1 -or $request.output_limit -gt 1048576 -or
    $request.timeout_ms -lt 1 -or $request.timeout_ms -gt 180000) { throw 'Invalid pinned runner request' }
# Interactive mode relays bounded newline frames between the trusted parent (this
# runner's stdin/stdout) and the contained child. Single-shot requests omit `mode`.
$fields = $request.PSObject.Properties.Name
$interactive = $fields -contains 'mode'
if ($interactive) {
    $bounds = $request.interaction
    if ($request.mode -cne 'interactive' -or $fields -contains 'input_base64' -or $null -eq $bounds -or
        ($bounds.PSObject.Properties.Name | Sort-Object) -join ',' -cne 'idle_ms,max_frame_bytes,max_frames,max_total_bytes' -or
        $bounds.max_frames -isnot [long] -or $bounds.max_frames -lt 1 -or $bounds.max_frames -gt 4096 -or
        $bounds.max_frame_bytes -isnot [long] -or $bounds.max_frame_bytes -lt 1 -or $bounds.max_frame_bytes -gt 65536 -or
        $bounds.max_total_bytes -isnot [long] -or $bounds.max_total_bytes -lt 1 -or $bounds.max_total_bytes -gt 1048576 -or
        $bounds.idle_ms -isnot [long] -or $bounds.idle_ms -lt 1 -or $bounds.idle_ms -gt $request.timeout_ms -or
        $request.output_limit -gt 65536) { throw 'Invalid interactive runner request' }
}
function Read-Regular([string]$Path, [string]$Hash, [long]$Maximum) {
    $full = [IO.Path]::GetFullPath($Path)
    $item = Get-Item -LiteralPath $full
    if ($item.PSIsContainer -or $item.Length -gt $Maximum) { throw 'Invalid bounded source file' }
    for ($part = $item; $null -ne $part; $part = $part.Parent ?? $part.Directory) {
        if ($part.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse source rejected' }
    }
    $bytes = Read-Bounded $full $Maximum
    if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant() -cne $Hash) { throw 'Pinned source hash mismatch' }
    return ,$bytes
}
$node = Read-Regular $request.node $request.node_sha256 134217728
$bootstrap = Join-Path $PSScriptRoot ($interactive ? 'node-fixture-interactive-bootstrap.cjs' : 'node-fixture-bootstrap.cjs')
$bootstrapBytes = Read-Regular $bootstrap $request.bootstrap_sha256 65536
$inputBytes = [byte[]]::new(0)
if (-not $interactive) {
    if ($request.input_base64 -isnot [string] -or $request.input_base64.Length -gt 87384) { throw 'Encoded input exceeds byte ceiling' }
    $inputBytes = [Convert]::FromBase64String($request.input_base64)
}
if ($inputBytes.Length -gt 65536 -or $request.files.Count -lt 1 -or $request.files.Count -gt 16) { throw 'Invalid bounded fixture input' }
$files = @{}
$total = 0
foreach ($file in $request.files) {
    if ($file.name -cnotmatch '^[a-z][a-z0-9-]*\.(cjs|json)$' -or $file.name -in @('bootstrap.cjs', 'package.json') -or
        $files.ContainsKey($file.name) -or $file.sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid fixture inventory' }
    $bytes = Read-Regular $file.path $file.sha256 1048576
    $total += $bytes.Length
    if ($total -gt 1048576) { throw 'Fixture inventory exceeds byte ceiling' }
    $files.Add($file.name, $bytes)
}
if (-not $files.ContainsKey('candidate.cjs')) { throw 'Missing candidate entry point' }
Add-Type -Path (Join-Path $PSScriptRoot '../../src/tests/support/windows/AppContainerFixture.cs')
$fixture = if ($ProfileName) { [Vcp.Qualification.AppContainerFixture]::new($ProfileName) } else { [Vcp.Qualification.AppContainerFixture]::new() }
$outcome = [ordered]@{ schema = 1; cleanup = 'pending'; node_sha256 = $request.node_sha256; bootstrap_sha256 = $request.bootstrap_sha256; result = $null }
try {
    $program = Join-Path $fixture.Root 'node.exe'
    [IO.File]::WriteAllBytes($program, $node)
    [IO.File]::WriteAllBytes((Join-Path $fixture.Root 'bootstrap.cjs'), $bootstrapBytes)
    [IO.File]::WriteAllText((Join-Path $fixture.Root 'package.json'), '{"type":"commonjs"}')
    foreach ($name in $files.Keys) { [IO.File]::WriteAllBytes((Join-Path $fixture.Root $name), $files[$name]) }
    # This first adapter only computes external JSON responses. It needs no
    # writable fixture files. Give the container read/execute and the owner full
    # control, replacing inherited writable profile access for this owned tree.
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    $inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
    $ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    foreach ($entry in @(@($ownerSid, 'FullControl'), @([Security.Principal.SecurityIdentifier]::new('S-1-5-18'), 'FullControl'),
        @([Security.Principal.SecurityIdentifier]::new($fixture.Sid), 'ReadAndExecute'))) {
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($entry[0], $entry[1], $inherit, 'None', 'Allow'))
    }
    Set-Acl -LiteralPath $fixture.Root -AclObject $acl
    # Node's default realpath walks outside the profile. Keep loader paths literal;
    # the OS boundary still denies outside access and fixture input rejects links.
    $arguments = @('--no-addons', '--preserve-symlinks', '--preserve-symlinks-main', 'bootstrap.cjs')
    if ($interactive) {
        $parentOutput = [Console]::OpenStandardOutput()
        $result = $fixture.RunInteractive($program, $arguments, $true, [Console]::OpenStandardInput(), $parentOutput,
            $bounds.max_frames, $bounds.max_frame_bytes, $bounds.max_total_bytes, $request.output_limit,
            $request.memory_bytes, $request.timeout_ms, $bounds.idle_ms, [Threading.CancellationToken]::None)
        $outcome.mode = 'interactive'
        $outcome.result = [ordered]@{
            process = $result.Process; pid = $result.Pid; termination = $result.Termination
            stderr_base64 = [Convert]::ToBase64String($result.Error); stderr_bytes = $result.ErrorBytes
            frames_to_child = $result.FramesToChild; frames_from_child = $result.FramesFromChild
            bytes_to_child = $result.BytesToChild; bytes_from_child = $result.BytesFromChild; trailing_bytes = $result.TrailingBytes
            peak_active_processes = $result.PeakActiveProcesses; memory_limit_bytes = $result.MemoryLimitBytes
        }
    } else {
        $result = $fixture.RunBounded($program, $arguments, $true, $inputBytes,
            $request.output_limit, $request.memory_bytes, $request.timeout_ms, [Threading.CancellationToken]::None)
        $outcome.result = [ordered]@{
            process = $result.Process; pid = $result.Pid; termination = $result.Termination
            stdout_base64 = [Convert]::ToBase64String($result.Output); stderr_base64 = [Convert]::ToBase64String($result.Error)
            stdout_bytes = $result.OutputBytes; stderr_bytes = $result.ErrorBytes
            peak_active_processes = $result.PeakActiveProcesses; memory_limit_bytes = $result.MemoryLimitBytes
        }
    }
    foreach ($name in $files.Keys) {
        if (-not [Linq.Enumerable]::SequenceEqual([byte[]]$files[$name], [IO.File]::ReadAllBytes((Join-Path $fixture.Root $name)))) { throw 'Fixture input changed during execution' }
    }
} finally {
    $fixture.Dispose()
    $outcome.cleanup = 'completed'
}
if ($interactive) {
    # The receipt shares the relay stream, so it travels in its own envelope.
    $receipt = [Text.Encoding]::UTF8.GetBytes('{"receipt":' + ($outcome | ConvertTo-Json -Depth 8 -Compress) + "}`n")
    $parentOutput.Write($receipt, 0, $receipt.Length); $parentOutput.Flush()
} else { $outcome | ConvertTo-Json -Depth 8 -Compress }
