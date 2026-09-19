# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$Toolchain = 'stable',
    [ValidateRange(1,16)][int]$Jobs = 4
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native Windows is required for P3 owner qualification' }
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$output = Join-Path $repository ('artifacts/p3/' + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $output -Force | Out-Null
$manifest = Join-Path $output 'manifest.json'
$record = [ordered]@{schema_version=1;task_id='P3-01/P3-02';status='running';started_at=[DateTime]::UtcNow.ToString('o');stages=@()}
function Save-Record { $record | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifest -Encoding utf8 }
function Stage([string]$Name, [string]$Program, [string[]]$Arguments) {
    $log = Join-Path $output ($Name + '.log')
    & $Program @Arguments *> $log
    $code = $LASTEXITCODE
    $record.stages += @{name=$Name;command=@($Program)+$Arguments;exit_code=$code;log=$log;sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()}
    Save-Record
    if ($code -ne 0 -or (Get-Content -LiteralPath $log -Raw) -match 'panicked at') { throw "$Name failed; see $log" }
}
Save-Record
try {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vsRoot) { throw 'Visual C++ x64 build tools are required' }
    & (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
    foreach ($relative in @('Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin', 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')) {
        $env:PATH = (Join-Path $vsRoot $relative) + ';' + $env:PATH
    }
    $env:CARGO_TARGET_DIR = Join-Path $repository 'artifacts/codex-target'
    $env:RUST_MIN_STACK = '16777216'
    $env:CODEX_TEST_ENVIRONMENT = 'local'
    $env:VCP_TEST_NODE = (Get-Command node -CommandType Application).Source
    $env:VCP_TEST_GIT = (Get-Command git -CommandType Application).Source
    $record.rustc = & rustc "+$Toolchain" --version
    if ($LASTEXITCODE -ne 0) { throw 'Requested Rust toolchain is unavailable' }
    $record.node = & node --version
    $record.commit = & git -C $repository rev-parse HEAD
    $record.dirty = @(& git -C $repository status --porcelain).Count -ne 0
    $record.platform = [Runtime.InteropServices.RuntimeInformation]::OSDescription
    $inputs = @('vcp-cli','vcp-domain','vcp-protocol','vcp-store','vcp-engine','vcp-budget','vcp-lifecycle','vcp-models','vcp-tools') | ForEach-Object {
        Get-ChildItem -LiteralPath (Join-Path $repository "src/crates/$_") -Recurse -File |
            Where-Object { $_.Extension -in @('.rs','.cjs') -or $_.Name -eq 'Cargo.toml' } |
            ForEach-Object FullName
    }
    $inputs += @($PSCommandPath, (Join-Path $repository 'src/third_party/codex/codex-rs/Cargo.lock'))
    $record.inputs = @($inputs | Sort-Object | ForEach-Object {
        @{path=[IO.Path]::GetRelativePath($repository,$_).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant()}
    })
    Push-Location (Join-Path $repository 'src/third_party/codex/codex-rs')
    try {
        $base = @("+$Toolchain", 'test', '--locked', '--offline', '-j', "$Jobs")
        Stage 'contracts' 'cargo' ($base + @('-p','vcp-cli','-p','vcp-domain','-p','vcp-protocol','-p','vcp-store','-p','vcp-engine','-p','vcp-budget','-p','vcp-models','-p','vcp-tools','--tests'))
        Stage 'executable' 'cargo' ($base + @('-p','vcp-cli','--features','qualification','--test','executable'))
        $executableLog = Get-Content -LiteralPath (Join-Path $output 'executable.log') -Raw
        foreach ($required in @('executable_runs_verifies_lists_inspects_and_forks','executable_owner_controls_authenticate_and_cancel_inflight_work','executable_preflight_budget_question_and_incomplete_are_truthful','executable_closed_consumer_pauses_before_send_and_resume_keeps_its_cap','executable_terminal_pauses_steers_resizes_and_resumes_in_same_console','executable_terminal_question_requires_explicit_answer_and_resume')) {
            if ($executableLog -notmatch ('test ' + [regex]::Escape($required) + ' \.\.\. ok')) { throw "Required executable qualification did not run: $required" }
        }
        Stage 'native-terminal' 'cargo' ($base + @('-p','vcp-cli','--features','qualification','--test','terminal_console'))
        if ((Get-Content -LiteralPath (Join-Path $output 'native-terminal.log') -Raw) -notmatch 'test native_keyboard_unicode_resize_explicit_answer_and_close \.\.\. ok') { throw 'Native terminal qualification did not run' }
        Stage 'retained-control' 'cargo' ($base + @('-p','vcp-lifecycle','--test','canonical_host','cli_control_authenticates_before_stopping_and_retries_without_another_effect'))
        if ((Get-Content -LiteralPath (Join-Path $output 'retained-control.log') -Raw) -notmatch 'test cli_control::cli_control_authenticates_before_stopping_and_retries_without_another_effect \.\.\. ok') {
            throw 'Retained control qualification did not execute the required test'
        }
        Stage 'coding-turns' 'cargo' ($base + @('-p','vcp-lifecycle','--test','canonical_host','coding'))
        $turnLog = Get-Content -LiteralPath (Join-Path $output 'coding-turns.log') -Raw
        foreach ($required in @('canonical_coding_loop_assembles_current_sources_and_dispatches_prepared_files','retained_verification_checks_changed_source_and_accounts_final_response','provider_retry_reassembles_coding_continuity_with_current_liability','coding_budget_denial_is_durable_before_any_provider_send')) {
            if ($turnLog -notmatch ('test [^\r\n]*::' + [regex]::Escape($required) + ' \.\.\. ok')) { throw "Required coding turn qualification did not run: $required" }
        }
    } finally { Pop-Location }
    foreach ($inputRecord in $record.inputs) {
        if ((Get-FileHash -LiteralPath (Join-Path $repository $inputRecord.path)).Hash.ToLowerInvariant() -ne $inputRecord.sha256) { throw 'Qualification source changed during the run' }
    }
    $record.status = 'pass'
} catch {
    $record.status = 'fail'
    $record.reason = $_.Exception.Message
} finally {
    $record.ended_at = [DateTime]::UtcNow.ToString('o')
    Save-Record
}
$record | ConvertTo-Json -Depth 8
if ($record.status -ne 'pass') { exit 1 }
