param(
  [string]$Src = $PSScriptRoot,
  [string]$Out = (Join-Path $PSScriptRoot "out"),
  [string[]]$Only = @(),
  [double]$Scale = 1.5
)
$ErrorActionPreference = "Stop"
$chrome = @(
  "C:\Program Files\Google\Chrome\Application\chrome.exe",
  "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $chrome) { throw "No Chromium-based browser found for headless rendering." }
New-Item -ItemType Directory -Force $Out | Out-Null

$pages = Get-ChildItem (Join-Path $Src "*.html") | Sort-Object Name
if ($Only.Count -gt 0) { $pages = $pages | Where-Object { $Only -contains $_.BaseName } }
foreach ($p in $pages) {
  $html = Get-Content $p.FullName -Raw
  $w = 1600; $h = 1000
  if ($html -match '<meta name="mock-size" content="(\d+)x(\d+)"') { $w = [int]$Matches[1]; $h = [int]$Matches[2] }
  $png = Join-Path $Out ($p.BaseName + ".png")
  $uri = "file:///" + ($p.FullName -replace '\\','/')
  & $chrome --headless=new --disable-gpu --hide-scrollbars --no-first-run --no-default-browser-check `
    "--force-device-scale-factor=$Scale" "--window-size=$w,$h" "--screenshot=$png" $uri 2>&1 | Out-Null
  if (-not (Test-Path $png)) { throw "Render failed: $($p.Name)" }
  "{0,-40} {1}x{2} @{3}x -> {4:N0} KB" -f $p.Name, $w, $h, $Scale, ((Get-Item $png).Length / 1KB)
}
