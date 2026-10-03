# SPDX-License-Identifier: Apache-2.0
# Source-stage editor compatibility only; run npm run build before invoking.
# All profile, extension, workspace and evidence writes stay in a fresh artifacts root.
#requires -Version 7.0
param(
    [Parameter(Mandatory)][string]$Code,
    [string]$OutputRoot
)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
. (Join-Path $repo 'scripts/release/candidate-runtime.ps1')
$artifacts=Assert-BetaEditorPath (Join-Path $repo 'artifacts') $true
if (-not $OutputRoot) { $OutputRoot=Join-Path $artifacts ('editor-compatibility-'+[guid]::NewGuid().ToString('N')) }
$root=[IO.Path]::GetFullPath($OutputRoot)
$relative=[IO.Path]::GetRelativePath($artifacts,$root)
if ($relative -eq '.' -or $relative -eq '..' -or $relative.StartsWith('..'+[IO.Path]::DirectorySeparatorChar) -or [IO.Path]::IsPathRooted($relative)) { throw 'Compatibility output must be a named artifacts directory' }
if (Test-Path -LiteralPath $root) { throw 'Compatibility output must be fresh' }
$null=Assert-BetaEditorPath (Split-Path -Parent $root) $true
$Code=Assert-BetaEditorPath ([IO.Path]::GetFullPath($Code)) $false
$codeItem=Get-Item -LiteralPath $Code
$signature=Get-AuthenticodeSignature -LiteralPath $Code
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -cne 'CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US') { throw 'A valid official Microsoft editor executable is required' }
$runtimes=@(@($codeItem.Directory)+@(Get-ChildItem -LiteralPath $codeItem.DirectoryName -Directory) | Where-Object {
    $metadata=Join-Path $_.FullName 'resources/app/package.json'
    (Test-Path -LiteralPath $metadata) -and ((Get-Content -LiteralPath $metadata -Raw | ConvertFrom-Json).version -ceq $codeItem.VersionInfo.ProductVersion)
})
if ($runtimes.Count -ne 1) { throw 'Exactly one matching official editor runtime is required' }
$runtime=Assert-BetaEditorPath $runtimes[0].FullName $true
$product=Get-Content -LiteralPath (Join-Path $runtime 'resources/app/product.json') -Raw | ConvertFrom-Json
$version=$codeItem.VersionInfo.ProductVersion
$node=(Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$pwsh=(Get-Command pwsh -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$manifest=Get-Content -LiteralPath (Join-Path $PSScriptRoot '../package.json') -Raw | ConvertFrom-Json

$null=New-Item -ItemType Directory -Path $root
$extensions=Join-Path $root 'extensions'
$user=Join-Path $root 'user'
$workspace=Join-Path $root 'workspace'
foreach ($directory in @($extensions,(Join-Path $user 'User'),$workspace)) { $null=New-Item -ItemType Directory -Path $directory }
$extension=Join-Path $extensions ('iokaio.vcp-'+$manifest.version)
$null=Invoke-BetaProcess $node @((Join-Path $PSScriptRoot 'stage.cjs'),$extension) $root
$driver=Join-Path $extensions 'vcp-test.compatibility-smoke-0.0.1'
$null=New-Item -ItemType Directory -Path $driver
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../tests/compatibility-host.cjs') -Destination (Join-Path $driver 'driver.cjs')
@{name='compatibility-smoke';publisher='vcp-test';version='0.0.1';engines=@{vscode='^1.138.0'};main='./driver.cjs';activationEvents=@('*');extensionKind=@('ui');capabilities=@{untrustedWorkspaces=@{supported=$true}}} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $driver 'package.json') -Encoding utf8NoBOM
@{'security.workspace.trust.enabled'=$true;'security.workspace.trust.startupPrompt'='never';'update.mode'='none';'extensions.autoUpdate'=$false;'extensions.autoCheckUpdates'=$false;'telemetry.telemetryLevel'='off';'workbench.startupEditor'='none';'files.autoSave'='off'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $user 'User/settings.json') -Encoding utf8NoBOM
$workspaceFile=Join-Path $root 'compatibility.code-workspace'
@{folders=@(@{path=$workspace})} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $workspaceFile -Encoding utf8NoBOM
$inputFile=Join-Path $root 'input.json'
$resultFile=Join-Path $root 'result.json'
@{restrictedPath=$true;code=$Code;editorVersion=$version;installed=$true;workspaceFile=$workspaceFile;userData=$user;extensions=$extensions;extension=$extension;workspace=$workspace;version=$manifest.version;engineRange=$manifest.engines.vscode;result=$resultFile;stdout=(Join-Path $root 'editor.stdout.log');stderr=(Join-Path $root 'editor.stderr.log');diagnostics=(Join-Path $root 'editor-diagnostics');runtimeEvidence=(Join-Path $root 'editor-runtime.json')} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $inputFile -Encoding utf8NoBOM
$null=Invoke-BetaProcess $pwsh @('-NoProfile','-File',(Join-Path $PSScriptRoot 'run-extension-host.ps1'),'-InputFile',$inputFile) $root @{} 150
$result=Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
if ($result.status -cne 'pass') { throw ('Editor compatibility smoke failed: '+($result | ConvertTo-Json -Depth 8 -Compress)) }
$result | Add-Member -NotePropertyMembers @{editor_commit=$product.commit;editor_executable_sha256=(Get-FileHash -LiteralPath $Code -Algorithm SHA256).Hash.ToLowerInvariant();staged_manifest_sha256=(Get-FileHash -LiteralPath (Join-Path $extension 'package.json') -Algorithm SHA256).Hash.ToLowerInvariant();evidence_root=$root}
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $root 'verification.json') -Encoding utf8NoBOM
$result | ConvertTo-Json -Depth 8
