# SPDX-License-Identifier: Apache-2.0
# Qualification-only resolver for the committed portable editor tool selection.
function Assert-BetaEditorPath([string]$Path,[bool]$Directory) {
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw 'Editor paths must be absolute' }
    $full=[IO.Path]::GetFullPath($Path)
    if ($full.TrimEnd('\') -ine $Path.TrimEnd('\')) { throw 'Editor path contains noncanonical components' }
    $item=Get-Item -LiteralPath $full -Force -ErrorAction Stop
    if ([bool]$item.PSIsContainer -ne $Directory) { throw 'Editor path has the wrong file type' }
    for ($current=$item; $current; $current=if ($current -is [IO.DirectoryInfo]) {$current.Parent} else {$current.Directory}) {
        if ($current.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Editor paths and ancestors must not be redirected' }
    }
    return $item.FullName
}
function Resolve-BetaEditor([Parameter(Mandatory)][string]$Code) {
    $pins=Get-Content -LiteralPath (Join-Path $PSScriptRoot '../../release/candidate-tools.json') -Raw | ConvertFrom-Json
    if ($pins.schema -cne 'vcp-candidate-tools/1' -or $pins.editor.version -cne '1.138.0' -or
        $pins.editor.commit -cnotmatch '^[a-f0-9]{40}$' -or $pins.editor.sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Exact supported editor pin required' }
    $codeFile=Assert-BetaEditorPath $Code $false
    if ([IO.Path]::GetFileName($codeFile) -cne 'Code.exe') { throw 'Select Code.exe at the portable editor root' }
    $root=Split-Path -Parent $codeFile
    $candidates=@($root)
    foreach ($directory in @(Get-ChildItem -LiteralPath $root -Directory -Force)) {
        $candidates+=Assert-BetaEditorPath $directory.FullName $true
    }
    $matches=@()
    foreach ($directory in $candidates) {
        $packageFile=Join-Path $directory 'resources/app/package.json'
        if (Test-Path -LiteralPath $packageFile) {
            $packageFile=Assert-BetaEditorPath $packageFile $false
            $package=Get-Content -LiteralPath $packageFile -Raw | ConvertFrom-Json
            if ($package.version -ceq $pins.editor.version) { $matches+=@{runtime=$directory;package=$packageFile} }
        }
    }
    if ($matches.Count -ne 1) { throw 'Exactly one matching pinned editor runtime is required' }
    $runtime=$matches[0].runtime
    $commitRuntime=Join-Path $root $pins.editor.commit.Substring(0,10)
    if ($runtime -ine $root -and $runtime -ine $commitRuntime) { throw 'Editor runtime is outside the reviewed flat or commit-prefix layout' }
    $app=Assert-BetaEditorPath (Join-Path $runtime 'resources/app') $true
    $productFile=Assert-BetaEditorPath (Join-Path $app 'product.json') $false
    $product=Get-Content -LiteralPath $productFile -Raw | ConvertFrom-Json
    if ($product.commit -cne $pins.editor.commit) { throw 'Exact supported editor commit required' }
    # Match the extension-host runner's DLL requirements before any installation.
    foreach ($relative in @('ffmpeg.dll','libEGL.dll','libGLESv2.dll','icudtl.dat','v8_context_snapshot.bin',
        'resources/app/out/cli.js','resources/app/out/main.js','resources/app/out/vs/workbench/workbench.desktop.main.js')) {
        $null=Assert-BetaEditorPath (Join-Path $runtime $relative) $false
    }
    return @{code=$codeFile;root=$root;runtime=$runtime;app=$app;cli=(Join-Path $app 'out/cli.js');
        version=$pins.editor.version;commit=$product.commit;archive_sha256=$pins.editor.sha256;
        code_sha256=(Get-FileHash -LiteralPath $codeFile).Hash.ToLowerInvariant();
        package_sha256=(Get-FileHash -LiteralPath $matches[0].package).Hash.ToLowerInvariant();
        product_sha256=(Get-FileHash -LiteralPath $productFile).Hash.ToLowerInvariant()}
}
