# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path (Split-Path -Parent $PSScriptRoot) 'VcpScenarioHarness.psm1') -Force
. (Join-Path $PSScriptRoot 'scenario-contracts.ps1')
. (Join-Path $PSScriptRoot 'sql.ps1')
$count = 0
function Check([bool]$Value, [string]$Message) { if (-not $Value) { throw $Message }; $script:count++ }
foreach ($file in Get-ChildItem -LiteralPath $PSScriptRoot -Filter '*.ps1') {
    $tokens=$null;$errors=$null
    [void][Management.Automation.Language.Parser]::ParseFile($file.FullName,[ref]$tokens,[ref]$errors)
    Check ($errors.Count -eq 0) "PowerShell parse failure: $($file.Name): $errors"
}
foreach ($kind in 'A','B') {
    $selected = Get-OriginalScenarioContracts $kind
    Check (-not $selected.ToString().Contains('Initialize-VcpScenario')) 'Importer included the original top-level workflow.'
    Check (-not $selected.ToString().Contains('Invoke-VcpTask')) 'Importer included a paid workflow.'
    . $selected
    Check ($namesT5.Count -eq $(if($kind -eq 'A'){13}else{21})) 'Original cumulative named test requirements disappeared.'
    $originalPath = Join-Path (Split-Path -Parent $PSScriptRoot) $(if($kind -eq 'A'){'scenario-a-vue-taskboard.ps1'}else{'scenario-b-aspnet-inventory.ps1'})
    $tokens=$null;$errors=$null
    $original=[Management.Automation.Language.Parser]::ParseFile($originalPath,[ref]$tokens,[ref]$errors)
    foreach($fn in $selected.Ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$true)) {
        $name=$fn.Name
        $same=@($original.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name},$true))
        Check ($same.Count -eq 1 -and $same[0].Extent.Text -ceq $fn.Extent.Text) "Original gate changed: $name"
    }
}
# Load only the pure gate resolver; do not run run.ps1's top-level workflow.
$tokens=$null;$errors=$null
$runner=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'run.ps1'),[ref]$tokens,[ref]$errors)
$fn=$runner.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Require-Gates'},$true)
. ([scriptblock]::Create($fn.Extent.Text))
$fn=$runner.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-CurrentPauseVerification'},$true)
. ([scriptblock]::Create($fn.Extent.Text))
$fingerprint=@{repository=('a'*64);buffers=('b'*64);environment=('c'*64)}
$scope=@{task='task';session='session';workspace='workspace'}
$record=@{scope=$scope;steering=1;fingerprint=$fingerprint;checks=@(@{specification='required';outcome=@{status='passed'};exit_code=0});outputs=@('artifact');outstanding_issues=@();unresolved_effects=@()}
$bundle=@{task=@{scope=$scope;steering=1;fingerprint=$fingerprint;required_checks=@('required')};views=@{verification=@(@{items=@(@{collection='verification';visibility='available';record=$record})})}} | ConvertTo-Json -Depth 20 | ConvertFrom-Json
Check (@(Get-CurrentPauseVerification $bundle 'task').Count -eq 1) 'Current scoped successful verification must qualify.'
$bundle.task.fingerprint.repository='d'*64
Check (@(Get-CurrentPauseVerification $bundle 'task').Count -eq 0) 'Verification before a later source change must not qualify.'
$bundle.task.fingerprint.repository='a'*64;$bundle.task.steering=2
Check (@(Get-CurrentPauseVerification $bundle 'task').Count -eq 0) 'Old steering must not qualify.'
$bundle.task.steering=1;$bundle.task.required_checks=@('missing')
Check (@(Get-CurrentPauseVerification $bundle 'task').Count -eq 0) 'Incomplete check coverage must not qualify.'
$ctx=@{Gates=@(@{stage='E';id='integrity';required=$true;outcome='fail'},@{stage='E-repair1';id='integrity';required=$true;outcome='pass'})}
Require-Gates 'E';Check $true 'Repair supersedes only its original gate.'
$ctx.Gates += @{stage='E';id='browser';required=$true;outcome='skip'}
$refused=$false;try{Require-Gates 'E'}catch{$refused=$true};Check $refused 'Missing browser evidence must fail.'
$before=@{'dbo.Products'=@(@{Id=1;Name='Existing'});'dbo.__EFMigrationsHistory'=@(@{MigrationId='old'})}
$after=@{'dbo.Products'=@(@{Id=1;Name='Existing'});'dbo.__EFMigrationsHistory'=@(@{MigrationId='old'},@{MigrationId='new'})}
Test-DatabasePreserved $before $after -AllowMigrationAppend;Check $true 'Additive migration should preserve old rows.'
$after['dbo.Products'][0].Name='changed'
$refused=$false;try{Test-DatabasePreserved $before $after -AllowMigrationAppend}catch{$refused=$true};Check $refused 'Existing data change must fail.'
$after['dbo.Products'][0].Name='Existing'; $after['dbo.NewTable']=@()
$refused=$false;try{Test-DatabasePreserved $before $after}catch{$refused=$true};Check $refused 'Original database must not gain a table.'
Check (-not (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'sql.ps1') -Raw).Contains('WITH REPLACE')) 'Clone must not overwrite a database.'
$builder=[System.Data.Common.DbConnectionStringBuilder]::new()
$builder.set_ConnectionString('Server=(localdb)\MSSQLLocalDB;Database=Original;Integrated Security=True')
Check ($builder['Server'] -eq '(localdb)\MSSQLLocalDB' -and $builder['Database'] -eq 'Original') 'Protected connection must parse as actual keywords.'
$sqlSource=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'sql.ps1') -Raw
foreach($predicate in 'i.has_filter=0','i.is_disabled=0','i.is_hypothetical=0') { Check ($sqlSource.Contains($predicate)) "Missing global unique-index predicate: $predicate" }
$inventory = & node -e "process.stdout.write(JSON.stringify(require('./docs/test-plans/engagements/base-contract.cjs').final))" | ConvertFrom-Json
foreach($kind in 'A','B') {
    $file=Join-Path (Split-Path -Parent $PSScriptRoot) $(if($kind -eq 'A'){'scenario-a-vue-taskboard.ps1'}else{'scenario-b-aspnet-inventory.ps1'})
    $source=Get-Content -LiteralPath $file -Raw
    foreach($id in $inventory.$kind) {
        if($id -like 'ui.form-*') { Check ($source.Contains("Id = '$($id.Substring(8))'")) "Missing original dynamic gate $id" }
        else { Check ($source.Contains("-Id '$id'")) "Missing original gate source $id" }
    }
}
Write-Output "$count offline engagement contract assertions passed; no application, model or SQL process was launched."
