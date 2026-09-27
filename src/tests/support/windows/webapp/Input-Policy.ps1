# SPDX-License-Identifier: Apache-2.0
# Definitions only. Registry/ACL inspection is called solely by explicit execution.
function Assert-NoPolicyValueNames([string[]]$Present) {
    foreach ($name in @('iokaio.vcp.cs3.webview2.probe','WebViewHost.exe','*')) {
        if ($Present -contains $name) { throw 'Applicable WebView2 policy override rejected (value not read)' }
    }
}
function Assert-NoWritableRuntimeRule([string]$Principal,[int]$Rights,[string]$Type,[string]$ContainerSid='') {
    $mask=[int][Security.AccessControl.FileSystemRights]'Write,Delete,DeleteSubdirectoriesAndFiles,ChangePermissions,TakeOwnership'
    if ($Type -eq 'Allow' -and $Principal -in @('S-1-1-0','S-1-5-11','S-1-5-32-545','S-1-15-2-1','S-1-15-2-2',$ContainerSid) -and ($Rights -band $mask)) { throw 'Runtime ACL permits broad or container writes' }
}
function Assert-NoWebViewOverrides {
    foreach ($entry in [Environment]::GetEnvironmentVariables().Keys) {
        if ([string]$entry -match '^(WEBVIEW2_|COREWEBVIEW2_MAX_INSTANCES$)') { throw 'Ambient WebView2 override rejected (value not logged)' }
    }
    $names=@('iokaio.vcp.cs3.webview2.probe','WebViewHost.exe','*')
    $subkeys=@('BrowserExecutableFolder','UserDataFolder','AdditionalBrowserArguments','ChannelSearchKind','ReleaseChannels','ReleaseChannelPreference')
    foreach ($hive in @([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryHive]::CurrentUser)) {
        foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)) {
            $base=$null
            try {
                $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey($hive,$view)
                foreach ($subkey in $subkeys) {
                    $key=$null
                    try {
                        $key=$base.OpenSubKey(('Software\Policies\Microsoft\Edge\WebView2\'+$subkey),$false)
                        if ($null -ne $key) { Assert-NoPolicyValueNames $key.GetValueNames() }
                    } finally { if ($null -ne $key) { $key.Dispose() } }
                }
                foreach ($name in $names) {
                    $key=$null
                    try { $key=$base.OpenSubKey(('Software\Policies\Microsoft\EmbeddedBrowserWebView\LoaderOverride\'+$name),$false); if ($null -ne $key) { throw 'Legacy loader override rejected (value not read)' } }
                    finally { if ($null -ne $key) { $key.Dispose() } }
                }
            } finally { if ($null -ne $base) { $base.Dispose() } }
        }
    }
}
function Get-InputSnapshot([string]$Root,[switch]$CheckRuntimeAcl,[string]$ContainerSid='') {
    $full=[IO.Path]::GetFullPath($Root).TrimEnd('\')
    if ($full -notmatch '^[A-Za-z]:\\.+') { throw 'Exact local input root required' }
    for ($ancestor=$full; $ancestor; $ancestor=[IO.Path]::GetDirectoryName($ancestor)) {
        if (([IO.File]::GetAttributes($ancestor) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Redirected input ancestor' }
    }
    $pending=[Collections.Generic.Queue[string]]::new(); $pending.Enqueue($full)
    $entries=[Collections.Generic.List[object]]::new(); $files=[Collections.Generic.List[object]]::new(); [long]$bytes=0; $count=0
    while ($pending.Count) {
        $directory=$pending.Dequeue()
        foreach ($path in @($directory)+@([IO.Directory]::EnumerateFileSystemEntries($directory))) {
            if (([IO.File]::GetAttributes($path) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Redirected input entry' }
            if ($path -ne $directory -and ([IO.File]::GetAttributes($path) -band [IO.FileAttributes]::Directory)) { $pending.Enqueue($path); continue }
            if (++$count -gt 8192) { throw 'Input entry ceiling' }
            $relative=if ($path -eq $full) { '.' } else { [IO.Path]::GetRelativePath($full,$path).Replace('\','/') }
            if ($relative.Length -gt 2048 -or ($relative.Split('/').Count -gt 16) -or $relative.StartsWith('../')) { throw 'Input path bound' }
            $attr=[IO.File]::GetAttributes($path)
            if ($attr -band [IO.FileAttributes]::ReparsePoint) { throw 'Redirected input entry' }
            $isDirectory=[bool]($attr -band [IO.FileAttributes]::Directory)
            $acl=Get-Acl -LiteralPath $path
            if ($CheckRuntimeAcl) {
                foreach ($ace in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
                    Assert-NoWritableRuntimeRule $ace.IdentityReference.Value ([int]$ace.FileSystemRights) ([string]$ace.AccessControlType) $ContainerSid
                }
            }
            $aclHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($acl.Sddl))).ToLowerInvariant()
            $entry=[ordered]@{path=$relative;directory=$isDirectory;acl_sha256=$aclHash}
            if (-not $isDirectory) {
                $length=(Get-Item -LiteralPath $path).Length; $bytes+=$length
                if ($files.Count -ge 4096 -or $length -gt 1GB -or $bytes -gt 4GB) { throw 'Input file/byte ceiling' }
                $entry.bytes=$length; $entry.sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
                $files.Add([ordered]@{path=$relative;bytes=$length;sha256=$entry.sha256})
            }
            $entries.Add($entry)
        }
    }
    return [ordered]@{root=$full;files=@($files | Sort-Object path);entries=@($entries | Sort-Object path);bytes=$bytes;serviced_input=[bool]$CheckRuntimeAcl}
}
function Assert-SnapshotSame($Before,$After) {
    if (($Before | ConvertTo-Json -Depth 8 -Compress) -cne ($After | ConvertTo-Json -Depth 8 -Compress)) { throw 'Runtime/host inventory, bytes or ACL changed; observation invalid' }
}
