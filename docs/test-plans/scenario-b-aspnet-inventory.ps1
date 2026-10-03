#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Scenario B - Contoso Inventory: ASP.NET Core Razor Pages + RESTful API in C#,
EF Core migrations on SQL Server (LocalDB by default), built over multiple VCP
CLI turns and assessed with deterministic build, test, database and HTTP gates.

.DESCRIPTION
See docs/test-plans/cli-test-plans.md, "Scenario B". The script scaffolds a
solution with the real dotnet templates, then runs VCP turns for the data model
and migrations, the REST API, the Razor Pages UI, protected regression tests,
optimistic concurrency (interrupted by a short deadline and resumed with
'vcp resume <task>'), and a plan-mode review. It finishes with dotnet publish,
a smoke test of the published executable and results/scorecard.json.

.EXAMPLE
pwsh -File .\scenario-b-aspnet-inventory.ps1 -ProviderGeneration C:\vcp-private\provider-20261002
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ProviderGeneration,
    [string]$RunRoot = (Join-Path $env:SystemDrive 'vcp-scenarios'),
    [string]$ProjectPath,
    [string]$Vcp,
    [decimal]$TurnBudgetUsd = 3,
    [decimal]$MaxScenarioUsd = 30,
    [int]$MaxRepairTurns = 1,
    [int]$OutputTokens = 8192,
    [int]$MaxRequests = 96,
    [int]$DeadlineSeconds = 1800,
    [int]$ShortDeadlineSeconds = 150,
    [ValidateRange(1, 65535)][int]$AppPort = 41750,
    [ValidateRange(1, 65535)][int]$PublishedPort = 41751,
    # Optional: use an existing SQL Server with integrated authentication. This value is
    # model-visible and included in checkpoints/publish output; credentials are forbidden.
    [string]$SqlConnectionString,
    [switch]$DropDatabase,
    [switch]$AllowProcessPublish,
    [switch]$SkipPaidStages
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force

$ctx = Initialize-VcpScenario -Name 'b-aspnet-inventory' -RunRoot $RunRoot -ProjectPath $ProjectPath -Vcp $Vcp -ProviderGeneration $ProviderGeneration `
    -TurnBudgetUsd $TurnBudgetUsd -MaxScenarioUsd $MaxScenarioUsd -MaxRepairTurns $MaxRepairTurns -OutputTokens $OutputTokens `
    -MaxRequests $MaxRequests -DeadlineSeconds $DeadlineSeconds -ShortDeadlineSeconds $ShortDeadlineSeconds -AllowProcessPublish:$AllowProcessPublish -SkipPaidStages:$SkipPaidStages
$ws = $ctx.Workspace
$webProject = 'src/Inventory.Web/Inventory.Web.csproj'
$testProject = 'tests/Inventory.Tests/Inventory.Tests.csproj'

#region Toolchain

$dotnet = Find-Executable -Name 'dotnet' -Candidates @("$env:ProgramFiles\dotnet\dotnet.exe")
if (-not $dotnet) { throw '.NET SDK (dotnet.exe) 8 or later is required.' }
$sdkMajors = @(& $dotnet --list-sdks | ForEach-Object { if ($_ -match '^(\d+)\.\d+\.\d+\s') { [int]$Matches[1] } } | Sort-Object -Unique)
if (-not $sdkMajors -or $sdkMajors[-1] -lt 8) { throw '.NET SDK 8 or later is required.' }
$major = $sdkMajors[-1]
$tfm = "net$major.0"

function Get-LatestNuGetVersion([string]$Id, [int]$Major) {
    $index = Invoke-RestMethod -Uri ("https://api.nuget.org/v3-flatcontainer/{0}/index.json" -f $Id.ToLowerInvariant()) -TimeoutSec 60
    $stable = @($index.versions | Where-Object { $_ -match "^$Major\.\d+\.\d+$" } | Sort-Object { [version]$_ })
    if (-not $stable) { throw "No stable $Id $Major.x on NuGet." }
    return $stable[-1]
}

function New-InventoryConnection([string]$ConnectionString, [string]$Database) {
    $builder = [System.Data.Common.DbConnectionStringBuilder]::new()
    # PowerShell treats property assignment on this IDictionary as adding a key.
    $builder.set_ConnectionString($ConnectionString)
    foreach ($key in @($builder.Keys)) {
        if ([string]$key -match '(?i)^(password|pwd|access\s*token|token)$') {
            throw 'SqlConnectionString must not contain credentials: it is stored in the model-visible workspace and published artifacts. Use integrated authentication.'
        }
    }
    $integrated = @('Integrated Security', 'Trusted_Connection') | Where-Object {
        $builder.ContainsKey($_) -and [string]$builder[$_] -match '^(?i:true|yes|sspi)$'
    }
    if (-not $integrated) { throw 'SqlConnectionString must use integrated authentication (Integrated Security=True or Trusted_Connection=True).' }
    foreach ($key in 'Initial Catalog', 'Database') { [void]$builder.Remove($key) }
    $builder['Database'] = $Database
    return $builder.get_ConnectionString()
}

$database = 'VcpInventory_' + ($ctx.RunId -replace '[^A-Za-z0-9]', '_')
$useLocalDb = -not $SqlConnectionString
if ($useLocalDb) {
    $localDbCandidates = @(Get-ChildItem "$env:ProgramFiles\Microsoft SQL Server\*\Tools\Binn\SqlLocalDB.exe" -ErrorAction SilentlyContinue | Sort-Object FullName | ForEach-Object FullName)
    $sqlLocalDb = Find-Executable -Name 'SqlLocalDB' -Candidates ($localDbCandidates | Sort-Object -Descending)
    if (-not $sqlLocalDb) { throw 'SQL Server LocalDB (SqlLocalDB.exe) not found. Install it or pass -SqlConnectionString.' }
    $connection = "Server=(localdb)\MSSQLLocalDB;Database=$database;Trusted_Connection=True;MultipleActiveResultSets=true;TrustServerCertificate=True"
}
else {
    $connection = New-InventoryConnection $SqlConnectionString $database
}
$sqlcmd = Find-Executable -Name 'sqlcmd'

function Invoke-Dotnet([string]$Stage, [string]$Label, [string[]]$Arguments, [int]$TimeoutSeconds = 1200) {
    return Invoke-Tool -Ctx $ctx -Stage $Stage -Label $Label -FilePath $dotnet -ArgumentList $Arguments -TimeoutSeconds $TimeoutSeconds `
        -Environment @{ ASPNETCORE_ENVIRONMENT = 'Development'; DOTNET_CLI_TELEMETRY_OPTOUT = '1'; DOTNET_NOLOGO = '1'; ConnectionStrings__Inventory = $connection }
}
function Get-Tail([string]$Text, [int]$Count = 40) { return (($Text -split "`r?`n") | Select-Object -Last $Count) -join "`n" }
function Get-Solution { return (Get-ChildItem -LiteralPath $ws -File | Where-Object { $_.Extension -in '.sln', '.slnx' } | Select-Object -First 1).FullName }

#endregion

#region Seed content

$readme = @'
# Contoso Inventory

Inventory management for a small electronics distributor: products, suppliers
and stock movements, with a Razor Pages UI and a RESTful JSON API in one
ASP.NET Core application backed by SQL Server through EF Core.

## Layout

- `src/Inventory.Web` - ASP.NET Core app: Razor Pages UI (`Pages/`), REST API under `/api`,
  EF Core `Data/InventoryDbContext` and `Data/Migrations`.
- `tests/Inventory.Tests` - xUnit tests. API tests use `WebApplicationFactory<Program>` in the
  `Testing` environment with the EF Core InMemory provider instead of SQL Server.

## Database

The connection string named `Inventory` lives in `src/Inventory.Web/appsettings.Development.json`
(SQL Server LocalDB for local development). The schema is managed with EF Core migrations and the
local `dotnet-ef` tool (`.config/dotnet-tools.json`):

    dotnet tool run dotnet-ef migrations add <Name> --project src/Inventory.Web
    dotnet tool run dotnet-ef database update --project src/Inventory.Web

The application never migrates automatically at startup.
'@
$appsettingsDevelopment = @'
{
  "ConnectionStrings": {
    "Inventory": "{{CONNECTION}}"
  },
  "Logging": {
    "LogLevel": {
      "Default": "Information",
      "Microsoft.AspNetCore": "Warning",
      "Microsoft.EntityFrameworkCore.Database.Command": "Warning"
    }
  }
}
'@
$appsettingsDevelopment = $appsettingsDevelopment.Replace('"{{CONNECTION}}"', (ConvertTo-Json -InputObject $connection -Compress))

$regressionTests = @'
// PROTECTED FILE - added by the scenario harness as an acceptance test. Do not edit.
using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using Inventory.Web.Data;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.DependencyInjection;
using Xunit;

namespace Inventory.Tests;

public sealed class RegressionFactory : WebApplicationFactory<Program>
{
    private readonly string _database = "regressions-" + Guid.NewGuid().ToString("N");

    protected override void ConfigureWebHost(IWebHostBuilder builder)
    {
        builder.UseEnvironment("Testing");
        builder.ConfigureServices(services =>
        {
            var registrations = services.Where(d =>
                d.ServiceType == typeof(InventoryDbContext) ||
                (d.ServiceType.IsGenericType && d.ServiceType.GenericTypeArguments.Contains(typeof(InventoryDbContext))))
                .ToList();
            foreach (var registration in registrations)
            {
                services.Remove(registration);
            }
            services.AddDbContext<InventoryDbContext>(options => options.UseInMemoryDatabase(_database));
        });
    }
}

public sealed class RegressionTests(RegressionFactory factory) : IClassFixture<RegressionFactory>
{
    private readonly HttpClient _client = factory.CreateClient();

    private async Task<int> CreateSupplierAsync(string name)
    {
        var response = await _client.PostAsJsonAsync("/api/suppliers", new { name, email = $"{Guid.NewGuid():N}@example.test" });
        Assert.Equal(HttpStatusCode.Created, response.StatusCode);
        var body = await response.Content.ReadFromJsonAsync<JsonElement>();
        return body.GetProperty("id").GetInt32();
    }

    private async Task<JsonElement> CreateProductAsync(string sku, int supplierId)
    {
        var response = await _client.PostAsJsonAsync("/api/products",
            new { sku, name = "Regression " + sku.Trim(), unitPrice = 1.50m, reorderLevel = 5, supplierId });
        Assert.Equal(HttpStatusCode.Created, response.StatusCode);
        return await response.Content.ReadFromJsonAsync<JsonElement>();
    }

    [Fact]
    public async Task Sku_is_trimmed_and_uppercased_on_create()
    {
        var supplier = await CreateSupplierAsync("Regression Supplier A");
        var product = await CreateProductAsync("  ab-123 ", supplier);
        Assert.Equal("AB-123", product.GetProperty("sku").GetString());
    }

    [Fact]
    public async Task Sale_exceeding_stock_returns_insufficient_stock_problem()
    {
        var supplier = await CreateSupplierAsync("Regression Supplier B");
        var product = await CreateProductAsync("REG-SALE-1", supplier);
        var id = product.GetProperty("id").GetInt32();
        var receipt = await _client.PostAsJsonAsync($"/api/products/{id}/movements", new { quantity = 5, reason = "Receipt" });
        Assert.Equal(HttpStatusCode.Created, receipt.StatusCode);
        var sale = await _client.PostAsJsonAsync($"/api/products/{id}/movements", new { quantity = -10, reason = "Sale" });
        Assert.Equal(HttpStatusCode.Conflict, sale.StatusCode);
        var problem = await sale.Content.ReadFromJsonAsync<JsonElement>();
        Assert.Equal("urn:inventory:insufficient-stock", problem.GetProperty("type").GetString());
        var stock = await _client.GetFromJsonAsync<JsonElement>($"/api/products/{id}/stock");
        Assert.Equal(5, stock.GetProperty("onHand").GetInt32());
    }

    [Fact]
    public async Task Whitespace_only_product_name_is_rejected()
    {
        var supplier = await CreateSupplierAsync("Regression Supplier C");
        var response = await _client.PostAsJsonAsync("/api/products",
            new { sku = "REG-NAME-1", name = "   ", unitPrice = 1m, reorderLevel = 1, supplierId = supplier });
        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode);
    }

    [Fact]
    public async Task Supplier_email_must_be_valid()
    {
        var response = await _client.PostAsJsonAsync("/api/suppliers", new { name = "Regression Supplier D", email = "not-an-email" });
        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode);
    }
}
'@

#endregion

#region Prompts

$seededSkus = @('CBL-HDMI-2M', 'CBL-USB-C-1M', 'CBL-USB-C-2M', 'HUB-USB-C-7', 'KB-MECH-TKL', 'MS-WL-ERGO', 'PWR-100W-GAN', 'PWR-65W-GAN', 'PWR-BANK-20K', 'SSD-NVME-1TB')

$environmentBlock = @'

## Environment and rules (applies to every task in this project)

- Work only inside the current workspace. Read README.md and the existing code first.
- Process profile available to `vcp_exec` (no shell; literal arguments): `dotnet` runs the
  .NET {{MAJOR}} SDK. Examples: `["build", "{{SOLUTION}}"]`, `["test", "{{SOLUTION}}"]`,
  `["tool", "run", "dotnet-ef", "migrations", "add", "<Name>", "--project", "src/Inventory.Web",
  "--output-dir", "Data/Migrations", "--", "--environment", "Development",
  "--ConnectionStrings:Inventory", {{CONNECTION_JSON}}]`.
  The process profile only supplies VCP's allowed public bootstrap environment. For every EF
  command, forward the Development environment and this run's connection string as shown above;
  never target an existing application's database. The harness applies migrations itself.
- Target framework {{TFM}}; EF Core and ASP.NET Core packages {{EFVERSION}} are already referenced.
  Add a package only if essential, pinned to an exact version, and explain why.
- Keep `appsettings.Development.json` and its `Inventory` connection string unchanged.
- Never apply migrations automatically at startup; the harness runs
  `dotnet tool run dotnet-ef database update` itself.
- Startup must not require a connection string or database in the `Testing` environment; tests
  replace the DbContext registration with EF Core InMemory via `WebApplicationFactory<Program>`.
- Protected files (never edit, rename or delete): `src/Inventory.Web/appsettings.Development.json`{{PROTECTED}}.
- Before finishing, build the solution and run all tests, and fix any failure. Finish with a short
  summary of changed files, migrations added and command results.
'@

function New-Prompt([string]$Body, [string]$Protected = '') {
    if ($ctx.ReuseProject -and (Test-Path -LiteralPath (Join-Path $ws 'tests/Inventory.Tests/RegressionTests.cs'))) {
        $Protected = '`, `tests/Inventory.Tests/RegressionTests.cs`'
    }
    $block = $environmentBlock.Replace('{{MAJOR}}', [string]$major).Replace('{{TFM}}', $tfm).Replace('{{EFVERSION}}', $efVersion).Replace('{{SOLUTION}}', (Split-Path -Leaf (Get-Solution)))
    $block = $block.Replace('{{CONNECTION_JSON}}', (ConvertTo-Json -InputObject $connection -Compress))
    return $Body + $block.Replace('{{PROTECTED}}', $Protected)
}

#endregion

#region Gates

function Test-Build([string]$Stage) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'build' -Description 'dotnet build of the solution succeeds' -Test {
            $run = Invoke-Dotnet $Stage 'build' @('build', (Get-Solution), '-c', 'Debug', '-nologo')
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + "`n" + $run.Errors) 50))
            $true })
}

function Test-Tests([string]$Stage, [int]$MinTests, [string[]]$Required = @()) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'dotnet-test' -Description "dotnet test passes with >= $MinTests tests$(if ($Required) { ' incl. ' + ($Required -join ', ') })" -Test {
            $results = Join-Path $ctx.Logs "$Stage\testresults-$([guid]::NewGuid().ToString('N'))"
            $run = Invoke-Dotnet $Stage 'test' @('test', (Get-Solution), '--no-build', '--logger', 'trx', '--results-directory', $results)
            $trx = @(Get-ChildItem -LiteralPath $results -Filter '*.trx' -Recurse -ErrorAction SilentlyContinue)
            Assert-That ($trx.Count -gt 0) ("no TRX produced (exit {0})`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $testResults = @($trx | ForEach-Object {
                $report = [xml](Get-Content -LiteralPath $_.FullName -Raw)
                $report.TestRun.Results.UnitTestResult
            })
            $failedNames = @($testResults | Where-Object { $_.outcome -ne 'Passed' } | ForEach-Object testName)
            Assert-That ($run.ExitCode -eq 0 -and $failedNames.Count -eq 0) ("exit {0}; failed or skipped: {1}" -f $run.ExitCode, ($failedNames -join '; '))
            $passed = @($testResults | Where-Object { $_.outcome -eq 'Passed' } | ForEach-Object testName)
            Assert-That ($passed.Count -ge $MinTests) "only $($passed.Count) passing tests"
            $missing = @($Required | Where-Object { $name = $_; -not ($passed | Where-Object { $_ -like "*$name" }) })
            Assert-That ($missing.Count -eq 0) ('required tests not passing: ' + ($missing -join ', '))
            $true })
}

function Test-Migrations([string]$Stage, [string[]]$Names) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'migrations' -Description "migrations present: $($Names -join ', '); database update applies them" -Test {
            $list = Invoke-Dotnet $Stage 'ef-migrations-list' @('tool', 'run', 'dotnet-ef', 'migrations', 'list', '--project', 'src/Inventory.Web', '--no-build')
            Assert-That ($list.ExitCode -eq 0) ("migrations list exit {0}`n{1}" -f $list.ExitCode, (Get-Tail ($list.Output + $list.Errors)))
            $missing = @($Names | Where-Object { $list.Output -notmatch "_$_\b" })
            Assert-That ($missing.Count -eq 0) ('missing migrations: ' + ($missing -join ', '))
            $update = Invoke-Dotnet $Stage 'ef-database-update' @('tool', 'run', 'dotnet-ef', 'database', 'update', '--project', 'src/Inventory.Web', '--no-build')
            Assert-That ($update.ExitCode -eq 0) ("database update exit {0}`n{1}" -f $update.ExitCode, (Get-Tail ($update.Output + $update.Errors)))
            $true })
    if ($sqlcmd -and $useLocalDb) {
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'db.seed' -Description 'database holds the 10 seeded products' -Advisory -Test {
                $run = Invoke-Tool -Ctx $ctx -Stage $Stage -Label 'sqlcmd-count' -FilePath $sqlcmd -ArgumentList @(
                    '-S', '(localdb)\MSSQLLocalDB', '-d', $database, '-h', '-1', '-W', '-Q',
                    "SET NOCOUNT ON; SELECT COUNT(*) FROM Products WHERE Sku IN ('$($seededSkus -join "','")')")
                Assert-That ($run.ExitCode -eq 0 -and $run.Output.Trim() -eq '10') "sqlcmd exit $($run.ExitCode): $($run.Output.Trim()) $($run.Errors.Trim())"; $true })
    }
    else { [void](Skip-Gate $ctx $Stage 'db.seed' 'database holds the 10 seeded products' 'sqlcmd not available or not LocalDB') }
}

function Start-App([string]$Stage, [string]$Label, [int]$Port, [switch]$Published) {
    $environment = @{ ASPNETCORE_ENVIRONMENT = 'Development'; DOTNET_CLI_TELEMETRY_OPTOUT = '1'; ASPNETCORE_URLS = "http://127.0.0.1:$Port"; ConnectionStrings__Inventory = $connection }
    if ($Published) {
        $directory = Join-Path $ws 'artifacts\publish'
        return Start-BackgroundServer -Ctx $ctx -Stage $Stage -Label $Label -FilePath (Join-Path $directory 'Inventory.Web.exe') `
            -WorkingDirectory $directory -Environment $environment -ReadyUrl "http://127.0.0.1:$Port/api/suppliers" -ReadySeconds 90
    }
    return Start-BackgroundServer -Ctx $ctx -Stage $Stage -Label $Label -FilePath $dotnet `
        -ArgumentList @('run', '--project', $webProject, '--no-build', '--no-launch-profile', '--urls', "http://127.0.0.1:$Port") `
        -Environment $environment -ReadyUrl "http://127.0.0.1:$Port/api/suppliers" -ReadySeconds 120
}

function Get-ErrorKeys($Json) {
    if (-not $Json -or -not $Json.errors) { return @() }
    return @($Json.errors.PSObject.Properties.Name | ForEach-Object { $_.ToLowerInvariant() })
}

function Test-ApiContract([string]$Stage, [int]$Port) {
    $base = "http://127.0.0.1:$Port"
    $tag = [guid]::NewGuid().ToString('N').Substring(0, 16).ToUpperInvariant()
    $api = @{}
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.suppliers' -Description 'GET /api/suppliers lists the 3 seeded suppliers by name' -Test {
            $r = Invoke-Http GET "$base/api/suppliers"
            $names = @($r.Json | ForEach-Object name)
            Assert-That ($r.Status -eq 200 -and $names -contains 'Contoso Cables' -and $names -contains 'Fabrikam Power' -and $names -contains 'Northwind Components') "status $($r.Status) body $($r.Content)"
            Assert-That (($names -join '|') -eq (($names | Sort-Object) -join '|')) 'suppliers not sorted by name'
            $api.supplier = [int](@($r.Json | Where-Object name -eq 'Contoso Cables')[0].id); $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.paging' -Description 'GET /api/products?page=1&pageSize=5 pages, totals and sku order' -Test {
            $r = Invoke-Http GET "$base/api/products?page=1&pageSize=5"
            Assert-That ($r.Status -eq 200 -and @($r.Json.items).Count -eq 5 -and [int]$r.Json.page -eq 1 -and [int]$r.Json.pageSize -eq 5 -and [int]$r.Json.totalCount -ge 10) "status $($r.Status) body $($r.Content)"
            $all = Invoke-Http GET "$base/api/products?page=1&pageSize=100"
            $order = @($all.Json.items | ForEach-Object sku | Where-Object { $seededSkus -contains $_ })
            Assert-That (($order -join ',') -eq ($seededSkus -join ',')) "seeded SKU order: $($order -join ',')"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.page-size-limit' -Description 'pageSize=101 -> 400' -Test {
            $r = Invoke-Http GET "$base/api/products?pageSize=101"
            Assert-That ($r.Status -eq 400) "status $($r.Status)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.search' -Description 'search=cable returns the 3 cable products only' -Test {
            $r = Invoke-Http GET "$base/api/products?search=cable&pageSize=100"
            $skus = @($r.Json.items | ForEach-Object sku)
            Assert-That ($r.Status -eq 200 -and ($skus -join ',') -eq 'CBL-HDMI-2M,CBL-USB-C-1M,CBL-USB-C-2M') "skus: $($skus -join ',')"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.create' -Description 'POST /api/products -> 201 with Location' -Test {
            $r = Invoke-Http POST "$base/api/products" @{ sku = "H-$tag-01"; name = 'Harness Widget'; unitPrice = 19.95; reorderLevel = 10; supplierId = $api.supplier }
            Assert-That ($r.Status -eq 201 -and [int]$r.Json.id -gt 0 -and $r.Json.sku -eq "H-$tag-01") "status $($r.Status) body $($r.Content)"
            Assert-That (([string]$r.Headers['Location']) -match "/api/products/$($r.Json.id)$") "Location '$($r.Headers['Location'])'"
            $api.product = [int]$r.Json.id; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.duplicate-sku' -Description 'duplicate SKU -> 409 problem details' -Test {
            $r = Invoke-Http POST "$base/api/products" @{ sku = "H-$tag-01"; name = 'Duplicate'; unitPrice = 1; reorderLevel = 1; supplierId = $api.supplier }
            Assert-That ($r.Status -eq 409) "status $($r.Status) body $($r.Content)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.validation' -Description 'invalid SKU and negative price -> 400 with sku and unitPrice errors' -Test {
            $r = Invoke-Http POST "$base/api/products" @{ sku = 'bad sku!'; name = 'X'; unitPrice = -1; reorderLevel = 0; supplierId = $api.supplier }
            $keys = Get-ErrorKeys $r.Json
            Assert-That ($r.Status -eq 400 -and ($keys -match 'sku') -and ($keys -match 'unitprice')) "status $($r.Status) keys [$($keys -join ',')]"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.unknown-supplier' -Description 'unknown supplierId -> 400 with supplierId error' -Test {
            $r = Invoke-Http POST "$base/api/products" @{ sku = "H-$tag-09"; name = 'Orphan'; unitPrice = 1; reorderLevel = 1; supplierId = 999999 }
            Assert-That ($r.Status -eq 400 -and ((Get-ErrorKeys $r.Json) -match 'supplierid')) "status $($r.Status) body $($r.Content)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.movements' -Description 'receipt 50, sale -20 -> stock onHand 30; invalid movements -> 400' -Test {
            $a = Invoke-Http POST "$base/api/products/$($api.product)/movements" @{ quantity = 50; reason = 'Receipt' }
            $b = Invoke-Http POST "$base/api/products/$($api.product)/movements" @{ quantity = -20; reason = 'Sale' }
            Assert-That ($a.Status -eq 201 -and $b.Status -eq 201) "receipt $($a.Status) $($a.Content); sale $($b.Status) $($b.Content)"
            $stock = Invoke-Http GET "$base/api/products/$($api.product)/stock"
            Assert-That ($stock.Status -eq 200 -and [int]$stock.Json.onHand -eq 30) "stock $($stock.Content)"
            $zero = Invoke-Http POST "$base/api/products/$($api.product)/movements" @{ quantity = 0; reason = 'Adjustment' }
            $negativeReceipt = Invoke-Http POST "$base/api/products/$($api.product)/movements" @{ quantity = -5; reason = 'Receipt' }
            Assert-That ($zero.Status -eq 400 -and $negativeReceipt.Status -eq 400) "zero $($zero.Status), negative receipt $($negativeReceipt.Status)"
            $list = Invoke-Http GET "$base/api/products/$($api.product)/movements"
            Assert-That ($list.Status -eq 200 -and @($list.Json).Count -eq 2 -and [int]$list.Json[0].quantity -eq -20) "movements newest-first: $($list.Content)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.low-stock' -Description 'low-stock report lists seeded SKUs in order and excludes the restocked product' -Test {
            $r = Invoke-Http GET "$base/api/reports/low-stock"
            $skus = @($r.Json | ForEach-Object sku)
            $seededOrder = @($skus | Where-Object { $seededSkus -contains $_ })
            Assert-That ($r.Status -eq 200 -and ($seededOrder -join ',') -eq ($seededSkus -join ',')) "seeded order: $($seededOrder -join ',')"
            Assert-That (-not ($skus -contains "H-$tag-01")) 'restocked product listed as low stock'; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.delete' -Description 'DELETE with movements -> 409; without -> 204 then 404' -Test {
            $conflict = Invoke-Http DELETE "$base/api/products/$($api.product)"
            $fresh = Invoke-Http POST "$base/api/products" @{ sku = "H-$tag-02"; name = 'Disposable'; unitPrice = 2; reorderLevel = 1; supplierId = $api.supplier }
            $deleted = Invoke-Http DELETE "$base/api/products/$($fresh.Json.id)"
            $gone = Invoke-Http GET "$base/api/products/$($fresh.Json.id)"
            Assert-That ($conflict.Status -eq 409 -and $deleted.Status -eq 204 -and $gone.Status -eq 404) "conflict $($conflict.Status), delete $($deleted.Status), get $($gone.Status)"; $true })
}

function Test-RazorPages([string]$Stage, [int]$Port) {
    $base = "http://127.0.0.1:$Port"
    $tag = [guid]::NewGuid().ToString('N').Substring(0, 16).ToUpperInvariant()
    $session = [Microsoft.PowerShell.Commands.WebRequestSession]::new()
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'ui.index' -Description 'GET /Products lists products and filters with ?search=cable' -Test {
            $all = Invoke-Http GET "$base/Products" -Session $session
            $filtered = Invoke-Http GET "$base/Products?search=cable" -Session $session
            Assert-That ($all.Status -eq 200 -and $all.Content.Contains('CBL-USB-C-1M')) "index status $($all.Status)"
            Assert-That ($filtered.Status -eq 200 -and $filtered.Content.Contains('CBL-USB-C-1M') -and -not $filtered.Content.Contains('SSD-NVME-1TB')) 'search filter not applied'; $true })
    $state = @{}
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'ui.create' -Description 'Create form with antiforgery token creates a product (Input.* fields)' -Test {
            $form = Invoke-Http GET "$base/Products/Create" -Session $session
            $token = [regex]::Match($form.Content, 'name="__RequestVerificationToken"[^>]*value="(?<t>[^"]+)"').Groups['t'].Value
            Assert-That ($form.Status -eq 200 -and $token) 'create page or antiforgery token missing'
            $state.token = $token
            $suppliers = (Invoke-Http GET "$base/api/suppliers").Json
            $fields = @{ 'Input.Sku' = "UI-$tag"; 'Input.Name' = 'Form Created Gadget'; 'Input.UnitPrice' = '12.50'; 'Input.ReorderLevel' = '3'
                'Input.SupplierId' = [string]$suppliers[0].id; '__RequestVerificationToken' = $token }
            $post = Invoke-Http POST "$base/Products/Create" $fields -Session $session -ContentType 'application/x-www-form-urlencoded'
            Assert-That ($post.Status -eq 200) "post status $($post.Status)"
            $found = Invoke-Http GET "$base/api/products?search=UI-$tag&pageSize=100"
            $match = @($found.Json.items | Where-Object sku -eq "UI-$tag")
            Assert-That ($match.Count -eq 1) 'product created through the form not found via API'
            $state.id = [int]$match[0].id; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'ui.validation' -Description 'invalid form re-renders with field validation errors' -Test {
            $form = Invoke-Http GET "$base/Products/Create" -Session $session
            $token = [regex]::Match($form.Content, 'name="__RequestVerificationToken"[^>]*value="(?<t>[^"]+)"').Groups['t'].Value
            $fields = @{ 'Input.Sku' = "UI-$tag-B"; 'Input.Name' = ''; 'Input.UnitPrice' = '-3'; 'Input.ReorderLevel' = '1'; 'Input.SupplierId' = '1'; '__RequestVerificationToken' = $token }
            $post = Invoke-Http POST "$base/Products/Create" $fields -Session $session -ContentType 'application/x-www-form-urlencoded'
            Assert-That ($post.Status -eq 200 -and $post.Content -match 'field-validation-error') "status $($post.Status); no field-validation-error"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'ui.details' -Description 'Details page shows the product and a Record movement form' -Test {
            $r = Invoke-Http GET "$base/Products/Details/$($state.id)" -Session $session
            Assert-That ($r.Status -eq 200 -and $r.Content.Contains("UI-$tag") -and $r.Content -match 'Record movement') "status $($r.Status)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'ui.low-stock' -Description 'GET /Reports/LowStock renders seeded low-stock SKUs' -Test {
            $r = Invoke-Http GET "$base/Reports/LowStock" -Session $session
            Assert-That ($r.Status -eq 200 -and $r.Content.Contains('SSD-NVME-1TB')) "status $($r.Status)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'ui.antiforgery' -Description 'form POST without antiforgery token is rejected (400)' -Test {
            $r = Invoke-Http POST "$base/Products/Create" @{ 'Input.Sku' = "UI-$tag-C"; 'Input.Name' = 'No token' } -ContentType 'application/x-www-form-urlencoded'
            Assert-That ($r.Status -eq 400) "status $($r.Status)"; $true })
}

function Test-Concurrency([string]$Stage, [int]$Port) {
    $base = "http://127.0.0.1:$Port"
    $tag = [guid]::NewGuid().ToString('N').Substring(0, 16).ToUpperInvariant()
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'etag.flow' -Description 'ETag/If-Match: 428 without, 412 stale, 200 current with a new ETag' -Test {
            $supplier = [int]((Invoke-Http GET "$base/api/suppliers").Json[0].id)
            $created = Invoke-Http POST "$base/api/products" @{ sku = "E-$tag"; name = 'Etag Probe'; unitPrice = 5; reorderLevel = 1; supplierId = $supplier }
            Assert-That ($created.Status -eq 201 -and [int]$created.Json.id -gt 0) 'could not create ETag probe product'
            $id = [int]$created.Json.id
            $get = Invoke-Http GET "$base/api/products/$id"
            $etag = [string]$get.Headers['ETag']
            Assert-That ($get.Status -eq 200 -and $etag -match '^"[^"\r\n]+"$') 'GET did not return a strong quoted ETag'
            $body = @{ sku = "E-$tag"; name = 'Etag Renamed'; unitPrice = 6; reorderLevel = 2; supplierId = $supplier }
            $none = Invoke-Http PUT "$base/api/products/$id" $body
            Assert-That ($none.Status -eq 428) "PUT without If-Match -> $($none.Status)"
            $staleTag = if ($etag -eq '"AAAAAAAAB9E="') { '"AAAAAAAAB9I="' } else { '"AAAAAAAAB9E="' }
            $stale = Invoke-Http PUT "$base/api/products/$id" $body -Headers @{ 'If-Match' = $staleTag }
            Assert-That ($stale.Status -eq 412) "PUT with bogus ETag -> $($stale.Status)"
            $ok = Invoke-Http PUT "$base/api/products/$id" $body -Headers @{ 'If-Match' = $etag }
            $newTag = [string]$ok.Headers['ETag']
            Assert-That ($ok.Status -eq 200 -and $ok.Json.name -eq 'Etag Renamed' -and $newTag -match '^"[^"\r\n]+"$' -and $newTag -ne $etag) "PUT current -> $($ok.Status), new ETag '$newTag'"
            $replay = Invoke-Http PUT "$base/api/products/$id" $body -Headers @{ 'If-Match' = $etag }
            Assert-That ($replay.Status -eq 412) "replayed old ETag -> $($replay.Status)"; $true })
}

function Invoke-RuntimeGates([string]$Stage, [scriptblock]$Body, [int]$Port = $AppPort, [switch]$Published) {
    $server = $null
    try {
        $server = Start-App $Stage 'inventory-web' $Port -Published:$Published
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'app.start' -Description 'application starts against SQL Server' -Outcome 'pass' -Required $true)
    }
    catch {
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'app.start' -Description 'application starts against SQL Server' -Outcome 'fail' -Detail $_.Exception.Message -Required $true)
        return
    }
    try { & $Body $Port } finally { Stop-BackgroundServer $server }
}

function Test-ProtectedUnchanged([string]$Stage, [hashtable]$Hashes) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'protected-files' -Description 'protected files are byte-identical' -Test {
            foreach ($path in $Hashes.Keys) {
                $full = Join-Path $ws $path
                Assert-That (Test-Path -LiteralPath $full) "$path deleted"
                Assert-That ((Get-Sha256 $full) -eq $Hashes[$path]) "$path modified"
            }
            $true })
}

#endregion

$exitCode = 1
try {
    Invoke-CommonPreflight $ctx

    # --- B0: scaffold with the real dotnet templates ---------------------
    $stage = 'B0-baseline'
    Write-Step $ctx "B0 scaffold ($tfm), restore, LocalDB, baseline build" 'phase'
    if ($ctx.ReuseProject) {
        foreach ($required in @($webProject, $testProject, '.config/dotnet-tools.json', 'src/Inventory.Web/appsettings.Development.json')) {
            if (-not (Test-Path -LiteralPath (Join-Path $ws $required) -PathType Leaf)) {
                throw "Existing project is not an Inventory scenario project: missing $required. Select its workspace directory or a new empty project directory."
            }
        }
        if (-not (Get-Solution)) { throw 'Existing Inventory project has no solution (.sln or .slnx).' }
        [xml]$existingProject = Get-Content -LiteralPath (Join-Path $ws $webProject) -Raw
        $frameworkNode = $existingProject.SelectSingleNode('//TargetFramework')
        if (-not $frameworkNode -or $frameworkNode.InnerText -notmatch '^net(\d+)\.0$') { throw 'Existing Inventory project requires a single net8.0 or later target framework.' }
        $tfm = $frameworkNode.InnerText
        $targetMajor = [int]$Matches[1]
        if ($targetMajor -lt 8 -or $targetMajor -gt $major) { throw "Existing target $tfm is not supported by installed SDK $major." }
        $efPackage = $existingProject.SelectSingleNode('//PackageReference[@Include="Microsoft.EntityFrameworkCore.SqlServer"]')
        $efVersion = if ($efPackage -and $efPackage.GetAttribute('Version')) { $efPackage.GetAttribute('Version') } else { '(existing project versions)' }
        Write-Step $ctx "Reusing Inventory project: $ws (existing source, configuration, and dependency versions retained)." 'ok'
        $ctx.Notes.Add("Existing project configuration is preserved. Harness database commands and HTTP tests use isolated database '$database' via ConnectionStrings__Inventory.")
    }
    else {
        $efVersion = Get-LatestNuGetVersion 'Microsoft.EntityFrameworkCore.SqlServer' $major
        $testingVersion = Get-LatestNuGetVersion 'Microsoft.AspNetCore.Mvc.Testing' $major
        $efToolVersion = Get-LatestNuGetVersion 'dotnet-ef' $major
        $ctx.Notes.Add("Pinned EF Core $efVersion, Mvc.Testing $testingVersion, dotnet-ef $efToolVersion for $tfm.")
    }
    if ($useLocalDb) {
        $info = Invoke-Tool -Ctx $ctx -Stage $stage -Label 'localdb-info' -FilePath $sqlLocalDb -ArgumentList @('info', 'MSSQLLocalDB')
        if ($info.ExitCode -ne 0) { [void](Invoke-Tool -Ctx $ctx -Stage $stage -Label 'localdb-create' -FilePath $sqlLocalDb -ArgumentList @('create', 'MSSQLLocalDB')) }
        $start = Invoke-Tool -Ctx $ctx -Stage $stage -Label 'localdb-start' -FilePath $sqlLocalDb -ArgumentList @('start', 'MSSQLLocalDB')
        if ($start.ExitCode -ne 0) { throw "LocalDB failed to start: $($start.Errors)" }
    }
    if (-not $ctx.ReuseProject) {
        $steps = @(
            @('new-sln', @('new', 'sln', '-n', 'Inventory', '-o', $ws)),
            @('new-webapp', @('new', 'webapp', '-n', 'Inventory.Web', '-o', (Join-Path $ws 'src\Inventory.Web'), '-f', $tfm)),
            @('new-xunit', @('new', 'xunit', '-n', 'Inventory.Tests', '-o', (Join-Path $ws 'tests\Inventory.Tests'), '-f', $tfm)),
            @('new-gitignore', @('new', 'gitignore', '-o', $ws)),
            @('new-tool-manifest', @('new', 'tool-manifest', '-o', $ws)))
        foreach ($step in $steps) {
            $run = Invoke-Dotnet $stage $step[0] $step[1]
            if ($run.ExitCode -ne 0) { throw "dotnet $($step[0]) failed:`n$(Get-Tail ($run.Output + $run.Errors))" }
        }
        foreach ($step in @(
                @('sln-add', @('sln', (Get-Solution), 'add', (Join-Path $ws $webProject), (Join-Path $ws $testProject))),
                @('add-reference', @('add', (Join-Path $ws $testProject), 'reference', (Join-Path $ws $webProject))),
                @('add-ef-sqlserver', @('add', (Join-Path $ws $webProject), 'package', 'Microsoft.EntityFrameworkCore.SqlServer', '--version', $efVersion)),
                @('add-ef-design', @('add', (Join-Path $ws $webProject), 'package', 'Microsoft.EntityFrameworkCore.Design', '--version', $efVersion)),
                @('add-mvc-testing', @('add', (Join-Path $ws $testProject), 'package', 'Microsoft.AspNetCore.Mvc.Testing', '--version', $testingVersion)),
                @('add-ef-inmemory', @('add', (Join-Path $ws $testProject), 'package', 'Microsoft.EntityFrameworkCore.InMemory', '--version', $efVersion)),
                @('tool-install-ef', @('tool', 'install', 'dotnet-ef', '--version', $efToolVersion)))) {
            $run = Invoke-Dotnet $stage $step[0] $step[1]
            if ($run.ExitCode -ne 0) { throw "dotnet $($step[0]) failed:`n$(Get-Tail ($run.Output + $run.Errors))" }
        }
        Write-Utf8File (Join-Path $ws 'README.md') $readme
        Write-Utf8File (Join-Path $ws 'src\Inventory.Web\appsettings.Development.json') $appsettingsDevelopment
        Add-Content -LiteralPath (Join-Path $ws '.gitignore') -Value "`nartifacts/`n" -Encoding utf8NoBOM
    }
    else {
        foreach ($restore in @(@('tool-restore', @('tool', 'restore')), @('restore', @('restore', (Get-Solution))))) {
            $run = Invoke-Dotnet $stage $restore[0] $restore[1]
            if ($run.ExitCode -ne 0) { throw "dotnet $($restore[0]) failed:`n$(Get-Tail ($run.Output + $run.Errors))" }
        }
    }
    Test-Build $stage
    Test-Tests $stage 1
    if ((Get-FailedGates $ctx $stage).Count) { throw 'Baseline solution does not build/test; fix the .NET toolchain before spending on VCP turns.' }
    Initialize-GitCheckpoint $ctx
    $protected = @{ 'src/Inventory.Web/appsettings.Development.json' = (Get-Sha256 (Join-Path $ws 'src\Inventory.Web\appsettings.Development.json')) }
    if (Test-Path -LiteralPath (Join-Path $ws 'tests/Inventory.Tests/RegressionTests.cs')) {
        $protected['tests/Inventory.Tests/RegressionTests.cs'] = Get-Sha256 (Join-Path $ws 'tests/Inventory.Tests/RegressionTests.cs')
    }

    # --- Prompts that depend on the scaffold ----------------------------
    $promptT1 = New-Prompt @'
# Task T1 - Domain model, EF Core and the initial migration

In `src/Inventory.Web`, add the data layer for Contoso Inventory.

- Namespace `Inventory.Web.Data`, class `InventoryDbContext` with `DbSet`s named `Products`,
  `Suppliers` and `StockMovements` (tables of the same names). Integer identity keys named `Id`.
- `Supplier`: Name (required, max 100, unique), Email (required, valid address, max 200).
- `Product`: Sku (required, 3-32 chars, `^[A-Z0-9-]+$`, unique), Name (required, max 120),
  UnitPrice (decimal(18,2), >= 0), ReorderLevel (int >= 0), SupplierId (required FK).
- `StockMovement`: ProductId (FK), Quantity (int, non-zero), Reason (enum Receipt, Sale,
  Adjustment stored as string, max 20), OccurredAt (UTC), Note (optional, max 200).
- Register the context with `UseSqlServer(builder.Configuration.GetConnectionString("Inventory"))`
  in `Program.cs`, and add `public partial class Program;` so tests can use
  `WebApplicationFactory<Program>`.
- Seed with `HasData` exactly these suppliers (Id, Name, Email): 1 Northwind Components
  orders@northwind.example; 2 Contoso Cables sales@contoso-cables.example; 3 Fabrikam Power
  support@fabrikam-power.example; and these products (Id, Sku, Name, UnitPrice, ReorderLevel,
  SupplierId): 1 CBL-USB-C-1M "USB-C Cable 1m" 9.99 25 2; 2 CBL-USB-C-2M "USB-C Cable 2m" 12.49 20 2;
  3 CBL-HDMI-2M "HDMI Cable 2m" 14.99 15 2; 4 PWR-65W-GAN "65W GaN Charger" 39.00 10 3;
  5 PWR-100W-GAN "100W GaN Charger" 59.00 8 3; 6 PWR-BANK-20K "Power Bank 20000mAh" 49.95 6 3;
  7 KB-MECH-TKL "Mechanical Keyboard TKL" 89.00 5 1; 8 MS-WL-ERGO "Wireless Ergonomic Mouse" 54.50 5 1;
  9 HUB-USB-C-7 "USB-C Hub 7-in-1" 44.99 7 1; 10 SSD-NVME-1TB "NVMe SSD 1TB" 79.99 4 1.
- Create the migration `InitialCreate` in `Data/Migrations` with the local dotnet-ef tool.
- Add unit tests in `tests/Inventory.Tests` for model validation rules (at least 3 tests).
'@
    $promptT2 = New-Prompt @'
# Task T2 - RESTful JSON API

Add the REST API (controllers with `[ApiController]` or minimal APIs - your choice, consistently)
under `/api`. JSON uses camelCase and enums as strings. Errors use RFC 7807 problem details;
validation failures are 400 validation problem details whose `errors` keys name the fields.

- `GET /api/suppliers` -> array of `{id,name,email}` ordered by name.
  `POST /api/suppliers` -> 201; duplicate name -> 409.
- `GET /api/products?search=&page=1&pageSize=20` -> `{items,page,pageSize,totalCount}`;
  search is a case-insensitive match on SKU or name; items ordered by SKU; pageSize 1-100,
  page >= 1, otherwise 400. Item shape `{id,sku,name,unitPrice,reorderLevel,supplierId}`.
- `GET /api/products/{id}` -> 200 or 404. `POST /api/products` -> 201 with a Location header
  `/api/products/{id}`; duplicate SKU -> 409; unknown supplierId -> 400 with a `supplierId` error.
  `PUT /api/products/{id}` -> 200 with the updated product. `DELETE /api/products/{id}` -> 204,
  404, or 409 when the product has stock movements.
- `POST /api/products/{id}/movements` with `{quantity,reason,note?}` -> 201 movement
  `{id,productId,quantity,reason,occurredAt,note}`. Quantity must be non-zero; Receipt must be
  positive and Sale negative, otherwise 400. `GET /api/products/{id}/movements` -> newest first.
- `GET /api/products/{id}/stock` -> `{productId,sku,onHand}` (sum of movement quantities).
- `GET /api/reports/low-stock` -> array of `{productId,sku,name,onHand,reorderLevel}` for products
  with onHand <= reorderLevel, ordered by SKU.
- Integration tests with `WebApplicationFactory<Program>` in environment `Testing` using EF Core
  InMemory (at least 8 tests covering the endpoints above).
'@
    $promptT3 = New-Prompt @'
# Task T3 - Razor Pages UI

Build the Razor Pages UI on the same services as the API (no HTTP calls from pages to the API):

- `/Products` - table of products (SKU, name, price, reorder level, supplier, on hand) with a
  `search` query-string filter and paging; link to details and create.
- `/Products/Create` and `/Products/Edit/{id}` - forms bound to `[BindProperty] public ProductInput Input`
  so field names are `Input.Sku`, `Input.Name`, `Input.UnitPrice`, `Input.ReorderLevel`,
  `Input.SupplierId` (supplier drop-down). Server-side validation with the same rules as the API,
  validation messages next to fields (`asp-validation-for`), antiforgery tokens, and on success a
  redirect to the product details page. Duplicate SKU shows a model error on `Input.Sku`.
- `/Products/Details/{id}` - product facts, on-hand stock, movement history newest first, and a
  form with the heading text `Record movement` (quantity, reason, note) that posts to the page.
- `/Reports/LowStock` - the low-stock report as a table.
- Add the pages to the layout navigation. Add tests for the page model logic (at least 3 tests).
'@
    $promptT4 = New-Prompt @'
# Task T4 - Make the protected regression tests pass

A teammate added `tests/Inventory.Tests/RegressionTests.cs`. It describes required behavior that
currently fails. Make every test in it pass by changing the application only; the test file is
protected. Expected behavior: SKUs are trimmed and upper-cased before validation and storage
(API and UI); product names are trimmed and whitespace-only names are invalid; supplier email must
be a valid address; a movement that would make on-hand stock negative is rejected with 409 and a
problem details `type` of `urn:inventory:insufficient-stock`. Keep all other tests passing.
'@ '`, `tests/Inventory.Tests/RegressionTests.cs`'
    $promptT5 = New-Prompt @'
# Task T5 - Optimistic concurrency for product updates

Prevent lost updates when two clerks edit the same product:

- Add a SQL Server `rowversion` concurrency token to `Product` and a migration named
  `AddProductRowVersion`.
- `GET /api/products/{id}` returns a strong `ETag` header derived from the row version.
- `PUT /api/products/{id}` requires `If-Match`: missing -> 428 problem details; not matching the
  current version -> 412 problem details; matching -> 200 with the updated product and a new ETag.
- The Razor `Edit` page carries the row version in a hidden field and shows a clear concurrency
  error (model error) when the product changed since it was loaded.
- Tests for the 428/412/200 paths (InMemory does not enforce rowversion; simulate the check in
  your concurrency logic or test it at the service level, and say which).
'@ '`, `tests/Inventory.Tests/RegressionTests.cs`'
    $promptReview = @'
# Task T6 - Read-only engineering review

Do not modify, create or delete any file. Review this solution before its first production
deployment: data access and query efficiency (N+1, missing indexes), validation consistency between
the API and the Razor Pages, concurrency handling, security (antiforgery, over-posting, error detail
leakage), migrations, and test coverage. Cite file paths and line numbers.

End your answer with one fenced ```json block of the form
{"findings":[{"severity":"high|medium|low","file":"path","line":n,"title":"...","recommendation":"..."}]}
listing at most 10 findings ordered by severity.
'@

    # --- Profiles ------------------------------------------------------------
    $stage = 'P1-profiles'
    $dotnetProcess = New-ProcessProfile -Name 'dotnet' -Executable $dotnet -Ctx $ctx -MaxTimeoutMs 1200000
    $affected = @('README.md', 'src', 'tests')
    $profileMain = New-ScenarioProfile -Ctx $ctx -Name 'profile-main' -AffectedPaths $affected -Processes @($dotnetProcess)
    $profileShort = New-ScenarioProfile -Ctx $ctx -Name 'profile-short' -AffectedPaths $affected -Processes @($dotnetProcess) -DeadlineSeconds $ctx.ShortDeadlineSeconds
    $profileReview = New-ScenarioProfile -Ctx $ctx -Name 'profile-review' -AffectedPaths $affected -MaximumAutonomy 'plan' -AutomaticEffects @('read')
    $profileUntrusted = New-ScenarioProfile -Ctx $ctx -Name 'profile-untrusted' -AffectedPaths $affected -Processes @($dotnetProcess) -TrustWorkspace $false -Guardrail
    foreach ($pair in @(@('main', $profileMain), @('short', $profileShort), @('review', $profileReview))) { [void](Test-ProfileCheck $ctx $stage $pair[1] $pair[0]) }

    # --- G0: zero-spend guardrail: untrusted workspace ---------------------
    $guardPrompt = Join-Path $ctx.Logs 'G0-guardrail\prompt.md'
    Write-Utf8File $guardPrompt 'Guardrail probe. This task must be rejected before execution.'
    Invoke-GuardrailRun -Ctx $ctx -Stage 'G0-guardrail' -Id 'untrusted-workspace' -Config $profileUntrusted `
        -Description 'run with a profile lacking trust_workspace is rejected, exit 2, no task' `
        -Arguments @('run', '--file', $guardPrompt, '--budget-usd', '0.01', '--autonomy', 'autonomous') -ExpectStderr 'trust'

    if ($ctx.SkipPaidStages) {
        Write-Step $ctx 'Dry run (-SkipPaidStages): toolchain, seed, baseline, profiles and guardrail verified; stopping before paid stages.' 'ok'
        throw 'VCP_SCENARIO_DRY_RUN_COMPLETE'
    }

    # --- T1 ----------------------------------------------------------------
    $gatesT1 = { param($s) Test-Build $s; Test-Tests $s 4; Test-Migrations $s @('InitialCreate'); Test-ProtectedUnchanged $s $protected }
    $t1 = Invoke-VcpTask -Ctx $ctx -Stage 'T1-data' -Title 'Domain model, EF Core, InitialCreate' -Prompt $promptT1 -Config $profileMain
    if ($t1) { Test-StageExit $ctx $t1 'T1-data'; & $gatesT1 'T1-data'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T1-data' -Config $profileMain -GateScript $gatesT1) }
    Save-Checkpoint $ctx 'T1: data model and InitialCreate'

    # --- T2 ----------------------------------------------------------------
    $gatesT2 = { param($s) Test-Build $s; Test-Tests $s 12; Test-Migrations $s @('InitialCreate'); Test-ProtectedUnchanged $s $protected; Invoke-RuntimeGates $s { param($p) Test-ApiContract $s $p } }
    $t2 = Invoke-VcpTask -Ctx $ctx -Stage 'T2-api' -Title 'RESTful JSON API' -Prompt $promptT2 -Config $profileMain
    if ($t2) { Test-StageExit $ctx $t2 'T2-api'; & $gatesT2 'T2-api'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T2-api' -Config $profileMain -GateScript $gatesT2) }
    Save-Checkpoint $ctx 'T2: REST API'

    # --- T3 ----------------------------------------------------------------
    $gatesT3 = { param($s) Test-Build $s; Test-Tests $s 15; Test-Migrations $s @('InitialCreate'); Test-ProtectedUnchanged $s $protected; Invoke-RuntimeGates $s { param($p) Test-RazorPages $s $p; Test-ApiContract $s $p } }
    $t3 = Invoke-VcpTask -Ctx $ctx -Stage 'T3-razor' -Title 'Razor Pages UI' -Prompt $promptT3 -Config $profileMain
    if ($t3) { Test-StageExit $ctx $t3 'T3-razor'; & $gatesT3 'T3-razor'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T3-razor' -Config $profileMain -GateScript $gatesT3) }
    Save-Checkpoint $ctx 'T3: Razor Pages UI'

    # --- T4: protected regression tests -----------------------------------
    Write-SeedFiles -Root $ws -Files @{ 'tests/Inventory.Tests/RegressionTests.cs' = $regressionTests } -MissingOnly
    Save-Checkpoint $ctx 'T4 setup: protected regression tests added by harness'
    if (-not $protected.ContainsKey('tests/Inventory.Tests/RegressionTests.cs')) {
        $protected['tests/Inventory.Tests/RegressionTests.cs'] = Get-Sha256 (Join-Path $ws 'tests/Inventory.Tests/RegressionTests.cs')
    }
    $regressionNames = @('Sku_is_trimmed_and_uppercased_on_create', 'Sale_exceeding_stock_returns_insufficient_stock_problem', 'Whitespace_only_product_name_is_rejected', 'Supplier_email_must_be_valid')
    $gatesT4 = { param($s) Test-Build $s; Test-Tests $s 19 $regressionNames; Test-ProtectedUnchanged $s $protected; Invoke-RuntimeGates $s { param($p) Test-ApiContract $s $p; Test-RazorPages $s $p } }
    $t4 = Invoke-VcpTask -Ctx $ctx -Stage 'T4-regressions' -Title 'Make protected regression tests pass' -Prompt $promptT4 -Config $profileMain
    if ($t4) { Test-StageExit $ctx $t4 'T4-regressions'; & $gatesT4 'T4-regressions'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T4-regressions' -Config $profileMain -GateScript $gatesT4) }
    Save-Checkpoint $ctx 'T4: regression fixes'

    # --- T5: concurrency, short deadline then 'resume <task>' --------------
    $gatesT5 = { param($s) Test-Build $s; Test-Tests $s 22 $regressionNames; Test-Migrations $s @('InitialCreate', 'AddProductRowVersion'); Test-ProtectedUnchanged $s $protected; Invoke-RuntimeGates $s { param($p) Test-Concurrency $s $p; Test-ApiContract $s $p; Test-RazorPages $s $p } }
    $t5 = Invoke-VcpTask -Ctx $ctx -Stage 'T5-concurrency' -Title 'Optimistic concurrency (short deadline)' -Prompt $promptT5 -Config $profileShort -AcceptExit @(0, 3, 8)
    if ($t5) {
        Test-StageExit $ctx $t5 'T5-concurrency'
        if ($t5.exit_code -eq 8 -and $t5.task) {
            $resumed = Invoke-VcpContinuation -Ctx $ctx -Stage 'T5-resume' -Title "resume $($t5.task)" -Arguments @('resume', $t5.task) -Config $profileMain -AcceptExit @(0, 3)
            if ($resumed) {
                Test-StageExit $ctx $resumed 'T5-resume'
                [void](Invoke-Gate -Ctx $ctx -Stage 'T5-resume' -Id 'resume-same-task' -Description 'resume <task> continued the paused T5 task' -Test {
                        Assert-That ($resumed.task -eq $t5.task) "resumed '$($resumed.task)' != paused '$($t5.task)'"; $true })
            }
        }
        else { [void](Skip-Gate $ctx 'T5-resume' 'resume-same-task' 'resume <task> continued the paused T5 task' "T5 ended with exit $($t5.exit_code); continuation not exercised") }
        & $gatesT5 'T5-concurrency'
        [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T5-concurrency' -Config $profileMain -GateScript $gatesT5)
    }
    Save-Checkpoint $ctx 'T5: optimistic concurrency'

    # --- T6 ----------------------------------------------------------------
    [void](Invoke-PlanModeReview -Ctx $ctx -Stage 'T6-review' -Config $profileReview -Prompt $promptReview)

    # --- FINAL: publish and smoke the published executable ----------------
    $stage = 'FINAL'
    Write-Step $ctx 'FINAL publish and independent verification' 'phase'
    Test-Build $stage
    Test-Tests $stage 22 $regressionNames
    Test-Migrations $stage @('InitialCreate', 'AddProductRowVersion')
    Test-ProtectedUnchanged $stage $protected
    $publishDir = Join-Path $ws 'artifacts\publish'
    [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'publish' -Description 'dotnet publish -c Release produces Inventory.Web.exe' -Test {
            $run = Invoke-Dotnet $stage 'publish' @('publish', (Join-Path $ws $webProject), '-c', 'Release', '-o', $publishDir, '-nologo')
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            Assert-That (Test-Path -LiteralPath (Join-Path $publishDir 'Inventory.Web.exe')) 'Inventory.Web.exe missing'; $true })
    if (Test-Path -LiteralPath (Join-Path $publishDir 'Inventory.Web.exe')) {
        Invoke-RuntimeGates $stage { param($p) Test-ApiContract $stage $p; Test-RazorPages $stage $p; Test-Concurrency $stage $p } -Port $PublishedPort -Published
        Add-Asset $ctx (Join-Path $publishDir 'Inventory.Web.exe') 'Published ASP.NET Core application host (Release)'
        Add-Asset $ctx (Join-Path $publishDir 'Inventory.Web.dll') 'Published application assembly'
        $zip = Join-Path $ws "artifacts\inventory-web-$($ctx.RunId).zip"
        Compress-Archive -Path (Join-Path $publishDir '*') -DestinationPath $zip -Force
        Add-Asset $ctx $zip 'Zipped Release publish output'
    }
    $script = Join-Path $ws 'artifacts\migrations.sql'
    [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'migration-script' -Description 'idempotent deployment migration script is generated and nonempty' -Test {
            $idempotent = Invoke-Dotnet $stage 'ef-script' @('tool', 'run', 'dotnet-ef', 'migrations', 'script', '--idempotent', '--project', 'src/Inventory.Web', '--output', $script)
            Assert-That ($idempotent.ExitCode -eq 0) "migration script exit $($idempotent.ExitCode)"
            Assert-That ((Test-Path -LiteralPath $script) -and (Get-Item -LiteralPath $script).Length -gt 0) 'migration script missing or empty'
            Add-Asset $ctx $script 'Idempotent SQL migration script for deployment'
            $true })
    Save-Checkpoint $ctx 'FINAL: verified state'
    if ($DropDatabase) {
        [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'database-drop' -Description 'requested scenario database cleanup succeeds' -Test {
                $drop = Invoke-Dotnet $stage 'ef-database-drop' @('tool', 'run', 'dotnet-ef', 'database', 'drop', '--force', '--project', 'src/Inventory.Web')
                Assert-That ($drop.ExitCode -eq 0) "database drop exit $($drop.ExitCode)"
                $true })
    }
    else { $ctx.Notes.Add("Database '$database' was left in place for inspection; drop it with dotnet-ef database drop or -DropDatabase.") }

    Invoke-FinalEvidenceSweep $ctx 'products'
}
catch {
    if ($_.Exception.Message -eq 'VCP_SCENARIO_DRY_RUN_COMPLETE') { $ctx.Notes.Add('Dry run: paid stages and FINAL gates were not executed.') }
    else {
        $ctx.Fatal = $_.Exception.Message
        Write-Step $ctx "FATAL: $($ctx.Fatal)" 'fail'
    }
}
finally {
    $exitCode = Complete-VcpScenario $ctx
}
exit $exitCode
