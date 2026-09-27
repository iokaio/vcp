# SPDX-License-Identifier: Apache-2.0
# Build a fresh, source-bound diagnostic probe. This command never launches a browser.
#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$CoreAssembly,
    [Parameter(Mandatory)][string]$Loader,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
if (-not $IsWindows -or [IntPtr]::Size -ne 8) { throw 'Native x64 Windows required' }
function Hash([string]$File) { (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant() }
function Assert-PlainPath([string]$Value,[string]$Label,[bool]$Leaf) {
    if ([string]::IsNullOrWhiteSpace($Value) -or -not [IO.Path]::IsPathFullyQualified($Value)) { throw "$Label must be an absolute path" }
    $full=[IO.Path]::GetFullPath($Value)
    if (-not (Test-Path -LiteralPath $full)) { throw "$Label does not exist" }
    $item=Get-Item -LiteralPath $full -Force
    if ($Leaf -and -not ($item -is [IO.FileInfo])) { throw "$Label must be a regular file" }
    if (-not $Leaf -and -not ($item -is [IO.DirectoryInfo])) { throw "$Label must be a directory" }
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "$Label must not be a reparse point" }
    return $full
}
function Assert-ArtifactChain([string]$Directory,[string]$Boundary) {
    $current=[IO.Path]::GetFullPath($Directory); $stop=[IO.Path]::GetFullPath($Boundary)
    while ($true) {
        $item=Get-Item -LiteralPath $current -Force
        if (-not ($item -is [IO.DirectoryInfo]) -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Build path contains a non-directory or redirected component' }
        if ($current.Equals($stop,[StringComparison]::OrdinalIgnoreCase)) { return }
        $parent=[IO.Path]::GetDirectoryName($current)
        if (-not $parent -or $parent.Equals($current,[StringComparison]::OrdinalIgnoreCase)) { throw 'Build path escaped repository artifacts' }
        $current=$parent
    }
}
$repository=Assert-PlainPath ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))) 'Repository root' $false
$source=Assert-PlainPath (Join-Path $repository 'src/tests/support/windows/webapp') 'Diagnostic source root' $false
Assert-ArtifactChain $source $repository
$output=[IO.Path]::GetFullPath($OutputDirectory)
$artifacts=Assert-PlainPath (Join-Path $repository 'artifacts') 'Repository artifacts root' $false
if (-not $output.StartsWith($artifacts+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Build output must be a new directory under repository artifacts' }
for ($ancestor=[IO.Path]::GetDirectoryName($output); $ancestor -and $ancestor.StartsWith($artifacts,[StringComparison]::OrdinalIgnoreCase); $ancestor=[IO.Path]::GetDirectoryName($ancestor)) {
    if (Test-Path -LiteralPath $ancestor) { Assert-ArtifactChain $ancestor $artifacts; break }
}
if (Test-Path -LiteralPath $output) { throw 'Refuse to reuse an existing build or run directory' }
$deps=@(
    # Fresh Hyper-V diagnostic: Microsoft.Web.WebView2 NuGet 1.0.4191.47,
    # lib/net462 Core and build/native/x64 loader. Not the earlier host SDK.
    @{source=$CoreAssembly;name='Microsoft.Web.WebView2.Core.dll';sha256='e6f54c8ce208e3797c427d01ad671b47cb25abc85604753d6ec2546d0ffef550'},
    @{source=$Loader;name='WebView2Loader.dll';sha256='c66e4a92fdc7a216118e43b7a5024ea2200e8c43f9310bf20d96a0084f82c5bc'}
)
foreach ($dep in $deps) {
    $dep.source=Assert-PlainPath $dep.source ('Pinned dependency '+$dep.name) $true
    if ((Hash $dep.source) -cne $dep.sha256) { throw 'Pinned SDK/loader input differs' }
}
$names=@('HostContract.cs','HostContractTests.cs','WebViewHost.cs','WebDomContract.cs','WebDomContractTests.cs','NativeProbe.cs','WebViewSupervisor.cs','WorkerGuardian.cs','ProbeContract.cs','DomEvidence.cs','InputDiagnosticEvidence.cs','Invoke-NativeProbe.ps1','Input-Policy.ps1','Controller-helpers.ps1','Pe-Contract.ps1','Test-Contracts.ps1','Test-WorkerGuardian.ps1')
$sources=@($names | ForEach-Object { $file=Assert-PlainPath (Join-Path $source $_) ('Diagnostic source '+$_) $true; [ordered]@{path=$_;sha256=(Hash $file)} })
$builderSha256=Hash $PSCommandPath
$compiler=Assert-PlainPath 'C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe' 'C# compiler' $true
$references=Assert-PlainPath 'C:/Program Files (x86)/Reference Assemblies/Microsoft/Framework/.NETFramework/v4.8' '.NET Framework reference root' $false
$referenceNames=@('mscorlib.dll','System.dll','System.Core.dll','System.Drawing.dll','System.Windows.Forms.dll','System.Web.Extensions.dll')
$toolchain=@([ordered]@{path=$compiler;sha256=(Hash $compiler)})+@($referenceNames | ForEach-Object { $file=Assert-PlainPath (Join-Path $references $_) ('Reference assembly '+$_) $true; [ordered]@{path=$file;sha256=(Hash $file)} })
New-Item -ItemType Directory -Path $output -ErrorAction Stop | Out-Null
Assert-ArtifactChain $output $artifacts
foreach ($entry in $sources) {
    $origin=Join-Path $source $entry.path; $target=Join-Path $output $entry.path
    if ((Hash $origin) -cne $entry.sha256) { throw 'Source changed before staging' }
    [IO.File]::Copy($origin,$target,$false)
    if ((Hash $target) -cne $entry.sha256) { throw 'Staged source differs' }
}
foreach ($dep in $deps) {
    $target=Join-Path $output $dep.name
    if ((Hash $dep.source) -cne $dep.sha256) { throw 'Pinned dependency changed before staging' }
    [IO.File]::Copy([IO.Path]::GetFullPath($dep.source),$target,$false)
    if ((Hash $target) -cne $dep.sha256) { throw 'Staged dependency differs' }
}
$common=@('/nologo','/noconfig','/nostdlib+','/platform:x64','/optimize+',"/reference:$references\mscorlib.dll","/reference:$references\System.dll","/reference:$references\System.Core.dll")
$buildTemp=Join-Path $output 'compiler-temp'
New-Item -ItemType Directory -Path $buildTemp -ErrorAction Stop | Out-Null
$oldTemp=$env:TEMP; $oldTmp=$env:TMP
try {
    $env:TEMP=$buildTemp; $env:TMP=$buildTemp
    # Compilation and these pure contract checks never start WebView2 or another browser.
    & $compiler @common /target:winexe "/out:$output\WebViewHost.exe" "/reference:$references\System.Drawing.dll" "/reference:$references\System.Windows.Forms.dll" "/reference:$references\System.Web.Extensions.dll" "/reference:$output\Microsoft.Web.WebView2.Core.dll" "$output\HostContract.cs" "$output\WebDomContract.cs" "$output\WebViewHost.cs"
    if ($LASTEXITCODE -ne 0) { throw 'Host compile failed' }
    foreach ($suite in @('HostContract','WebDomContract')) {
        & $compiler @common /target:exe "/out:$output\$($suite)Tests.exe" "$output\$suite.cs" "$output\$($suite)Tests.cs"
        if ($LASTEXITCODE -ne 0) { throw 'Pure test compilation failed' }
        & "$output/$($suite)Tests.exe"
        if ($LASTEXITCODE -ne 0) { throw 'Pure test failed' }
    }
    . (Join-Path $output 'Pe-Contract.ps1')
    Assert-PeExecutable ([IO.File]::ReadAllBytes((Join-Path $output 'WebViewHost.exe'))) 2
    & (Join-Path $output 'Test-Contracts.ps1')
    & (Join-Path $output 'Invoke-NativeProbe.ps1') -Mode compile-only
} finally {
    $env:TEMP=$oldTemp; $env:TMP=$oldTmp
}
foreach ($entry in $sources) {
    if ((Hash (Join-Path $source $entry.path)) -cne $entry.sha256 -or (Hash (Join-Path $output $entry.path)) -cne $entry.sha256) { throw 'Source changed during build' }
}
foreach ($dep in $deps) { if ((Hash $dep.source) -cne $dep.sha256 -or (Hash (Join-Path $output $dep.name)) -cne $dep.sha256) { throw 'Pinned dependency changed during build' } }
foreach ($entry in $toolchain) { if ((Hash $entry.path) -cne $entry.sha256) { throw 'Compiler or reference assembly changed during build' } }
if ((Hash $PSCommandPath) -cne $builderSha256) { throw 'Builder changed during build' }
$hostFiles=@('WebViewHost.exe','Microsoft.Web.WebView2.Core.dll','WebView2Loader.dll') | ForEach-Object {
    $file=Join-Path $output $_
    [ordered]@{path=$_;bytes=(Get-Item -LiteralPath $file).Length;sha256=(Hash $file)}
}
$buildOutputs=@('WebViewHost.exe','HostContractTests.exe','WebDomContractTests.exe') | ForEach-Object {
    $file=Join-Path $output $_
    [ordered]@{path=$_;bytes=(Get-Item -LiteralPath $file).Length;sha256=(Hash $file)}
}
$manifest=[ordered]@{
    schema='cs3-webview2-inputs/1';probe_kind='full-dom-f24-readiness-diagnostic';input_host='message-only-hidden';readiness_key='F24';readiness_windows_virtual_key_code=135;readiness_pairs_per_gate=1;pre_sentinel_settle_milliseconds=100;enter_retries=0;text_insertion_retries=0;runtime='C:\Program Files (x86)\Microsoft\EdgeWebView\Application\153.0.4234.48';version='153.0.4234.48'
    runtime_executable_sha256='65afdc3965a6d1c4ccd5b47801fec8a16db15613d35c8e3a6d1fb6c0da970eea'
    sdk_package='Microsoft.Web.WebView2';sdk_version='1.0.4191.47';sdk_package_sha256='f492bbf547d0da329553b6727435b677579b1e9f91cc9e4a1ad029366d5f23d0'
    browser_argument='--edge-webview-no-dpi-workaround';host=@($hostFiles);build_outputs=@($buildOutputs);sources=$sources;external_dependencies=@($deps | ForEach-Object { [ordered]@{path=$_.name;sha256=$_.sha256} });toolchain=$toolchain
    builder_sha256=$builderSha256;builder_powershell=$PSVersionTable.PSVersion.ToString();builder_architecture=[Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
    runtime_identity_limitation='Installed Evergreen is serviced in place. Launch-time and before/after identity checks are not continuous immutability proof.'
    dpi_argument_limitation='Development diagnostic for the documented shell-launch workaround only; not sandbox, capability, compatibility or product evidence.'
    scope='synthetic in-memory full-DOM readiness diagnostic; sentinel and form outcomes are evidence, not general browser/server qualification or a CS-3 completion claim'
}
[IO.File]::WriteAllText((Join-Path $output 'inputs.json'),($manifest | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
[ordered]@{build=$output;inputs_sha256=(Hash (Join-Path $output 'inputs.json'));browser_executed=$false;full_plan_user_authorized=$true;root_review_required=$true;launch_ready=$false} | ConvertTo-Json
