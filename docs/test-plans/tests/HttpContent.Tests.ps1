#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../VcpScenarioHarness.psm1') -Force -DisableNameChecking
$checks = 0
function Check($condition, $message) { if (-not $condition) { throw $message }; $script:checks++ }
$tokens = $null; $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '../scenario-b-aspnet-inventory.ps1'), [ref]$tokens, [ref]$parseErrors)
$getKeys = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-ErrorKeys' }, $true)
. ([scriptblock]::Create($getKeys.Extent.Text))
# A real loopback server reproduces Invoke-WebRequest's different Content types
# for application/problem+json versus application/json and text/plain.
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$server = Start-ThreadJob -ArgumentList $listener -ScriptBlock {
    param($listener)
    while ($true) {
        $client = $listener.AcceptTcpClient()
        try {
            $stream = $client.GetStream(); $stream.ReadTimeout = 5000; $stream.WriteTimeout = 5000
            $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::ASCII, $false, 1024, $true)
            $request = $reader.ReadLine()
            while ($reader.ReadLine()) { }
            $path = ($request -split ' ')[1]
            $status = 200; $type = 'application/json; charset=utf-8'
            $body = switch ($path) {
                '/problem' { $status = 400; $type = 'application/problem+json'; '{"errors":{"supplierId":["Fournisseur inconnu: café 🚫"]},"title":"Échec"}' }
                '/json' { $status = 201; '{"name":"café","count":2}' }
                '/text' { $status = 418; $type = 'text/plain; charset=utf-8'; 'Plain café 🚫' }
                '/malformed' { $status = 400; $type = 'application/problem+json'; '{"errors":' }
                '/array' { '[{"name":"café"},{"name":"茶"}]' }
                '/empty' { $status = 204; '' }
                default { throw 'Unexpected fixture request' }
            }
            $bytes = [Text.Encoding]::UTF8.GetBytes([string]$body)
            $header = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 $status Fixture`r`nContent-Type: $type`r`nContent-Length: $($bytes.Length)`r`nConnection: close`r`n`r`n")
            $stream.Write($header); $stream.Write($bytes); $stream.Flush()
            $reader.Dispose()
        }
        finally { $client.Dispose() }
    }
}
try {
    $base = "http://127.0.0.1:$port"
    $problem = Invoke-Http GET "$base/problem"
    Check ($problem.Status -eq 400) 'Problem response status changed'
    Check ($problem.Content -ceq '{"errors":{"supplierId":["Fournisseur inconnu: café 🚫"]},"title":"Échec"}') 'Problem JSON bytes were not decoded as UTF-8 text'
    Check ($problem.Json.title -ceq 'Échec' -and $problem.Json.errors.supplierId[0] -ceq 'Fournisseur inconnu: café 🚫') 'Non-ASCII problem JSON did not parse correctly'
    Check ((Get-ErrorKeys $problem.Json) -contains 'supplierid') 'Actual B supplier error gate cannot read problem JSON'
    Check (($problem.Headers['Content-Type'] -join '') -eq 'application/problem+json') 'Response headers changed'
    $json = Invoke-Http GET "$base/json"
    Check ($json.Status -eq 201 -and $json.Json.name -ceq 'café' -and $json.Json.count -eq 2) 'Existing string JSON response regressed'
    Check ($json.Content -ceq '{"name":"café","count":2}') 'String JSON content changed'
    $text = Invoke-Http GET "$base/text"
    Check ($text.Status -eq 418 -and $text.Content -ceq 'Plain café 🚫' -and $null -eq $text.Json) 'Plain-text status/content compatibility regressed'
    $bad = Invoke-Http GET "$base/malformed"
    Check ($bad.Status -eq 400 -and $bad.Content -ceq '{"errors":' -and $null -eq $bad.Json) 'Malformed JSON was accepted or its evidence changed'
    $array = Invoke-Http GET "$base/array"
    Check ($array.Status -eq 200 -and $array.Json.Count -eq 2 -and $array.Json[1].name -ceq '茶') 'JSON array response regressed'
    $empty = Invoke-Http GET "$base/empty"
    Check ($empty.Status -eq 204 -and $empty.Content -eq '' -and $null -eq $empty.Json) 'Empty response regressed'
    Write-Host "HTTP content checks passed: $checks (real loopback HTTP; no VCP/provider calls)."
}
finally {
    $listener.Stop()
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job $server -Force -ErrorAction SilentlyContinue
}
