param([string]$FixturePath)
Get-Item -LiteralPath $FixturePath -ErrorAction Stop | Select-Object -ExpandProperty Name
