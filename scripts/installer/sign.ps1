# SPDX-License-Identifier: Apache-2.0
#requires -Version 7.0
# Inno invokes this only for its newly generated setup and uninstaller.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Context,[Parameter(Mandatory)][string]$File)
$ErrorActionPreference = 'Stop'
$config = Get-Content -LiteralPath $Context -Raw | ConvertFrom-Json
$target = [IO.Path]::GetFullPath($File)
$parent = [IO.Path]::GetDirectoryName($target)
if ($parent -ieq $config.uninstaller_root) { $role='uninstaller' }
elseif ($parent -ieq $config.output_root) { $role='setup' }
else { throw 'Inno signing input is outside this build output' }
$evidence = Join-Path $config.output_root ('signing/' + $role)
& (Join-Path $PSScriptRoot '../release/signing-file.ps1') -File $target -Role $role -OutputRoot $evidence -ToolsManifest $config.tools_manifest | Out-Null
Copy-Item -LiteralPath $target -Destination (Join-Path $evidence 'signed.exe')
