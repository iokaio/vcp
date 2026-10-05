# SPDX-License-Identifier: Apache-2.0
function Get-OriginalScenarioContracts([ValidateSet('A', 'B')][string]$Kind) {
    # Parse only this repository's trusted acceptance implementation. Never dot-source
    # a generated app or execute the scenario's top-level scaffold/paid workflow.
    $name = if ($Kind -eq 'A') { 'scenario-a-vue-taskboard.ps1' } else { 'scenario-b-aspnet-inventory.ps1' }
    $file = Join-Path (Split-Path -Parent $PSScriptRoot) $name
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw "Invalid original scenario script: $file" }
    $functions = if ($Kind -eq 'A') {
        @('Invoke-Npm', 'Get-Tail', 'Test-Typecheck', 'Test-NodeTests', 'Test-UnitAndBuild', 'Start-Api', 'New-DataFile',
          'Test-ApiContract', 'Test-LabelsAndSort', 'Test-Production', 'Test-ProtectedUnchanged', 'New-NodeCheck')
    } else {
        @('New-InventoryConnection', 'Invoke-Dotnet', 'Get-Tail', 'Get-Solution', 'Test-Build', 'Test-Tests',
          'Test-Migrations', 'Start-App', 'Get-ErrorKeys', 'Test-ApiContract', 'Assert-RazorFieldError',
          'Test-RazorInputCase', 'Assert-RazorLowStockTable', 'Test-RazorPages', 'Test-Concurrency',
          'Invoke-RuntimeGates', 'Test-ProtectedUnchanged', 'New-DotnetCheck')
    }
    $variables = if ($Kind -eq 'A') { @('namesT1', 'namesT3', 'namesT4', 'namesT5', 'uiIds') }
        else { @('seededSkus', 'namesT1', 'namesT2', 'namesT3', 'regressionNames', 'namesT4', 'namesT5') }
    $selected = [Collections.Generic.List[object]]::new()
    foreach ($name in $functions) {
        $matches = @($ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true))
        if ($matches.Count -ne 1) { throw "Expected exactly one original function: $name" }
        $selected.Add($matches[0])
    }
    foreach ($name in $variables) {
        $matches = @($ast.FindAll({ param($node)
            $node -is [Management.Automation.Language.AssignmentStatementAst] -and
            $node.Left -is [Management.Automation.Language.VariableExpressionAst] -and $node.Left.VariablePath.UserPath -eq $name
        }, $true))
        if ($matches.Count -ne 1) { throw "Expected exactly one original contract variable: $name" }
        $selected.Add($matches[0])
    }
    return [scriptblock]::Create((($selected | Sort-Object { $_.Extent.StartOffset } | ForEach-Object { $_.Extent.Text }) -join "`n`n"))
}
