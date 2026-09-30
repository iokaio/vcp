# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
[CmdletBinding()]
param([string]$OutputRoot, [ValidateRange(1,16)][int]$Jobs = 2, [switch]$Release, [string]$ReviewedCommit)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $IsWindows) { throw 'Native Windows build required' }
if ($Release -and $ReviewedCommit -notmatch '^[a-f0-9]{40}$') { throw 'Release requires -ReviewedCommit with the exact reviewed source commit' }
if (-not $Release -and $ReviewedCommit) { throw '-ReviewedCommit requires -Release' }
$releaseTool = Join-Path $PSScriptRoot 'release/provenance.cjs'
$nativeOverrideNames = @('CC','CXX','CL','_CL_','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','AR','ARFLAGS','CMAKE_GENERATOR','CMAKE_TOOLCHAIN_FILE','CMAKE_ARGS','CARGO_MAKEFLAGS')
$nativeOverrides = @(Get-ChildItem Env: | Where-Object {
    $_.Value -and ($_.Name -in $nativeOverrideNames -or $_.Name -match '^(CC|CXX|CFLAGS|CXXFLAGS|CPPFLAGS|LDFLAGS|AR|ARFLAGS)_' -or $_.Name -match '^(HOST|TARGET)_(CC|CXX|CFLAGS|CXXFLAGS|AR|ARFLAGS)$')
})
if ($nativeOverrides.Count) { throw ('Unqualified native build overrides: ' + ($nativeOverrides.Name -join ', ')) }
if (-not $OutputRoot) { $OutputRoot = Join-Path $repository 'artifacts/p8-production-build' }
$out = Join-Path ([IO.Path]::GetFullPath($OutputRoot)) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $out | Out-Null
$workspace = Join-Path $repository 'src/third_party/codex/codex-rs'
$target = Join-Path $repository 'artifacts/codex-target'
# A release starts with an empty target directory: no stale developer artifacts,
# feature unification or unrecorded build-script output can be reused.
if ($Release) { $target = Join-Path $out 'cargo-target' }
# This recipe uses the committed workspace configuration only. An additional
# ancestor/user Cargo configuration needs its own reviewed build recipe.
$configCandidates = @()
for ($ancestor = $workspace; $ancestor; $ancestor = Split-Path -Parent $ancestor) {
    $configCandidates += (Join-Path $ancestor '.cargo/config'), (Join-Path $ancestor '.cargo/config.toml')
}
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$configCandidates += (Join-Path $cargoHome 'config'), (Join-Path $cargoHome 'config.toml')
$allowedConfig = [IO.Path]::GetFullPath((Join-Path $workspace '.cargo/config.toml'))
$cargoConfigs = @($configCandidates | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ } | ForEach-Object {
    $configPath = [IO.Path]::GetFullPath($_)
    if ($configPath -cne $allowedConfig) { throw "Unqualified inherited Cargo configuration: $configPath" }
    @{path=$configPath;sha256=(Get-FileHash -LiteralPath $configPath).Hash.ToLowerInvariant()}
})
$identity = Join-Path $repository 'scripts/evals/memory-source-identity.cjs'
function Capture-Source([string]$Destination) {
    if ($Release) {
        & node $releaseTool source $repository $ReviewedCommit > $Destination
    } else {
        & node -e "const m=require(process.argv[1]); console.log(JSON.stringify(m.sourceIdentity(process.argv[2],['scripts/build-production.ps1','scripts/package.ps1','scripts/package-install.ps1','scripts/package-inventory.cjs','scripts/package-models.ps1','scripts/skills','src/third_party/upstreams.toml','src/third_party/components','src/skills/builtin','LICENSE','NOTICE','THIRD_PARTY_NOTICES.md'])));" $identity $repository > $Destination
    }
    if ($LASTEXITCODE -ne 0) { throw 'Source inventory failed' }
}
$before = Join-Path $out 'source-before.json'
Capture-Source $before
& node (Join-Path $repository 'scripts/upstream/reconstruct.cjs') verify --component codex *> (Join-Path $out 'upstream-verification.log')
if ($LASTEXITCODE -ne 0) { throw 'Selected upstream source inventory mismatch' }
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$env:PATH = (Split-Path -Parent $vswhere) + ';' + $env:PATH
$vsRoot = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsRoot) { throw 'Native Visual C++ tools required' }
& (Join-Path $vsRoot 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
foreach ($relative in @('Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin','Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')) { $env:PATH = (Join-Path $vsRoot $relative) + ';' + $env:PATH }
$nativeTools = @('cl','link','lib','cmake','ninja') | ForEach-Object {
    $tool = (Get-Command $_ -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    @{name=$_;path=$tool;sha256=(Get-FileHash -LiteralPath $tool).Hash.ToLowerInvariant()}
}
$nativeTools += @('rustc','cargo') | ForEach-Object {
    $tool = [string](& rustup which --toolchain 1.95.0 $_)
    if ($LASTEXITCODE -ne 0 -or -not $tool) { throw "Cannot locate pinned toolchain: $_" }
    @{name=$_;path=$tool;sha256=(Get-FileHash -LiteralPath $tool).Hash.ToLowerInvariant()}
}
$nodeTool = (Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$nativeTools += @{name='node';path=$nodeTool;sha256=(Get-FileHash -LiteralPath $nodeTool).Hash.ToLowerInvariant()}
$env:RUST_MIN_STACK = '16777216'
# The committed target configuration selects static CRT. Do not inherit feature,
# rustflag or release-profile overrides from a developer's environment.
foreach ($name in @('RUSTFLAGS','RUSTC_BOOTSTRAP','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_RUSTFLAGS','CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS','CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER','CARGO_BUILD_RUSTC','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw "Unexpected build override: $name" }
}
if (@(Get-ChildItem Env: | Where-Object Name -like 'CARGO_PROFILE_RELEASE_*').Count) { throw 'Release profile overrides are not allowed' }
$rustflags = @('-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static')
$env:CARGO_ENCODED_RUSTFLAGS = $rustflags -join [char]31
$arguments = @('+1.95.0','build','--locked','--offline','--release','--no-default-features','-p','vcp-cli','--bin','vcp','--target','x86_64-pc-windows-msvc','--target-dir',$target,'-j',"$Jobs",'--message-format=json-render-diagnostics')
$log = Join-Path $out 'build.log'
$dependenciesBefore = Join-Path $out 'dependencies-before.json'
$dependenciesAfter = Join-Path $out 'dependencies-after.json'
$noticesTool = Join-Path $PSScriptRoot 'release/notices.cjs'
if ($Release) {
    & node $noticesTool verify-source $repository > $dependenciesBefore
    if ($LASTEXITCODE -ne 0) { throw 'Pinned dependency source/license verification failed before compilation' }
}
$started = [DateTime]::UtcNow
Write-Output "Production build evidence: $out"
Push-Location $workspace
try { & cargo @arguments *> $log; $code = $LASTEXITCODE } finally { Pop-Location }
$dependenciesCode = 0
if ($Release) { & node $noticesTool verify-source $repository > $dependenciesAfter; $dependenciesCode = $LASTEXITCODE }
& node (Join-Path $repository 'scripts/upstream/reconstruct.cjs') verify --component codex *> (Join-Path $out 'upstream-verification-after.log')
$upstreamCode = $LASTEXITCODE
$after = Join-Path $out 'source-after.json'
Capture-Source $after
$source = Get-Content -LiteralPath $before -Raw | ConvertFrom-Json
$finalSource = Get-Content -LiteralPath $after -Raw | ConvertFrom-Json
$receipt = [ordered]@{schema='vcp-local-build/1'; exit_code=$code; cargo_exit_code=$code; source_commit=$source.commit; source_dirty=$source.dirty; source_content_sha256=$source.content_sha256; source_stable=($source.content_sha256 -ceq $finalSource.content_sha256 -and $upstreamCode -eq 0); upstream_before_sha256=(Get-FileHash -LiteralPath (Join-Path $out 'upstream-verification.log')).Hash.ToLowerInvariant(); upstream_after_sha256=(Get-FileHash -LiteralPath (Join-Path $out 'upstream-verification-after.log')).Hash.ToLowerInvariant(); cargo_configs=$cargoConfigs; rustflags=$rustflags; command=@('cargo')+$arguments; working_directory=$workspace; rustc=(& rustc +1.95.0 --version --verbose); msvc=$env:VCToolsVersion; started_at=$started.ToString('o'); ended_at=[DateTime]::UtcNow.ToString('o'); qualification_build=$false; profile='release'; target='x86_64-pc-windows-msvc'; inputs=$source.files; log_sha256=(Get-FileHash -LiteralPath $log).Hash.ToLowerInvariant()}
$receipt.native_tools = @($nativeTools)
if ($Release) {
    $receipt.dependency_source = Get-Content -LiteralPath $dependenciesBefore -Raw | ConvertFrom-Json
    $receipt.dependencies_before_sha256 = (Get-FileHash -LiteralPath $dependenciesBefore).Hash.ToLowerInvariant()
    $receipt.dependencies_after_sha256 = (Get-FileHash -LiteralPath $dependenciesAfter).Hash.ToLowerInvariant()
    $receipt.dependency_sources_stable = ($dependenciesCode -eq 0 -and $receipt.dependencies_before_sha256 -ceq $receipt.dependencies_after_sha256)
    if (-not $receipt.dependency_sources_stable) { $receipt.exit_code = 1; $receipt.failure = 'Pinned dependency source or license inventory changed during compilation' }
}
$receipt.toolchain_stable = $true
foreach ($tool in $nativeTools) {
    if (-not (Test-Path -LiteralPath $tool.path) -or (Get-FileHash -LiteralPath $tool.path).Hash.ToLowerInvariant() -cne $tool.sha256) { $receipt.toolchain_stable = $false }
}
if (-not $receipt.toolchain_stable) { $receipt.exit_code = 1; $receipt.failure = 'Build toolchain changed during compilation' }
if (-not $receipt.source_stable) { $receipt.exit_code = 1; $receipt.failure = 'Source or upstream inventory changed during compilation' }
if ($receipt.exit_code -eq 0) {
    try {
    $artifacts = @(Get-Content -LiteralPath $log | Where-Object { $_.StartsWith('{') } | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object reason -eq 'compiler-artifact')
    $vcpArtifacts = @($artifacts | Where-Object { $_.target.name -eq 'vcp' -or $_.target.name.StartsWith('vcp_') })
    $receipt.vcp_features = @($vcpArtifacts | ForEach-Object { @{target=$_.target.name;features=@($_.features)} })
    if (@($vcpArtifacts | Where-Object { 'qualification' -in $_.features }).Count) { throw 'Qualification feature present in a VCP dependency' }
    $compiled = @($artifacts | Where-Object { $_.target.name -eq 'vcp' -and $_.executable -and -not $_.profile.test })
    if ($compiled.Count -ne 1 -or @($compiled[0].features).Count -ne 0 -or $compiled[0].profile.opt_level -ne '3') { throw 'Expected one optimized executable with no qualification features' }
    $executable = Join-Path $out 'vcp.exe'
    Copy-Item -LiteralPath $compiled[0].executable -Destination $executable
    $receipt.executable = $executable
    $receipt.executable_sha256 = (Get-FileHash -LiteralPath $executable).Hash.ToLowerInvariant()
    $receipt.compiler_artifact = $compiled[0]
    if ($Release) {
        $releaseJson = & node -e "const p=require(process.argv[1]),s=p.json(process.argv[3]),c=p.channel(process.argv[2]),b=p.verifyExecutable(process.argv[4],c.native_version);console.log(JSON.stringify({release:p.releaseIdentity(c,s,b.sha256),version:b.version,target:b.target}));" $releaseTool $repository $before $executable
        if ($LASTEXITCODE -ne 0) { throw 'Release executable identity failed' }
        $releaseIdentity = $releaseJson | ConvertFrom-Json
        $receipt.release = $releaseIdentity.release
        $receipt.executable_version = $releaseIdentity.version
        $receipt.executable_target = $releaseIdentity.target
    }
    $symbols = [IO.Path]::ChangeExtension($compiled[0].executable, '.pdb')
    if (Test-Path -LiteralPath $symbols) { Copy-Item -LiteralPath $symbols -Destination (Join-Path $out 'vcp.pdb'); $receipt.symbols_sha256 = (Get-FileHash -LiteralPath (Join-Path $out 'vcp.pdb')).Hash.ToLowerInvariant() }
    } catch { $receipt.exit_code = 1; $receipt.failure = $_.Exception.Message }
}
$receiptPath = Join-Path $out 'build-receipt.json'
$receipt | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $receiptPath -Encoding utf8NoBOM
if ($receipt.exit_code -ne 0) { throw "Production build failed or inputs changed; retained $receiptPath" }
if ($Release) {
    & node $releaseTool verify-build $repository $receiptPath $receipt.executable $ReviewedCommit | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Strict release provenance failed; retained $receiptPath" }
}
Write-Output $receiptPath
