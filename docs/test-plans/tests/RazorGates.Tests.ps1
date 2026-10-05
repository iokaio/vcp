#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
$scenarioRoot = Split-Path -Parent $PSScriptRoot
Import-Module (Join-Path $scenarioRoot 'VcpScenarioHarness.psm1') -Force -DisableNameChecking
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $scenarioRoot 'scenario-b-aspnet-inventory.ps1'), [ref]$tokens, [ref]$errors)
Assert-That ($errors.Count -eq 0) 'Scenario B must parse'
foreach ($name in 'Assert-RazorFieldError', 'Test-RazorInputCase', 'Assert-RazorLowStockTable') {
    $definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
    . ([scriptblock]::Create($definition.Extent.Text))
}
$script:checks = 0
function Check([scriptblock]$Test, [bool]$Expected, [string]$Name) {
    $passed = $true; $failure = ''
    try { & $Test | Out-Null } catch { $passed = $false; $failure = $_.Exception.Message }
    Assert-That ($passed -eq $Expected) "$Name ($failure)"
    $script:checks++
}
$validError = '<span data-valmsg-for="Input.Sku" class="text-danger field-validation-error">SKU is invalid</span>'
Check { Assert-RazorFieldError $validError 'Input.Sku' } $true 'Real field error'
Check { Assert-RazorFieldError ("<div class='form-group'><label>SKU</label>$validError</div>") 'Input.Sku' } $true 'Field message nested in ordinary form container'
Check { Assert-RazorFieldError '<span class="text-danger field-validation-valid" data-valmsg-for="Input.Sku"></span><input required>' 'Input.Sku' } $false 'Generic CSS and required markup are not validation evidence'
Check { Assert-RazorFieldError $validError 'Input.Name' } $false 'Wrong field cannot satisfy isolated rule'
Check { Assert-RazorFieldError '<span data-valmsg-for="Input.Sku" class="field-validation-error"> </span>' 'Input.Sku' } $false 'Empty error must fail'
Check { Assert-RazorFieldError "<span class='field-validation-error' data-valmsg-for='Input.Sku'><b>Invalid &amp; rejected</b></span>" 'Input.Sku' } $true 'Attribute order and nested message markup'

function Table([string[]]$Skus) { '<table><thead><tr><th>Name</th><th>SKU</th></tr></thead><tbody>' + (($Skus | ForEach-Object { "<tr><td>Item</td><td><a>$_</a></td></tr>" }) -join '') + '</tbody></table>' }
Check { Assert-RazorLowStockTable (Table @('LA-1', 'LZ-1')) @('LA-1', 'LZ-1') } $true 'Full ordered table parity'
Check { Assert-RazorLowStockTable (Table @('LZ-1')) @('LA-1', 'LZ-1') } $false 'Equality row cannot be omitted'
Check { Assert-RazorLowStockTable (Table @('LA-1', 'LM-1', 'LZ-1')) @('LA-1', 'LZ-1') } $false 'Above threshold row cannot be included'
Check { Assert-RazorLowStockTable (Table @('LZ-1', 'LA-1')) @('LA-1', 'LZ-1') } $false 'Name order must fail SKU order'
Check { Assert-RazorLowStockTable (Table @('LA-1', 'LA-1', 'LZ-1')) @('LA-1', 'LZ-1') } $false 'Duplicate rows cannot pass'
Check { Assert-RazorLowStockTable '<p>LA-1 LZ-1</p>' @('LA-1', 'LZ-1') } $false 'Text outside table is insufficient'

$script:posted = $false; $script:insert = $false; $script:status = 200; $script:errorHtml = $validError
$script:zero = 0; $script:countStatus = 200
function Invoke-Http($Method, $Uri, $Body, $Session, $ContentType) {
    if ($Uri.EndsWith('/Products/Create') -and $Method -eq 'GET') { return @{ Status = 200; Content = '<input name="__RequestVerificationToken" value="token">' } }
    if ($Method -eq 'POST') {
        Assert-That ($Body['__RequestVerificationToken'] -eq 'token' -and $null -ne $Session) 'Token and cookie session required'
        $script:posted = $true
        $script:fields = $Body
        return @{ Status = $script:status; Content = $script:errorHtml }
    }
    if ($Uri -like '*search=*') {
        $items = if ($script:insert) { @(@{ sku = $script:fields['Input.Sku']; name = $script:fields['Input.Name']; unitPrice = $script:zero; reorderLevel = $script:zero }) } else { @() }
        return @{ Status = 200; Json = @{ items = $items } }
    }
    return @{ Status = $script:countStatus; Json = @{ totalCount = $(if ($script:posted -and $script:insert) { 11 } else { 10 }) } }
}
function InputCase([string]$ErrorField = 'Input.Sku') {
    $script:posted = $false
    Test-RazorInputCase 'http://fixture.invalid' ([Microsoft.PowerShell.Commands.WebRequestSession]::new()) 'BAD@SKU' 'Valid name' '1' $ErrorField
}
Check { InputCase } $true 'Invalid submission with real field error and no insertion'
$script:insert = $true
Check { InputCase } $false 'Error HTML must not mask insertion'
$script:insert = $false; $script:status = 400
Check { InputCase } $false 'Antiforgery rejection is not validation'
$script:status = 200; $script:errorHtml = '<span class="text-danger">error</span>'
Check { InputCase } $false 'Unassociated error rejected'
$script:errorHtml = $validError; $script:insert = $true
Check { InputCase '' } $true 'Valid zero price and reorder insertion'
$script:zero = 1
Check { InputCase '' } $false 'Silently coerced zero values rejected'
$script:zero = 0; $script:insert = $false
Check { InputCase '' } $false 'Rejected valid zero form fails'
$script:countStatus = 500
Check { InputCase } $false 'Missing persisted-state evidence fails'

# Exercise the actual low-stock gate with threshold-sensitive HTTP fixture responses.
$command = $ast.Find({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Invoke-Gate' -and "'ui.low-stock-parity'" -in $node.CommandElements.Extent.Text }, $true)
$gate = @($command.CommandElements | Where-Object { $_ -is [Management.Automation.Language.ScriptBlockExpressionAst] })[0].ScriptBlock.GetScriptBlock()
$base = 'http://fixture.invalid'; $tag = 'FIXTURE'; $session = [Microsoft.PowerShell.Commands.WebRequestSession]::new()
$script:products = @(); $script:omitEquality = $false; $script:includeAbove = $false; $script:pageReverse = $false
function Invoke-Http($Method, $Uri, $Body, $Session, $ContentType) {
    if ($Uri.EndsWith('/api/suppliers')) { return @{ Status = 200; Json = @(@{ id = 1 }) } }
    if ($Uri.EndsWith('/api/products') -and $Method -eq 'POST') {
        $script:products += $Body; return @{ Status = 201; Json = @{ id = $script:products.Count } }
    }
    if ($Uri -match '/api/products/(\d+)/movements$') {
        $script:products[[int]$Matches[1] - 1].quantity = $Body.quantity
        return @{ Status = 201 }
    }
    $report = @($script:products | Where-Object { ($_.quantity -lt $_.reorderLevel -or (-not $script:omitEquality -and $_.quantity -eq $_.reorderLevel)) -or $script:includeAbove } | Sort-Object sku)
    if ($Uri.EndsWith('/api/reports/low-stock')) { return @{ Status = 200; Json = $report } }
    if ($Uri.EndsWith('/Reports/LowStock')) {
        $skus = @($report | ForEach-Object sku)
        if ($script:pageReverse) { [array]::Reverse($skus) }
        return @{ Status = 200; Content = (Table $skus) }
    }
    throw "Unexpected fixture request $Method $Uri"
}
function LowStockCase { $script:products = @(); & $gate }
Check { LowStockCase } $true 'Real gate passes <= fixtures and parity'
$script:omitEquality = $true
Check { LowStockCase } $false 'Both API and page wrong with strict less-than still fail'
$script:omitEquality = $false; $script:includeAbove = $true
Check { LowStockCase } $false 'Both API and page include above threshold still fail'
$script:includeAbove = $false; $script:pageReverse = $true
Check { LowStockCase } $false 'Page wrong order fails even when API correct'
Write-Host "Razor gate checks passed: $script:checks"
