#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Scenario C - ledger-cli: a Java 21 console application (Maven, picocli, Jackson)
for importing bank CSV exports, categorizing and reporting, built over multiple
VCP CLI turns and assessed against harness-computed expected outputs.

.DESCRIPTION
See docs/test-plans/cli-test-plans.md, "Scenario C". The harness generates a
deterministic three-month transaction fixture and computes every expected
report itself, so CLI output is compared exactly. Turns: import, categorize and
JSON reports, table/budget/error handling, protected regression tests, export
and date ranges (interrupted by a short deadline and continued with
'vcp sessions resume <session>'), and a plan-mode review. It finishes with
'mvn verify', the shaded executable jar and sample outputs in artifacts/.

.EXAMPLE
pwsh -File .\scenario-c-java-ledger-cli.ps1 -ProviderGeneration C:\vcp-private\provider-20261002
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
    [switch]$SkipPaidStages
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force

$ctx = Initialize-VcpScenario -Name 'c-java-ledger-cli' -RunRoot $RunRoot -ProjectPath $ProjectPath -Vcp $Vcp -ProviderGeneration $ProviderGeneration `
    -TurnBudgetUsd $TurnBudgetUsd -MaxScenarioUsd $MaxScenarioUsd -MaxRepairTurns $MaxRepairTurns -OutputTokens $OutputTokens `
    -MaxRequests $MaxRequests -DeadlineSeconds $DeadlineSeconds -ShortDeadlineSeconds $ShortDeadlineSeconds -SkipPaidStages:$SkipPaidStages
$ws = $ctx.Workspace
$inv = [System.Globalization.CultureInfo]::InvariantCulture

#region Toolchain

$javaCandidates = @()
if ($env:JAVA_HOME) { $javaCandidates = @((Join-Path $env:JAVA_HOME 'bin\java.exe')) }
$java = Find-Executable -Name 'java' -Candidates $javaCandidates
if (-not $java) { throw 'A JDK 21+ (java.exe) is required. Set JAVA_HOME or add it to PATH.' }
$javac = Join-Path (Split-Path -Parent $java) 'javac.exe'
if (-not (Test-Path -LiteralPath $javac)) { throw "javac.exe not found next to $java; a JDK (not a JRE) is required." }
$javaVersionText = (& $java -version 2>&1 | Out-String)
$javaMajor = if ($javaVersionText -match 'version "(\d+)') { [int]$Matches[1] } else { 0 }
if ($javaMajor -lt 21) { throw "JDK $javaMajor found; 21 or later is required." }
$mvn = (Get-Command 'mvn.cmd' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1).Source
if (-not $mvn) { throw 'Apache Maven (mvn.cmd) 3.9+ is required on PATH.' }
$mavenHome = Split-Path -Parent (Split-Path -Parent $mvn)
$classworlds = Get-ChildItem -LiteralPath (Join-Path $mavenHome 'boot') -Filter 'plexus-classworlds-*.jar' -ErrorAction SilentlyContinue | Select-Object -First 1
$m2conf = Join-Path $mavenHome 'bin\m2.conf'
$mavenViaJava = $null
if ($classworlds -and (Test-Path -LiteralPath $m2conf)) {
    # mvn.cmd is a batch script, which VCP cannot run (no shell). This is the same
    # launcher invocation mvn.cmd performs, expressed for the 'java' process profile.
    $mavenViaJava = @('-classpath', $classworlds.FullName, "-Dclassworlds.conf=$m2conf", "-Dmaven.home=$mavenHome",
        "-Dmaven.multiModuleProjectDirectory=$ws", 'org.codehaus.plexus.classworlds.launcher.Launcher')
}
if (-not $mavenViaJava) { throw "Maven's Java launcher was not found under $mavenHome. Install the Apache Maven binary distribution and put its bin directory on PATH." }

function Invoke-Maven([string]$Stage, [string]$Label, [string[]]$Arguments, [int]$TimeoutSeconds = 1500) {
    $javaHome = Split-Path -Parent (Split-Path -Parent $java)
    return Invoke-Tool -Ctx $ctx -Stage $Stage -Label $Label -FilePath $java -ArgumentList ($mavenViaJava + @('-B', '-ntp') + $Arguments) `
        -TimeoutSeconds $TimeoutSeconds -Environment @{ JAVA_HOME = $javaHome }
}
function Invoke-Ledger([string]$Stage, [string]$Label, [string[]]$Arguments) {
    $jar = Join-Path $ws 'target\ledger-cli-1.0.0-all.jar'
    return Invoke-Tool -Ctx $ctx -Stage $Stage -Label $Label -FilePath $java -ArgumentList (@('-jar', $jar) + $Arguments) -TimeoutSeconds 120
}
function Get-Tail([string]$Text, [int]$Count = 40) { return (($Text -split "`r?`n") | Select-Object -Last $Count) -join "`n" }
function Format-Money([decimal]$Value) { return $Value.ToString('0.00', $inv) }

#endregion

#region Deterministic fixtures and expected results

$rules = @(
    @('GROCER', 'Groceries'), @('SUPERMARKET', 'Groceries'), @('COFFEE', 'Dining'), @('RESTAURANT', 'Dining'),
    @('PAYROLL', 'Income'), @('RENT', 'Housing'), @('ELECTRIC', 'Utilities'), @('WATER', 'Utilities'),
    @('AIRLINE', 'Travel'), @('HOTEL', 'Travel'), @('PHARMACY', 'Health'), @('GYM', 'Health'))

function Get-Category([string]$Description) {
    foreach ($rule in $rules) { if ($Description.ToUpperInvariant().Contains($rule[0])) { return $rule[1] } }
    return 'Uncategorized'
}

function New-Row([datetime]$Date, [string]$Description, [int]$Cents, [string]$Account) {
    return [pscustomobject]@{ Date = $Date.ToString('yyyy-MM-dd'); Description = $Description; Amount = [decimal]$Cents / 100; Account = $Account }
}

function New-LedgerFixture {
    $rng = [System.Random]::new(20260301)
    $rows = [System.Collections.Generic.List[object]]::new()
    $variable = @(
        @{ D = 'GREENLEAF GROCERS #{0}'; Min = 2500; Max = 18000; A = 'checking'; W = 5 },
        @{ D = 'FRESHMART SUPERMARKET'; Min = 1500; Max = 9000; A = 'checking'; W = 3 },
        @{ D = 'BEAN THERE COFFEE'; Min = 450; Max = 1200; A = 'credit'; W = 6 },
        @{ D = 'LUIGI RESTAURANT'; Min = 2500; Max = 9000; A = 'credit'; W = 2 },
        @{ D = 'CORNER PHARMACY'; Min = 800; Max = 6000; A = 'checking'; W = 1 },
        @{ D = 'ATM WITHDRAWAL'; Min = 2000; Max = 20000; A = 'checking'; W = 1 },
        @{ D = 'HARBORVIEW HOTEL'; Min = 12000; Max = 42000; A = 'credit'; W = 0.3 },
        @{ D = 'SKYWAYS AIRLINE'; Min = 18000; Max = 65000; A = 'credit'; W = 0.3 })
    $pool = [System.Collections.Generic.List[object]]::new()
    foreach ($merchant in $variable) { for ($i = 0; $i -lt [math]::Ceiling($merchant.W * 10); $i++) { $pool.Add($merchant) } }
    $start = [datetime]::new(2026, 1, 1)
    for ($day = 0; $day -lt 90; $day++) {
        $date = $start.AddDays($day)
        if ($date.Day -eq 1) {
            $rows.Add((New-Row $date 'HARBOR APARTMENTS RENT' -185000 'checking'))
            $rows.Add((New-Row $date 'CITY ELECTRIC UTILITY' (-1 * $rng.Next(6000, 14001)) 'checking'))
        }
        if ($date.Day -in 1, 15) { $rows.Add((New-Row $date 'ACME CORP PAYROLL' 325000 'checking')) }
        if ($date.Day -eq 5) {
            $rows.Add((New-Row $date 'CITY WATER UTILITY' (-1 * $rng.Next(3000, 6501)) 'checking'))
            $rows.Add((New-Row $date 'IRONWORKS GYM' -4500 'credit'))
            $rows.Add((New-Row $date 'STREAMFLIX SUBSCRIPTION' -1599 'credit'))
        }
        $count = $rng.Next(0, 4)
        for ($i = 0; $i -lt $count; $i++) {
            $merchant = $pool[$rng.Next(0, $pool.Count)]
            $description = [string]::Format($inv, $merchant.D, $rng.Next(100, 1000))
            $rows.Add((New-Row $date $description (-1 * $rng.Next($merchant.Min, $merchant.Max + 1)) $merchant.A))
        }
    }
    for ($i = 0; $i -lt 6; $i++) {
        $copy = $rows[$rng.Next(0, $rows.Count)]
        $rows.Insert($rng.Next(0, $rows.Count + 1), $copy)
    }
    return $rows
}

function ConvertTo-LedgerCsv([object[]]$Rows) {
    $lines = @('date,description,amount,account')
    foreach ($row in $Rows) { $lines += '{0},{1},{2},{3}' -f $row.Date, $row.Description, (Format-Money $row.Amount), $row.Account }
    return ($lines -join "`n") + "`n"
}

function Get-UniqueRows([object[]]$Rows) {
    $seen = [System.Collections.Generic.HashSet[string]]::new()
    return @($Rows | Where-Object { $seen.Add(('{0}|{1}|{2}|{3}' -f $_.Date, $_.Description, (Format-Money $_.Amount), $_.Account)) })
}

function Get-ExpectedReport([object[]]$Unique, [string]$From, [string]$To) {
    $inRange = @($Unique | Where-Object { [string]::CompareOrdinal($_.Date, $From) -ge 0 -and [string]::CompareOrdinal($_.Date, $To) -le 0 })
    $income = [decimal]0; $expenses = [decimal]0
    foreach ($row in $inRange) { if ($row.Amount -gt 0) { $income += $row.Amount } else { $expenses += $row.Amount } }
    $groups = @($inRange | Group-Object { Get-Category $_.Description } | ForEach-Object {
            $total = [decimal]0; foreach ($row in $_.Group) { $total += $row.Amount }
            [pscustomobject]@{ category = $_.Name; total = $total; count = $_.Count } })
    $sorted = [System.Collections.Generic.List[object]]::new()
    foreach ($group in $groups) { $sorted.Add($group) }
    $sorted.Sort([System.Comparison[object]] { param($a, $b)
            $c = $a.total.CompareTo($b.total); if ($c -ne 0) { return $c }; return [string]::CompareOrdinal($a.category, $b.category) })
    return [ordered]@{
        income = Format-Money $income; expenses = Format-Money $expenses; net = Format-Money ($income + $expenses)
        byCategory = @($sorted | ForEach-Object { [ordered]@{ category = $_.category; total = Format-Money $_.total; count = $_.count } })
    }
}

$fixtureRows = New-LedgerFixture
$uniqueRows = Get-UniqueRows $fixtureRows
$duplicateCount = $fixtureRows.Count - $uniqueRows.Count
$uncategorizedCount = @($uniqueRows | Where-Object { (Get-Category $_.Description) -eq 'Uncategorized' }).Count
$expectedMonth = @{}
foreach ($month in '2026-01', '2026-02', '2026-03') {
    $expectedMonth[$month] = Get-ExpectedReport $uniqueRows "$month-01" "$month-31"
}
$expectedQuarter = Get-ExpectedReport $uniqueRows '2026-01-01' '2026-03-31'
$diningRow = @($expectedMonth['2026-02'].byCategory | Where-Object { $_.category -eq 'Dining' }) | Select-Object -First 1
if (-not $diningRow) { throw 'Fixture invariant broken: February has no Dining transactions.' }
$diningFeb = [decimal]::Parse($diningRow.total, $inv)

# T4 fixture: bank-export formats (parenthesized negatives, thousands separators, quoted commas).
$bankRows = @(
    @{ Raw = '2026-04-01,ACME CORP PAYROLL,"3,250.00",checking'; Row = (New-Row ([datetime]'2026-04-01') 'ACME CORP PAYROLL' 325000 'checking') },
    @{ Raw = '2026-04-01,HARBOR APARTMENTS RENT,(1850.00),checking'; Row = (New-Row ([datetime]'2026-04-01') 'HARBOR APARTMENTS RENT' -185000 'checking') },
    @{ Raw = '2026-04-03,"CITY WATER, UTILITY",(45.00),checking'; Row = (New-Row ([datetime]'2026-04-03') 'CITY WATER, UTILITY' -4500 'checking') },
    @{ Raw = '2026-04-04,  BEAN   THERE COFFEE  ,(4.75),credit'; Row = (New-Row ([datetime]'2026-04-04') 'BEAN THERE COFFEE' -475 'credit') },
    @{ Raw = '2026-04-09,LUIGI RESTAURANT,-62.40,credit'; Row = (New-Row ([datetime]'2026-04-09') 'LUIGI RESTAURANT' -6240 'credit') },
    @{ Raw = '2026-04-15,ACME CORP PAYROLL,"3,250.00",checking'; Row = (New-Row ([datetime]'2026-04-15') 'ACME CORP PAYROLL' 325000 'checking') },
    @{ Raw = '2026-04-20,"SKYWAYS AIRLINE, INC.","(1,204.10)",credit'; Row = (New-Row ([datetime]'2026-04-20') 'SKYWAYS AIRLINE, INC.' -120410 'credit') },
    @{ Raw = '2026-04-20,"SKYWAYS AIRLINE, INC.","(1,204.10)",credit'; Row = (New-Row ([datetime]'2026-04-20') 'SKYWAYS AIRLINE, INC.' -120410 'credit') })
$bankCsv = (@('date,description,amount,account') + @($bankRows | ForEach-Object Raw)) -join "`n"
$expectedApril = Get-ExpectedReport (Get-UniqueRows @($bankRows | ForEach-Object Row)) '2026-04-01' '2026-04-30'

#endregion

#region Seed

$seed = [ordered]@{}
$seed['README.md'] = @'
# ledger-cli

A personal finance command-line tool: import bank CSV exports into a local JSON
ledger, categorize transactions with simple rules, and produce monthly reports
and budget checks.

    mvn -B verify                                   # build, test, shaded jar
    java -jar target/ledger-cli-1.0.0-all.jar --help

Stack: Java 21, Maven, picocli (commands), Jackson (JSON), JUnit 5.
Sample data lives in `samples/`.

Exit codes: 0 success, 2 usage error, 3 budget exceeded, 4 input or data error.
'@
$seed['.gitignore'] = @'
target/
artifacts/
*.iml
.idea/
'@
$seed['pom.xml'] = @'
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>io.vcp.scenarios</groupId>
  <artifactId>ledger-cli</artifactId>
  <version>1.0.0</version>
  <packaging>jar</packaging>

  <properties>
    <maven.compiler.release>21</maven.compiler.release>
    <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    <picocli.version>4.7.6</picocli.version>
    <jackson.version>2.18.2</jackson.version>
    <junit.version>5.11.4</junit.version>
  </properties>

  <dependencies>
    <dependency>
      <groupId>info.picocli</groupId>
      <artifactId>picocli</artifactId>
      <version>${picocli.version}</version>
    </dependency>
    <dependency>
      <groupId>com.fasterxml.jackson.core</groupId>
      <artifactId>jackson-databind</artifactId>
      <version>${jackson.version}</version>
    </dependency>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version>${junit.version}</version>
      <scope>test</scope>
    </dependency>
  </dependencies>

  <build>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-compiler-plugin</artifactId>
        <version>3.13.0</version>
      </plugin>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-surefire-plugin</artifactId>
        <version>3.5.2</version>
      </plugin>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-jar-plugin</artifactId>
        <version>3.4.2</version>
        <configuration>
          <archive>
            <manifest>
              <mainClass>io.vcp.ledger.LedgerApp</mainClass>
            </manifest>
          </archive>
        </configuration>
      </plugin>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-shade-plugin</artifactId>
        <version>3.6.0</version>
        <executions>
          <execution>
            <phase>package</phase>
            <goals><goal>shade</goal></goals>
            <configuration>
              <shadedArtifactAttached>true</shadedArtifactAttached>
              <shadedClassifierName>all</shadedClassifierName>
              <createDependencyReducedPom>false</createDependencyReducedPom>
              <transformers>
                <transformer implementation="org.apache.maven.plugins.shade.resource.ManifestResourceTransformer">
                  <mainClass>io.vcp.ledger.LedgerApp</mainClass>
                </transformer>
                <transformer implementation="org.apache.maven.plugins.shade.resource.ServicesResourceTransformer"/>
              </transformers>
              <filters>
                <filter>
                  <artifact>*:*</artifact>
                  <excludes>
                    <exclude>META-INF/*.SF</exclude>
                    <exclude>META-INF/*.DSA</exclude>
                    <exclude>META-INF/*.RSA</exclude>
                    <exclude>module-info.class</exclude>
                  </excludes>
                </filter>
              </filters>
            </configuration>
          </execution>
        </executions>
      </plugin>
    </plugins>
  </build>
</project>
'@
$seed['src/main/java/io/vcp/ledger/LedgerApp.java'] = @'
package io.vcp.ledger;

import java.util.concurrent.Callable;
import picocli.CommandLine;
import picocli.CommandLine.Command;
import picocli.CommandLine.Model.CommandSpec;
import picocli.CommandLine.Spec;

@Command(
        name = "ledger",
        mixinStandardHelpOptions = true,
        version = "ledger-cli 1.0.0",
        description = "Personal finance ledger: import bank CSV exports, categorize and report.")
public class LedgerApp implements Callable<Integer> {
    @Spec
    CommandSpec spec;

    @Override
    public Integer call() {
        spec.commandLine().usage(spec.commandLine().getOut());
        return 0;
    }

    public static void main(String[] args) {
        System.exit(new CommandLine(new LedgerApp()).execute(args));
    }
}
'@
$seed['src/test/java/io/vcp/ledger/LedgerAppTest.java'] = @'
package io.vcp.ledger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.PrintWriter;
import java.io.StringWriter;
import org.junit.jupiter.api.Test;
import picocli.CommandLine;

class LedgerAppTest {
    @Test
    void printsVersion() {
        StringWriter out = new StringWriter();
        CommandLine command = new CommandLine(new LedgerApp());
        command.setOut(new PrintWriter(out, true));
        assertEquals(0, command.execute("--version"));
        command.getOut().flush();
        assertTrue(out.toString().contains("ledger-cli 1.0.0"));
    }
}
'@
$seed['samples/transactions-2026Q1.csv'] = ConvertTo-LedgerCsv $fixtureRows
$seed['samples/rules.csv'] = "pattern,category`n" + (($rules | ForEach-Object { '{0},{1}' -f $_[0], $_[1] }) -join "`n") + "`n"

$regressionTest = @'
// PROTECTED FILE - added by the scenario harness as an acceptance test. Do not edit.
package io.vcp.ledger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import picocli.CommandLine;

class RegressionTest {
    @TempDir
    Path dir;

    private record Result(int exitCode, String out, String err) {}

    private Result run(String... args) {
        StringWriter out = new StringWriter();
        StringWriter err = new StringWriter();
        CommandLine command = new CommandLine(new LedgerApp());
        command.setOut(new PrintWriter(out, true));
        command.setErr(new PrintWriter(err, true));
        int code = command.execute(args);
        command.getOut().flush();
        command.getErr().flush();
        return new Result(code, out.toString(), err.toString());
    }

    private Path write(String name, String content) throws Exception {
        Path path = dir.resolve(name);
        Files.writeString(path, content, StandardCharsets.UTF_8);
        return path;
    }

    private Path importCsv(String content) throws Exception {
        Path csv = write("in-" + System.nanoTime() + ".csv", "date,description,amount,account\n" + content);
        Path db = dir.resolve("ledger.json");
        Result result = run("import", "--input", csv.toString(), "--db", db.toString());
        assertEquals(0, result.exitCode(), result.err());
        return db;
    }

    private JsonNode report(Path db, String month) throws Exception {
        Result result = run("report", "--db", db.toString(), "--month", month, "--format", "json");
        assertEquals(0, result.exitCode(), result.err());
        return new ObjectMapper().readTree(result.out());
    }

    @Test
    void parenthesizedAmountsAreNegative() throws Exception {
        Path db = importCsv("2026-05-02,CORNER PHARMACY,(45.10),checking\n");
        assertEquals("-45.10", report(db, "2026-05").get("expenses").asText());
    }

    @Test
    void thousandsSeparatorsAreParsed() throws Exception {
        Path db = importCsv("2026-05-01,ACME CORP PAYROLL,\"3,250.00\",checking\n");
        assertEquals("3250.00", report(db, "2026-05").get("income").asText());
    }

    @Test
    void quotedDescriptionWithCommaIsCategorized() throws Exception {
        Path db = importCsv("2026-05-03,\"CITY WATER, UTILITY\",-38.20,checking\n");
        Path rules = write("rules.csv", "pattern,category\nWATER,Utilities\n");
        assertEquals(0, run("categorize", "--db", db.toString(), "--rules", rules.toString()).exitCode());
        JsonNode categories = report(db, "2026-05").get("byCategory");
        assertEquals(1, categories.size());
        assertEquals("Utilities", categories.get(0).get("category").asText());
        assertEquals("-38.20", categories.get(0).get("total").asText());
    }

    @Test
    void reimportReportsAllRowsAsDuplicates() throws Exception {
        String rows = "2026-05-04,BEAN THERE COFFEE,-4.50,credit\n2026-05-05,BEAN THERE COFFEE,-5.25,credit\n";
        Path db = importCsv(rows);
        Path again = write("again.csv", "date,description,amount,account\n" + rows);
        Result result = run("import", "--input", again.toString(), "--db", db.toString());
        assertEquals(0, result.exitCode(), result.err());
        assertTrue(result.out().contains("Imported 0 transactions (2 duplicates skipped)"), result.out());
    }

    @Test
    void emptyMonthReportsZeros() throws Exception {
        Path db = importCsv("2026-05-06,IRONWORKS GYM,-45.00,credit\n");
        JsonNode empty = report(db, "2030-01");
        assertEquals("0.00", empty.get("income").asText());
        assertEquals("0.00", empty.get("expenses").asText());
        assertEquals("0.00", empty.get("net").asText());
        assertEquals(0, empty.get("byCategory").size());
    }
}
'@

#endregion

#region Prompts

$mavenLine = if ($mavenViaJava) {
    $mavenJson = ConvertTo-Json -Compress @($mavenViaJava + @('-B', '-ntp', 'verify'))
    '  - Maven through the `java` profile (`mvn.cmd` is a batch script and cannot run without a shell):' + "`n" +
    '    arguments ' + $mavenJson + "`n" + '    (replace the final "verify" with other goals as needed).'
}
else { '  - Maven is not runnable inside VCP on this machine; compile with javac and rely on the harness build.' }

$environmentBlock = @'

## Environment and rules (applies to every task in this project)

- Work only inside the current workspace. Read README.md, pom.xml and the existing code first.
- Process profiles available to `vcp_exec` (no shell; literal arguments):
  - `java` and `javac` from JDK {{JAVA}}.
{{MAVEN}}
- Dependencies available: picocli, Jackson databind, JUnit 5 (already in pom.xml). Do not add other
  dependencies unless essential; pin exact versions and explain why.
- All command output must go through picocli's `CommandLine.getOut()` / `getErr()` (inject
  `@Spec CommandSpec spec`), never `System.out`/`System.err`, so tests can capture it.
- Keep `io.vcp.ledger.LedgerApp` as the top-level command and `main` entry point, and keep
  `mvn verify` producing `target/ledger-cli-1.0.0-all.jar`.
- Exit codes: 0 success, 2 usage error (picocli default), 3 budget exceeded, 4 input or data error
  (missing or malformed file; message on stderr; nothing written).
- Protected files (never edit, rename or delete): `samples/transactions-2026Q1.csv`, `samples/rules.csv`{{PROTECTED}}.
- Before finishing, run `mvn verify` (or compile and run your tests) and fix any failure. Finish with a
  short summary of changed files and command results.
'@
$environmentBlock = $environmentBlock.Replace('{{JAVA}}', [string]$javaMajor).Replace('{{MAVEN}}', $mavenLine)
function New-Prompt([string]$Body, [string]$Protected = '') {
    if ($ctx.ReuseProject -and (Test-Path -LiteralPath (Join-Path $ws 'src/test/java/io/vcp/ledger/RegressionTest.java')) -and $Protected -notlike '*RegressionTest.java*') {
        $Protected += ', `src/test/java/io/vcp/ledger/RegressionTest.java`'
    }
    return $Body + $environmentBlock.Replace('{{PROTECTED}}', $Protected)
}

$promptT1 = New-Prompt @'
# Task T1 - Import bank CSV exports into a JSON ledger

Implement `ledger import --input <csv> --db <ledger.json>` as a picocli subcommand.

- Input: UTF-8 CSV with header `date,description,amount,account`, RFC 4180 quoting (quoted fields
  may contain commas and doubled quotes). Dates are `yyyy-MM-dd`; amounts are plain decimals with an
  optional leading `-` (for example `-45.67`, `3250.00`).
- Normalization: trim the description and collapse internal whitespace to single spaces; store
  amounts as `BigDecimal` with scale 2.
- Duplicates: a row is a duplicate when date, description, amount and account all equal an existing
  transaction (already in the ledger or earlier in the same file); duplicates are skipped.
- Output exactly one line on stdout: `Imported N transactions (M duplicates skipped)`.
- Errors: missing input file or any malformed row (bad date, amount, column count) -> message on
  stderr naming the 1-based physical line number like `line 7: invalid amount 'abc'`, exit code 4,
  and nothing written (import is all-or-nothing). A missing ledger file is created.
- The ledger is a JSON document written with Jackson, atomically (temporary file then move).
- Unit tests for CSV parsing (quoting), normalization, duplicate detection and the error path.
- Try it on `samples/transactions-2026Q1.csv`.
'@
$promptT2 = New-Prompt @'
# Task T2 - Categorize and monthly JSON report

- `ledger categorize --db <ledger.json> --rules <rules.csv>`: rules CSV has header `pattern,category`;
  the first rule whose pattern is a case-insensitive substring of the description wins; no match ->
  `Uncategorized`. Re-running replaces earlier categories. Output exactly one line:
  `Categorized N transactions (K uncategorized)`. Never-categorized transactions report as
  `Uncategorized`.
- `ledger report --db <ledger.json> --month yyyy-MM [--format json|table]` (default table; implement
  json now): prints exactly one JSON object
  `{"month":"2026-02","income":"6500.00","expenses":"-3210.55","net":"3289.45",
    "byCategory":[{"category":"Housing","total":"-1850.00","count":1}, ...]}`
  income = sum of positive amounts; expenses = sum of negative amounts (a negative value); net =
  income + expenses. byCategory contains every category with at least one transaction in the month
  (including Income), ordered by total ascending then category name (ordinal). All money values are
  strings with exactly two decimals. A month without transactions gives zeros and an empty array.
  A missing ledger file -> exit 4.
- Tests for rule precedence, report arithmetic and ordering.
'@
$promptT3 = New-Prompt @'
# Task T3 - Table output, budgets and robust errors

- `report --format table`: a fixed-width text table with a header line containing `Category`, `Total`
  and `Count`, one row per category in the same order as JSON, and final lines for `Income`,
  `Expenses` and `Net` using the same two-decimal formatting.
- `ledger budget check --db <ledger.json> --budgets <budgets.csv> --month yyyy-MM`: budgets CSV has
  header `category,limit` (positive limits). For each budget line print
  `<category>: spent <amount> of <limit> (OK|OVER)` where spent is the category's outflow in that
  month as a positive two-decimal number (0.00 if none). Exit 3 if any category is OVER, else 0.
- Malformed budgets or rules files -> exit 4 with the line number on stderr; unknown subcommands
  and bad options -> exit 2 with usage on stderr.
- Tests for each exit code.
'@
$promptT4 = New-Prompt @'
# Task T4 - Make the protected regression tests pass

A teammate added `src/test/java/io/vcp/ledger/RegressionTest.java` from real bank exports. Make every
test in it pass by changing the implementation only; the test file is protected. Real exports write
negative amounts in parentheses (`(45.10)` means -45.10) and use thousands separators inside quoted
amounts (`"3,250.00"`, `"(1,204.10)"`). Keep all other tests passing.
'@ '`, `src/test/java/io/vcp/ledger/RegressionTest.java`'
$promptT5 = New-Prompt @'
# Task T5 - Export and date ranges

- `ledger export --db <ledger.json> --format csv|json --output <file> [--month yyyy-MM]`: writes all
  (or that month's) transactions ordered by date, then description. JSON: an array of objects
  `{"date","description","amount","account","category"}` with amount as a two-decimal string. CSV:
  header `date,description,amount,account,category` with RFC 4180 quoting. Print
  `Exported N transactions to <file>`.
- `report` also accepts `--from yyyy-MM-dd --to yyyy-MM-dd` (inclusive) instead of `--month`; the JSON
  then has `"from"` and `"to"` instead of `"month"`. `--month` together with `--from`/`--to`, or only
  one of `--from`/`--to`, is a usage error (exit 2).
- `--help` for every subcommand documents its options; update README.md with an end-to-end example
  (import, categorize, report, budget check, export).
- Tests for export ordering/quoting and the date-range validation.
'@ '`, `src/test/java/io/vcp/ledger/RegressionTest.java`'
$promptReview = @'
# Task T6 - Read-only engineering review

Do not modify, create or delete any file. Review this CLI before its first release: money handling
(BigDecimal scale and rounding), CSV parsing edge cases, atomicity of ledger writes, error messages
and exit codes, command structure and help, and test coverage. Cite file paths and line numbers.

End your answer with one fenced ```json block of the form
{"findings":[{"severity":"high|medium|low","file":"path","line":n,"title":"...","recommendation":"..."}]}
listing at most 10 findings ordered by severity.
'@

#endregion

#region Gates

function Test-MavenVerify([string]$Stage, [int]$MinTests, [string[]]$Required = @()) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'mvn-verify' -Description "mvn verify passes with >= $MinTests tests and builds the shaded jar" -Test {
            $run = Invoke-Maven $Stage 'verify' @('clean', 'verify')
            $reports = @(Get-ChildItem -LiteralPath (Join-Path $ws 'target\surefire-reports') -Filter 'TEST-*.xml' -ErrorAction SilentlyContinue)
            $tests = 0; $failures = 0; $names = @(); $failed = @()
            foreach ($report in $reports) {
                $suite = ([xml](Get-Content -LiteralPath $report.FullName -Raw)).testsuite
                $failures += [int]$suite.failures + [int]$suite.errors
                foreach ($case in @($suite.testcase)) {
                    if ($case.SelectSingleNode('failure|error')) { $failed += "$($suite.name).$($case.name)" }
                    elseif (-not $case.SelectSingleNode('skipped')) { $tests++; $names += [string]$case.name }
                }
            }
            Assert-That ($run.ExitCode -eq 0) ("mvn exit {0}; failed tests: {1}`n{2}" -f $run.ExitCode, ($failed -join ', '), (Get-Tail $run.Output 60))
            Assert-That ($tests -ge $MinTests -and $failures -eq 0) "tests=$tests failures=$failures"
            $missing = @($Required | Where-Object { $names -notcontains $_ })
            Assert-That ($missing.Count -eq 0) ('required tests not passing: ' + ($missing -join ', '))
            Assert-That (Test-Path -LiteralPath (Join-Path $ws 'target\ledger-cli-1.0.0-all.jar')) 'shaded jar missing'
            $true })
}

function New-TempPath([string]$Stage, [string]$Name) {
    $directory = Join-Path $ctx.Temp ("$Stage-" + [guid]::NewGuid().ToString('N').Substring(0, 6))
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    return Join-Path $directory $Name
}

function Compare-Report($Actual, $Expected, [string]$Label) {
    foreach ($field in 'income', 'expenses', 'net') {
        Assert-That ([string]$Actual.$field -eq $Expected[$field]) "$Label ${field}: expected $($Expected[$field]) got $($Actual.$field)"
    }
    $actualRows = @($Actual.byCategory | ForEach-Object { '{0}={1}/{2}' -f $_.category, $_.total, $_.count })
    $expectedRows = @($Expected.byCategory | ForEach-Object { '{0}={1}/{2}' -f $_.category, $_.total, $_.count })
    Assert-That (($actualRows -join '; ') -eq ($expectedRows -join '; ')) "$Label byCategory`n expected: $($expectedRows -join '; ')`n actual:   $($actualRows -join '; ')"
}

function Initialize-Ledger([string]$Stage, [switch]$Categorize) {
    $db = New-TempPath $Stage 'ledger.json'
    $import = Invoke-Ledger $Stage 'import' @('import', '--input', (Join-Path $ws 'samples\transactions-2026Q1.csv'), '--db', $db)
    if ($Categorize) { [void](Invoke-Ledger $Stage 'categorize' @('categorize', '--db', $db, '--rules', (Join-Path $ws 'samples\rules.csv'))) }
    return [pscustomobject]@{ Db = $db; Import = $import }
}

function Test-Import([string]$Stage) {
    $ledger = Initialize-Ledger $Stage
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'import.counts' -Description "import reports $($uniqueRows.Count) imported, $duplicateCount duplicates" -Test {
            $expected = "Imported $($uniqueRows.Count) transactions ($duplicateCount duplicates skipped)"
            Assert-That ($ledger.Import.ExitCode -eq 0 -and $ledger.Import.Output.Trim() -eq $expected) "exit $($ledger.Import.ExitCode): '$($ledger.Import.Output.Trim())' expected '$expected' $($ledger.Import.Errors)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'import.idempotent' -Description 're-import skips every row as a duplicate' -Test {
            $again = Invoke-Ledger $Stage 'reimport' @('import', '--input', (Join-Path $ws 'samples\transactions-2026Q1.csv'), '--db', $ledger.Db)
            $expected = "Imported 0 transactions ($($fixtureRows.Count) duplicates skipped)"
            Assert-That ($again.ExitCode -eq 0 -and $again.Output.Trim() -eq $expected) "got '$($again.Output.Trim())' expected '$expected'"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'import.malformed' -Description 'malformed row -> exit 4, stderr names line 7, nothing written' -Test {
            $lines = @('date,description,amount,account') + @($fixtureRows[0..4] | ForEach-Object { '{0},{1},{2},{3}' -f $_.Date, $_.Description, (Format-Money $_.Amount), $_.Account }) + @('2026-01-09,BROKEN ROW,abc,checking')
            $bad = New-TempPath $Stage 'malformed.csv'
            Write-Utf8File $bad (($lines -join "`n") + "`n")
            $db = New-TempPath $Stage 'malformed-ledger.json'
            $run = Invoke-Ledger $Stage 'import-malformed' @('import', '--input', $bad, '--db', $db)
            Assert-That ($run.ExitCode -eq 4) "exit $($run.ExitCode)"
            Assert-That ($run.Errors -match 'line 7') "stderr: $($run.Errors.Trim())"
            Assert-That (-not (Test-Path -LiteralPath $db)) 'ledger written despite the error'; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'import.missing-file' -Description 'missing input file -> exit 4' -Test {
            $run = Invoke-Ledger $Stage 'import-missing' @('import', '--input', (Join-Path $ctx.Temp 'nope.csv'), '--db', (New-TempPath $Stage 'x.json'))
            Assert-That ($run.ExitCode -eq 4) "exit $($run.ExitCode)"; $true })
}

function Test-Reports([string]$Stage) {
    $ledger = Initialize-Ledger $Stage -Categorize
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'categorize.counts' -Description "categorize reports $($uniqueRows.Count) categorized, $uncategorizedCount uncategorized" -Test {
            $run = Invoke-Ledger $Stage 'categorize-again' @('categorize', '--db', $ledger.Db, '--rules', (Join-Path $ws 'samples\rules.csv'))
            $expected = "Categorized $($uniqueRows.Count) transactions ($uncategorizedCount uncategorized)"
            Assert-That ($run.ExitCode -eq 0 -and $run.Output.Trim() -eq $expected) "got '$($run.Output.Trim())' expected '$expected'"; $true })
    foreach ($month in '2026-01', '2026-02', '2026-03') {
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id "report.$month" -Description "report --month $month --format json equals harness-computed totals" -Test {
                $run = Invoke-Ledger $Stage "report-$month" @('report', '--db', $ledger.Db, '--month', $month, '--format', 'json')
                Assert-That ($run.ExitCode -eq 0) "exit $($run.ExitCode): $($run.Errors)"
                $json = $run.Output | ConvertFrom-Json -Depth 20
                Assert-That ($json.month -eq $month) "month field '$($json.month)'"
                Compare-Report $json $expectedMonth[$month] $month
                Write-Utf8File (Join-Path $ctx.Logs "$Stage\report-$month.json") $run.Output; $true })
    }
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'report.empty-month' -Description 'empty month -> zeros and empty byCategory' -Test {
            $run = Invoke-Ledger $Stage 'report-empty' @('report', '--db', $ledger.Db, '--month', '2030-01', '--format', 'json')
            $json = $run.Output | ConvertFrom-Json -Depth 20
            Assert-That ($run.ExitCode -eq 0 -and $json.month -eq '2030-01' -and $json.income -eq '0.00' -and $json.expenses -eq '0.00' -and $json.net -eq '0.00' -and @($json.byCategory).Count -eq 0) "exit $($run.ExitCode) body $($run.Output)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'report.missing-db' -Description 'missing ledger -> exit 4' -Test {
            $run = Invoke-Ledger $Stage 'report-missing' @('report', '--db', (Join-Path $ctx.Temp 'missing-ledger.json'), '--month', '2026-01', '--format', 'json')
            Assert-That ($run.ExitCode -eq 4) "exit $($run.ExitCode)"; $true })
}

function Test-TableAndBudgets([string]$Stage) {
    $ledger = Initialize-Ledger $Stage -Categorize
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'table' -Description 'table format has header and one row per category with totals' -Test {
            $run = Invoke-Ledger $Stage 'report-table' @('report', '--db', $ledger.Db, '--month', '2026-02', '--format', 'table')
            Assert-That ($run.ExitCode -eq 0) "exit $($run.ExitCode)"
            Assert-That ($run.Output -match 'Category' -and $run.Output -match 'Total' -and $run.Output -match 'Count') 'header missing'
            foreach ($row in $expectedMonth['2026-02'].byCategory) {
                Assert-That ($run.Output -match ('(?m)^.*\b' + [regex]::Escape($row.category) + '\b.*' + [regex]::Escape($row.total))) "row for $($row.category) $($row.total) missing"
            }
            Assert-That ($run.Output -match ('(?m)^.*\bNet\b.*' + [regex]::Escape($expectedMonth['2026-02'].net))) 'Net line missing'; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'budget.over' -Description 'budget check flags Dining OVER and exits 3' -Test {
            $limit = Format-Money ([math]::Floor(-$diningFeb / 2))
            $budgets = New-TempPath $Stage 'budgets-tight.csv'
            Write-Utf8File $budgets "category,limit`nDining,$limit`nHousing,5000.00`nGroceries,5000.00`n"
            $run = Invoke-Ledger $Stage 'budget-over' @('budget', 'check', '--db', $ledger.Db, '--budgets', $budgets, '--month', '2026-02')
            $spent = Format-Money (-$diningFeb)
            Assert-That ($run.ExitCode -eq 3) "exit $($run.ExitCode)"
            Assert-That ($run.Output -match ("(?m)^Dining: spent {0} of {1} \(OVER\)" -f [regex]::Escape($spent), [regex]::Escape($limit))) "Dining line wrong:`n$($run.Output)"
            Assert-That ($run.Output -match '(?m)^Housing: spent 1850\.00 of 5000\.00 \(OK\)') "Housing line wrong:`n$($run.Output)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'budget.ok' -Description 'generous budgets -> exit 0' -Test {
            $budgets = New-TempPath $Stage 'budgets-loose.csv'
            Write-Utf8File $budgets "category,limit`nDining,99999.00`nTravel,99999.00`n"
            $run = Invoke-Ledger $Stage 'budget-ok' @('budget', 'check', '--db', $ledger.Db, '--budgets', $budgets, '--month', '2026-02')
            Assert-That ($run.ExitCode -eq 0 -and $run.Output -notmatch 'OVER') "exit $($run.ExitCode) $($run.Output)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'usage-error' -Description 'unknown subcommand -> exit 2' -Test {
            $run = Invoke-Ledger $Stage 'unknown-subcommand' @('frobnicate')
            Assert-That ($run.ExitCode -eq 2) "exit $($run.ExitCode)"; $true })
}

function Test-BankExport([string]$Stage) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'bank-export' -Description 'bank export (parentheses, thousands, quoted commas, whitespace) reports exactly' -Test {
            $csv = New-TempPath $Stage 'bank-export-2026-04.csv'
            Write-Utf8File $csv ($bankCsv + "`n")
            $db = New-TempPath $Stage 'bank-ledger.json'
            $import = Invoke-Ledger $Stage 'import-bank' @('import', '--input', $csv, '--db', $db)
            Assert-That ($import.ExitCode -eq 0 -and $import.Output.Trim() -eq 'Imported 7 transactions (1 duplicates skipped)') "import: $($import.ExitCode) '$($import.Output.Trim())' $($import.Errors)"
            $categorized = Invoke-Ledger $Stage 'categorize-bank' @('categorize', '--db', $db, '--rules', (Join-Path $ws 'samples\rules.csv'))
            Assert-That ($categorized.ExitCode -eq 0) "categorize exit $($categorized.ExitCode): $($categorized.Errors)"
            $run = Invoke-Ledger $Stage 'report-bank' @('report', '--db', $db, '--month', '2026-04', '--format', 'json')
            Assert-That ($run.ExitCode -eq 0) "report exit $($run.ExitCode): $($run.Errors)"
            Compare-Report ($run.Output | ConvertFrom-Json -Depth 20) $expectedApril '2026-04'; $true })
}

function Test-ExportAndRange([string]$Stage) {
    $ledger = Initialize-Ledger $Stage -Categorize
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'export.json' -Description 'export json has every transaction, ordered, categorized' -Test {
            $out = New-TempPath $Stage 'export.json'
            $run = Invoke-Ledger $Stage 'export-json' @('export', '--db', $ledger.Db, '--format', 'json', '--output', $out)
            Assert-That ($run.ExitCode -eq 0 -and (Test-Path -LiteralPath $out)) "exit $($run.ExitCode) $($run.Errors)"
            $items = @(Get-Content -LiteralPath $out -Raw | ConvertFrom-Json -Depth 10)
            Assert-That ($items.Count -eq $uniqueRows.Count) "count $($items.Count) expected $($uniqueRows.Count)"
            $keys = @($items | ForEach-Object { '{0}|{1}' -f $_.date, $_.description })
            $sortedKeys = [string[]]$keys.Clone(); [array]::Sort($sortedKeys, [System.StringComparer]::Ordinal)
            Assert-That (($keys -join "`n") -eq ($sortedKeys -join "`n")) 'not ordered by date then description'
            Compare-LedgerExport $items $uniqueRows; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'export.csv' -Description 'export csv has header plus one line per transaction' -Test {
            $out = New-TempPath $Stage 'export.csv'
            $run = Invoke-Ledger $Stage 'export-csv' @('export', '--db', $ledger.Db, '--format', 'csv', '--output', $out)
            $rows = @(Import-Csv -LiteralPath $out)
            Assert-That ($run.ExitCode -eq 0 -and $rows.Count -eq $uniqueRows.Count -and ($rows[0].PSObject.Properties.Name -join ',') -eq 'date,description,amount,account,category') "exit $($run.ExitCode) rows $($rows.Count)"
            Compare-LedgerExport $rows $uniqueRows; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'report.quarter' -Description 'report --from 2026-01-01 --to 2026-03-31 equals quarter totals' -Test {
            $run = Invoke-Ledger $Stage 'report-quarter' @('report', '--db', $ledger.Db, '--from', '2026-01-01', '--to', '2026-03-31', '--format', 'json')
            Assert-That ($run.ExitCode -eq 0) "exit $($run.ExitCode) $($run.Errors)"
            $json = $run.Output | ConvertFrom-Json -Depth 20
            Assert-That ($json.from -eq '2026-01-01' -and $json.to -eq '2026-03-31') 'from/to fields missing'
            Compare-Report $json $expectedQuarter 'Q1'; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'report.exclusive-options' -Description '--month with --from -> exit 2' -Test {
            $run = Invoke-Ledger $Stage 'report-conflict' @('report', '--db', $ledger.Db, '--month', '2026-01', '--from', '2026-01-01', '--to', '2026-01-31')
            Assert-That ($run.ExitCode -eq 2) "exit $($run.ExitCode)"; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'help' -Description '--help lists import, categorize, report, budget and export' -Test {
            $run = Invoke-Ledger $Stage 'help' @('--help')
            $missing = @('import', 'categorize', 'report', 'budget', 'export' | Where-Object { $run.Output -notmatch "\b$_\b" })
            Assert-That ($run.ExitCode -eq 0 -and $missing.Count -eq 0) "missing: $($missing -join ', ')"; $true })
}

function Compare-LedgerExport([object[]]$Actual, [object[]]$Expected) {
    $actualRows = [string[]]@($Actual | ForEach-Object { '{0}|{1}|{2}|{3}|{4}' -f $_.date, $_.description, $_.amount, $_.account, $_.category })
    $expectedRows = [string[]]@($Expected | ForEach-Object { '{0}|{1}|{2}|{3}|{4}' -f $_.Date, $_.Description, (Format-Money $_.Amount), $_.Account, (Get-Category $_.Description) })
    [array]::Sort($actualRows, [System.StringComparer]::Ordinal)
    [array]::Sort($expectedRows, [System.StringComparer]::Ordinal)
    Assert-That (($actualRows -join "`n") -ceq ($expectedRows -join "`n")) 'exported transaction fields differ from the fixture'
    $keys = [string[]]@($Actual | ForEach-Object { '{0}|{1}' -f $_.date, $_.description })
    $sortedKeys = [string[]]$keys.Clone(); [array]::Sort($sortedKeys, [System.StringComparer]::Ordinal)
    Assert-That (($keys -join "`n") -ceq ($sortedKeys -join "`n")) 'export is not ordered by date then description'
}

function Assert-LedgerFixture([string]$RelativePath, [string]$Expected) {
    $path = Join-Path $ws $RelativePath
    if (-not (Test-Path -LiteralPath $path)) { return }
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Incompatible existing ledger project: $RelativePath is not a file; nothing will be replaced." }
    $actual = [IO.File]::ReadAllText($path).Replace("`r`n", "`n").TrimEnd("`n")
    if ($actual -cne $Expected.Replace("`r`n", "`n").TrimEnd("`n")) {
        throw "Incompatible existing ledger project: $RelativePath differs from the scenario's protected fixture; nothing will be replaced. Use a fresh project directory for this fixture."
    }
}

function Initialize-LedgerProject {
    if ($ctx.ReuseProject) {
        foreach ($relative in 'pom.xml', 'src/main/java/io/vcp/ledger/LedgerApp.java') {
            if (-not (Test-Path -LiteralPath (Join-Path $ws $relative) -PathType Leaf)) {
                throw "Incompatible existing ledger project: missing $relative; choose an existing Scenario C project or an empty directory."
            }
        }
        foreach ($relative in 'samples/transactions-2026Q1.csv', 'samples/rules.csv') { Assert-LedgerFixture $relative $seed[$relative] }
        Assert-LedgerFixture 'src/test/java/io/vcp/ledger/RegressionTest.java' $regressionTest
    }
    Write-SeedFiles -Root $ws -Files $seed -MissingOnly:$ctx.ReuseProject
}

function Add-LedgerRegressionTests {
    $relative = 'src/test/java/io/vcp/ledger/RegressionTest.java'
    if ($ctx.ReuseProject) { Assert-LedgerFixture $relative $regressionTest }
    Write-SeedFiles -Root $ws -Files @{ $relative = $regressionTest } -MissingOnly:$ctx.ReuseProject
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

$regressionNames = @('parenthesizedAmountsAreNegative', 'thousandsSeparatorsAreParsed', 'quotedDescriptionWithCommaIsCategorized', 'reimportReportsAllRowsAsDuplicates', 'emptyMonthReportsZeros')

$exitCode = 1
try {
    Invoke-CommonPreflight $ctx

    # --- B0 ---------------------------------------------------------------
    $stage = 'B0-baseline'
    Write-Step $ctx "B0 seed Maven project (JDK $javaMajor), resolve dependencies, baseline verify" 'phase'
    Initialize-LedgerProject
    $ctx.Notes.Add("Fixture: $($fixtureRows.Count) rows, $($uniqueRows.Count) unique, $duplicateCount duplicates, $uncategorizedCount uncategorized.")
    $mavenVersion = Invoke-Maven $stage 'maven-version' @('--version')
    if ($mavenVersion.ExitCode -ne 0 -or $mavenVersion.Output -notmatch 'Apache Maven (\d+\.\d+\.\d+)' -or [version]$Matches[1] -lt [version]'3.9.0') {
        throw "Apache Maven 3.9+ is required: $($mavenVersion.Output) $($mavenVersion.Errors)"
    }
    Test-MavenVerify $stage 1
    if ((Get-FailedGates $ctx $stage).Count) { throw 'Baseline Maven build failed; fix the JDK/Maven setup before spending on VCP turns.' }
    Initialize-GitCheckpoint $ctx
    $protected = @{
        'samples/transactions-2026Q1.csv' = (Get-Sha256 (Join-Path $ws 'samples\transactions-2026Q1.csv'))
        'samples/rules.csv'               = (Get-Sha256 (Join-Path $ws 'samples\rules.csv'))
    }
    $existingRegression = 'src/test/java/io/vcp/ledger/RegressionTest.java'
    if (Test-Path -LiteralPath (Join-Path $ws $existingRegression) -PathType Leaf) {
        $protected[$existingRegression] = Get-Sha256 (Join-Path $ws $existingRegression)
    }

    # --- Profiles ---------------------------------------------------------
    $stage = 'P1-profiles'
    $javaProcesses = @(
        (New-ProcessProfile -Name 'java' -Executable $java -Ctx $ctx -MaxTimeoutMs 1200000),
        (New-ProcessProfile -Name 'javac' -Executable $javac -Ctx $ctx -MaxTimeoutMs 600000))
    $affected = @('README.md', 'pom.xml', 'src', 'samples')
    $profileMain = New-ScenarioProfile -Ctx $ctx -Name 'profile-main' -AffectedPaths $affected -Processes $javaProcesses
    $profileShort = New-ScenarioProfile -Ctx $ctx -Name 'profile-short' -AffectedPaths $affected -Processes $javaProcesses -DeadlineSeconds $ctx.ShortDeadlineSeconds
    $profileReview = New-ScenarioProfile -Ctx $ctx -Name 'profile-review' -AffectedPaths $affected -MaximumAutonomy 'plan' -AutomaticEffects @('read')
    foreach ($pair in @(@('main', $profileMain), @('short', $profileShort), @('review', $profileReview))) { [void](Test-ProfileCheck $ctx $stage $pair[1] $pair[0]) }

    # --- G0: zero-spend guardrail: empty task file -------------------------
    $emptyPrompt = Join-Path $ctx.Logs 'G0-guardrail\empty-task.md'
    Write-Utf8File $emptyPrompt ''
    Invoke-GuardrailRun -Ctx $ctx -Stage 'G0-guardrail' -Id 'empty-task-file' -Config $profileMain `
        -Description 'run --file with an empty task file is rejected, exit 2, no task' `
        -Arguments @('run', '--file', $emptyPrompt, '--budget-usd', '0.01', '--autonomy', 'autonomous')

    if ($ctx.SkipPaidStages) {
        Write-Step $ctx 'Dry run (-SkipPaidStages): toolchain, seed, baseline, profiles and guardrail verified; stopping before paid stages.' 'ok'
        throw 'VCP_SCENARIO_DRY_RUN_COMPLETE'
    }

    # --- T1..T3 -------------------------------------------------------------
    $gatesT1 = { param($s) Test-MavenVerify $s 4; Test-Import $s; Test-ProtectedUnchanged $s $protected }
    $t1 = Invoke-VcpTask -Ctx $ctx -Stage 'T1-import' -Title 'CSV import into JSON ledger' -Prompt $promptT1 -Config $profileMain
    if ($t1) { Test-StageExit $ctx $t1 'T1-import'; & $gatesT1 'T1-import'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T1-import' -Config $profileMain -GateScript $gatesT1) }
    Save-Checkpoint $ctx 'T1: import'

    $gatesT2 = { param($s) Test-MavenVerify $s 7; Test-Reports $s; Test-Import $s; Test-ProtectedUnchanged $s $protected }
    $t2 = Invoke-VcpTask -Ctx $ctx -Stage 'T2-reports' -Title 'Categorize and JSON monthly report' -Prompt $promptT2 -Config $profileMain
    if ($t2) { Test-StageExit $ctx $t2 'T2-reports'; & $gatesT2 'T2-reports'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T2-reports' -Config $profileMain -GateScript $gatesT2) }
    Save-Checkpoint $ctx 'T2: categorize and reports'

    $gatesT3 = { param($s) Test-MavenVerify $s 10; Test-TableAndBudgets $s; Test-Reports $s; Test-ProtectedUnchanged $s $protected }
    $t3 = Invoke-VcpTask -Ctx $ctx -Stage 'T3-budgets' -Title 'Table output, budgets, exit codes' -Prompt $promptT3 -Config $profileMain
    if ($t3) { Test-StageExit $ctx $t3 'T3-budgets'; & $gatesT3 'T3-budgets'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T3-budgets' -Config $profileMain -GateScript $gatesT3) }
    Save-Checkpoint $ctx 'T3: table, budgets, exit codes'

    # --- T4: protected regression tests -------------------------------------
    Add-LedgerRegressionTests
    Save-Checkpoint $ctx 'T4 setup: protected regression tests added by harness'
    $protected['src/test/java/io/vcp/ledger/RegressionTest.java'] = Get-Sha256 (Join-Path $ws 'src\test\java\io\vcp\ledger\RegressionTest.java')
    $gatesT4 = { param($s) Test-MavenVerify $s 15 $regressionNames; Test-BankExport $s; Test-Reports $s; Test-ProtectedUnchanged $s $protected }
    $t4 = Invoke-VcpTask -Ctx $ctx -Stage 'T4-regressions' -Title 'Make protected regression tests pass' -Prompt $promptT4 -Config $profileMain
    if ($t4) { Test-StageExit $ctx $t4 'T4-regressions'; & $gatesT4 'T4-regressions'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T4-regressions' -Config $profileMain -GateScript $gatesT4) }
    Save-Checkpoint $ctx 'T4: regression fixes'

    # --- T5: short deadline then 'sessions resume <session>' ---------------
    $gatesT5 = { param($s) Test-MavenVerify $s 17 $regressionNames; Test-ExportAndRange $s; Test-BankExport $s; Test-ProtectedUnchanged $s $protected }
    $t5 = Invoke-VcpTask -Ctx $ctx -Stage 'T5-export' -Title 'Export and date ranges (short deadline)' -Prompt $promptT5 -Config $profileShort -AcceptExit @(0, 3, 8)
    if ($t5) {
        Test-StageExit $ctx $t5 'T5-export'
        if ($t5.exit_code -eq 8 -and $t5.session) {
            $resumed = Invoke-VcpContinuation -Ctx $ctx -Stage 'T5-resume' -Title "sessions resume $($t5.session)" -Arguments @('sessions', 'resume', $t5.session) -Config $profileMain -AcceptExit @(0, 3)
            if ($resumed) {
                Test-StageExit $ctx $resumed 'T5-resume'
                [void](Invoke-Gate -Ctx $ctx -Stage 'T5-resume' -Id 'resume-same-task' -Description 'sessions resume continued the paused T5 task' -Test {
                        Assert-That ($resumed.task -eq $t5.task) "resumed '$($resumed.task)' != paused '$($t5.task)'"; $true })
            }
        }
        else { [void](Skip-Gate $ctx 'T5-resume' 'resume-same-task' 'sessions resume continued the paused T5 task' "T5 ended with exit $($t5.exit_code); continuation not exercised") }
        & $gatesT5 'T5-export'
        [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T5-export' -Config $profileMain -GateScript $gatesT5)
    }
    Save-Checkpoint $ctx 'T5: export and date ranges'

    # --- T6 -----------------------------------------------------------------
    [void](Invoke-PlanModeReview -Ctx $ctx -Stage 'T6-review' -Config $profileReview -Prompt $promptReview)

    # --- FINAL --------------------------------------------------------------
    $stage = 'FINAL'
    Write-Step $ctx 'FINAL independent verification and packaging' 'phase'
    Test-MavenVerify $stage 17 $regressionNames
    Test-Import $stage
    Test-Reports $stage
    Test-TableAndBudgets $stage
    Test-BankExport $stage
    Test-ExportAndRange $stage
    Test-ProtectedUnchanged $stage $protected
    $artifacts = Join-Path $(if ($ctx.ReuseProject) { $ctx.Root } else { $ws }) 'artifacts'
    New-Item -ItemType Directory -Force -Path $artifacts | Out-Null
    $jar = Join-Path $ws 'target\ledger-cli-1.0.0-all.jar'
    [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'final-artifacts' -Description 'built jar generates the complete sample artifact set successfully' -Test {
        Assert-That (Test-Path -LiteralPath $jar) 'final shaded jar missing'
        Copy-Item -LiteralPath $jar -Destination $artifacts -Force
        Add-Asset $ctx (Join-Path $artifacts 'ledger-cli-1.0.0-all.jar') 'Executable shaded jar (java -jar)'
        Add-Asset $ctx (Join-Path $ws 'target\ledger-cli-1.0.0.jar') 'Thin application jar'
        $db = Join-Path $artifacts 'sample-ledger.json'
        Remove-Item -LiteralPath $db -ErrorAction SilentlyContinue
        $import = Invoke-Ledger $stage 'sample-import' @('import', '--input', (Join-Path $ws 'samples\transactions-2026Q1.csv'), '--db', $db)
        Assert-That ($import.ExitCode -eq 0) "sample import exit $($import.ExitCode): $($import.Errors)"
        $categorize = Invoke-Ledger $stage 'sample-categorize' @('categorize', '--db', $db, '--rules', (Join-Path $ws 'samples\rules.csv'))
        Assert-That ($categorize.ExitCode -eq 0) "sample categorize exit $($categorize.ExitCode): $($categorize.Errors)"
        $report = Invoke-Ledger $stage 'sample-report' @('report', '--db', $db, '--month', '2026-02', '--format', 'json')
        Assert-That ($report.ExitCode -eq 0) "sample report exit $($report.ExitCode): $($report.Errors)"
        Compare-Report ($report.Output | ConvertFrom-Json -Depth 20) $expectedMonth['2026-02'] 'sample February'
        Write-Utf8File (Join-Path $artifacts 'report-2026-02.json') $report.Output
        $export = Invoke-Ledger $stage 'sample-export' @('export', '--db', $db, '--format', 'csv', '--output', (Join-Path $artifacts 'export-2026Q1.csv'))
        Assert-That ($export.ExitCode -eq 0) "sample export exit $($export.ExitCode): $($export.Errors)"
        Compare-LedgerExport @(Import-Csv -LiteralPath (Join-Path $artifacts 'export-2026Q1.csv')) $uniqueRows
        Write-JsonFile (Join-Path $artifacts 'expected-report-2026-02.json') $expectedMonth['2026-02']
        foreach ($name in 'sample-ledger.json', 'report-2026-02.json', 'export-2026Q1.csv', 'expected-report-2026-02.json') { Add-Asset $ctx (Join-Path $artifacts $name) 'Pipeline output from the built jar' }
        $true })
    Save-Checkpoint $ctx 'FINAL: verified state'

    Invoke-FinalEvidenceSweep $ctx 'ledger'
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
