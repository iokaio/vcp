param([string]$InputPath)
$ErrorActionPreference = "Stop"
(Get-Item -LiteralPath $InputPath).Name
