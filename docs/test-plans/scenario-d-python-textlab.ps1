#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Scenario D - textlab: a Python machine-learning project for support-ticket text
analysis (category and sentiment classification, keyword extraction, reports),
built over multiple VCP CLI turns and assessed on a hidden holdout set.

.DESCRIPTION
See docs/test-plans/cli-test-plans.md, "Scenario D". The harness generates
deterministic labelled train/dev data inside the workspace and a holdout set
(with unseen phrasings) outside it. Metrics are independently scored from batch
predictions. This is a withheld fixture, not a security boundary or a blind
evaluation: repair feedback can expose results across turns. Turns: baseline classifier and evaluation, sentiment and
keywords, robustness, protected preprocessing regression tests, HTML report
and model card (interrupted by a short deadline, then resumed with
'vcp resume <task> --expected-revision <rev>' from 'vcp workspace discover'),
a plan-mode review and a 'vcp sessions fork' of that review. It finishes with
retrained models, holdout metrics, a wheel and compiled bytecode.

.EXAMPLE
pwsh -File .\scenario-d-python-textlab.ps1 -ProviderGeneration C:\vcp-private\provider-20261002
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ProviderGeneration,
    [string]$RunRoot = (Join-Path $env:SystemDrive 'vcp-scenarios'),
    [string]$Vcp,
    [decimal]$TurnBudgetUsd = 3,
    [decimal]$MaxScenarioUsd = 30,
    [int]$MaxRepairTurns = 1,
    [int]$OutputTokens = 8192,
    [int]$MaxRequests = 96,
    [int]$DeadlineSeconds = 1800,
    [int]$ShortDeadlineSeconds = 150,
    [ValidateRange(0, 1)][double]$BaselineMacroF1 = 0.80,
    [ValidateRange(0, 1)][double]$TargetMacroF1 = 0.85,
    [ValidateRange(0, 1)][double]$TargetSentimentAccuracy = 0.70,
    [switch]$SkipPaidStages
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force

$ctx = Initialize-VcpScenario -Name 'd-python-textlab' -RunRoot $RunRoot -Vcp $Vcp -ProviderGeneration $ProviderGeneration `
    -TurnBudgetUsd $TurnBudgetUsd -MaxScenarioUsd $MaxScenarioUsd -MaxRepairTurns $MaxRepairTurns -OutputTokens $OutputTokens `
    -MaxRequests $MaxRequests -DeadlineSeconds $DeadlineSeconds -ShortDeadlineSeconds $ShortDeadlineSeconds -SkipPaidStages:$SkipPaidStages
$ws = $ctx.Workspace
$inv = [System.Globalization.CultureInfo]::InvariantCulture

#region Toolchain

function Resolve-BasePython {
    $launcher = Find-Executable -Name 'py'
    if ($launcher) {
        $path = (& $launcher -3 -c 'import sys; print(sys.executable)' 2>$null | Select-Object -First 1)
        if ($path -and (Test-Path -LiteralPath $path.Trim())) { return $path.Trim() }
    }
    $candidate = Get-Command 'python' -CommandType Application -ErrorAction SilentlyContinue |
        Where-Object { $_.Source -like '*.exe' -and $_.Source -notlike '*WindowsApps*' } | Select-Object -First 1
    if ($candidate) { return $candidate.Source }
    return $null
}
$basePython = Resolve-BasePython
if (-not $basePython) { throw 'Python 3.11+ is required (py launcher or python.exe on PATH, not the Store alias).' }
$pyVersion = [version](((& $basePython -c 'import platform; print(platform.python_version())') | Select-Object -First 1).Trim())
if ($pyVersion -lt [version]'3.11') { throw "Python $pyVersion found; 3.11 or later is required." }
$venv = Join-Path $ctx.Env 'venv'
$python = Join-Path $venv 'Scripts\python.exe'

function Invoke-Python([string]$Stage, [string]$Label, [string[]]$Arguments, [int]$TimeoutSeconds = 900) {
    return Invoke-Tool -Ctx $ctx -Stage $Stage -Label $Label -FilePath $python -ArgumentList $Arguments -TimeoutSeconds $TimeoutSeconds `
        -Environment @{ PYTHONHASHSEED = '0'; PYTHONIOENCODING = 'utf-8'; PIP_DISABLE_PIP_VERSION_CHECK = '1' }
}
function Get-Tail([string]$Text, [int]$Count = 40) { return (($Text -split "`r?`n") | Select-Object -Last $Count) -join "`n" }

#endregion

#region Deterministic data

$labels = @('account', 'billing', 'feedback', 'shipping', 'technical')
$templatesA = @{
    billing   = @('I was charged twice for my {plan} subscription this month', 'My invoice shows the wrong amount for {month}',
        'Please refund the payment I made on {day}', 'Why did my bill go up after the {plan} upgrade',
        'The credit card charge does not match my invoice', 'I need a copy of the receipt for my last payment')
    technical = @('The app crashes every time I open the {feature} screen', 'I get error code {code} when I try to sync',
        'The {feature} page keeps loading forever', 'After the latest update the app freezes on startup',
        'The export to PDF feature throws an error', 'Notifications stopped working on my {device}')
    account   = @('I cannot reset my password, the link has expired', 'Please change the email address on my account',
        'My account was locked after too many login attempts', 'How do I delete my account and personal data',
        'I want to update the username on my profile', 'Two factor authentication codes are not arriving for my login')
    shipping  = @('My package has not arrived and tracking shows no update', 'The courier delivered my order to the wrong address',
        'When will my order {order} be shipped', 'The tracking number for my delivery is invalid',
        'My parcel arrived damaged, the box was crushed', 'Can I change the delivery address for order {order}')
    feedback  = @('I love the new design of the {feature} page', 'It would be great to have a dark mode option',
        'Suggestion: allow exporting reports to Excel', 'The new {feature} feature is really useful for my team',
        'Please add an option to customize the dashboard', 'Your onboarding guide could be clearer for beginners')
}
$templatesB = @{
    billing   = @('There is an unexpected fee on my statement for {month}', 'Can you explain the extra charge on my {plan} billing')
    technical = @('The {feature} button does nothing when I tap it on my {device}', 'Sync fails with a timeout error since yesterday')
    account   = @('I am unable to sign in even with the correct password', 'Please merge my two accounts into one profile')
    shipping  = @('The shipment for order {order} is stuck at the depot', 'My delivery was marked as delivered but nothing came')
    feedback  = @('Idea: let us schedule reports to be sent weekly', 'I really like how fast the {feature} view is now')
}
$modifiersA = @{
    negative = @('This is really frustrating.', 'I am very disappointed.', 'This is the third time I am asking and I am angry.',
        'Terrible experience so far.', 'Unacceptable, please fix this now.')
    neutral  = @('', '', 'Thanks.', 'Details are below.', 'Let me know if you need anything else.')
    positive = @('Thanks so much, you are great!', 'I appreciate the quick help.', 'Love your product, keep it up!', 'Great job, thank you.')
}
$modifiersB = @{
    negative = @('I am extremely annoyed about this.', 'Really poor service, I expected better.')
    neutral  = @('', 'Please advise.')
    positive = @('You folks are wonderful, thanks!', 'Amazing support as always.')
}
$slots = [ordered]@{
    '{plan}'    = @('Pro', 'Basic', 'Team', 'Enterprise')
    '{month}'   = @('January', 'February', 'March', 'April')
    '{day}'     = @('Monday', 'Friday', 'the 3rd', 'the 15th')
    '{feature}' = @('dashboard', 'settings', 'calendar', 'projects', 'search')
    '{code}'    = @('E401', 'E503', '0x80070005', 'ERR_SYNC_12')
    '{device}'  = @('iPhone', 'Android tablet', 'Windows laptop', 'MacBook')
}
$signalWords = @{
    billing   = @('charged', 'invoice', 'refund', 'payment', 'bill', 'charge', 'receipt', 'credit')
    technical = @('crashes', 'error', 'loading', 'freezes', 'update', 'notifications', 'sync', 'app')
    account   = @('password', 'email', 'account', 'locked', 'login', 'delete', 'username', 'authentication')
    shipping  = @('package', 'tracking', 'courier', 'delivery', 'order', 'parcel', 'shipped', 'address')
    feedback  = @('love', 'design', 'dark', 'suggestion', 'useful', 'option', 'customize', 'onboarding')
}

function New-TicketSet([int]$Seed, [int]$Count, [double]$UnseenShare, [string]$Prefix) {
    $rng = [System.Random]::new($Seed)
    $sentiments = @('negative', 'neutral', 'positive')
    $rows = [System.Collections.Generic.List[object]]::new()
    for ($i = 0; $i -lt $Count; $i++) {
        $category = $labels[$i % $labels.Count]
        $unseen = $rng.NextDouble() -lt $UnseenShare
        $pool = if ($unseen) { $templatesB[$category] } else { $templatesA[$category] }
        $core = $pool[$rng.Next(0, $pool.Count)]
        foreach ($slot in $slots.Keys) { while ($core.Contains($slot)) { $core = ([regex]::new([regex]::Escape($slot))).Replace($core, $slots[$slot][$rng.Next(0, $slots[$slot].Count)], 1) } }
        $core = $core.Replace('{order}', '#' + $rng.Next(10000, 100000).ToString($inv))
        $sentiment = $sentiments[$rng.Next(0, 3)]
        $mods = if ($unseen) { $modifiersB[$sentiment] } else { $modifiersA[$sentiment] }
        $modifier = $mods[$rng.Next(0, $mods.Count)]
        $text = if (-not $modifier) { "$core." } elseif ($rng.Next(0, 2) -eq 0) { "$modifier $core." } else { "$core. $modifier" }
        $roll = $rng.NextDouble()
        if ($roll -lt 0.10) { $text = "Hi team, $text" } elseif ($roll -lt 0.20) { $text = "$text Regards, Sam" }
        if ($rng.NextDouble() -lt 0.25) { $text = $text.ToLowerInvariant() }
        $rows.Add([pscustomobject]@{ id = ('{0}-{1:D5}' -f $Prefix, $i); text = $text; category = $category; sentiment = $sentiment })
    }
    # Deterministic shuffle so labels are not in a fixed rotation.
    for ($i = $rows.Count - 1; $i -gt 0; $i--) { $j = $rng.Next(0, $i + 1); $tmp = $rows[$i]; $rows[$i] = $rows[$j]; $rows[$j] = $tmp }
    return , $rows
}

$trainRows = New-TicketSet 1301 1500 0.0 'tr'
$devRows = New-TicketSet 1302 300 0.0 'dv'
$holdoutRows = New-TicketSet 2701 500 0.35 'ho'
$holdoutRows.Add([pscustomobject]@{ id = 'ho-xss'; text = '<script>alert(1)</script> my invoice amount is wrong'; category = 'billing'; sentiment = 'neutral' })
$holdoutPath = Join-Path $ctx.Hidden 'tickets_holdout.csv'
$edgePath = Join-Path $ctx.Hidden 'edge_cases.csv'
$missingColumnPath = Join-Path $ctx.Hidden 'missing_text_column.csv'

#endregion

#region Seed

$seed = [ordered]@{}
$seed['README.md'] = @'
# textlab

Support-ticket triage toolkit: classify incoming tickets by category (account,
billing, feedback, shipping, technical) and sentiment (negative, neutral,
positive), extract keywords, and report model quality.

    python -m textlab --help
    python -m pytest

Data: `data/tickets_train.csv` and `data/tickets_dev.csv` with columns
`id,text,category,sentiment`. Models are written to a model directory
(`models/` by default) and must be reproducible from the same data and seed.

Stack: Python 3.11+, scikit-learn, numpy, joblib, pytest. Package code lives in
`src/textlab` (installed in editable mode in the development environment).
'@
$seed['.gitignore'] = @'
__pycache__/
.pytest_cache/
*.egg-info/
build/
dist/
models/
models-*/
reports/
artifacts/
'@
$seed['pyproject.toml'] = @'
[build-system]
requires = ["setuptools>=69", "wheel"]
build-backend = "setuptools.build_meta"

[project]
name = "textlab"
version = "0.1.0"
description = "Support ticket triage: category and sentiment classification with keyword extraction."
requires-python = ">=3.11"
dependencies = ["scikit-learn>=1.5", "numpy>=1.26", "joblib>=1.4"]

[project.optional-dependencies]
dev = ["pytest>=8"]

[project.scripts]
textlab = "textlab.__main__:main"

[tool.setuptools.packages.find]
where = ["src"]

[tool.pytest.ini_options]
testpaths = ["tests"]
'@
$seed['src/textlab/__init__.py'] = @'
"""Support ticket triage toolkit."""

__version__ = "0.1.0"
'@
$seed['src/textlab/__main__.py'] = @'
"""Command-line entry point: python -m textlab <command>."""

import argparse
import sys

from textlab import __version__


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="textlab", description="Support ticket triage toolkit.")
    parser.add_argument("--version", action="version", version=f"textlab {__version__}")
    parser.add_subparsers(dest="command")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.command is None:
        parser.print_help()
    return 0


if __name__ == "__main__":
    sys.exit(main())
'@
$seed['tests/test_cli.py'] = @'
import pytest

from textlab.__main__ import main


def test_version_flag(capsys):
    with pytest.raises(SystemExit) as exit_info:
        main(["--version"])
    assert exit_info.value.code == 0
    assert "textlab 0.1.0" in capsys.readouterr().out
'@

$regressionTests = @'
# PROTECTED FILE - added by the scenario harness as an acceptance test. Do not edit.
from textlab.preprocess import normalize_text


def test_fullwidth_and_case_are_normalized():
    assert normalize_text("ＲＥＦＵＮＤ  Please") == "refund please"


def test_urls_are_masked():
    assert normalize_text("see https://example.com/a?b=1 now") == "see <url> now"


def test_emails_are_masked():
    assert normalize_text("Mail me at Jane.Doe@Example.com") == "mail me at <email>"


def test_order_numbers_are_masked():
    assert normalize_text("Order #48213 is late") == "order <order> is late"


def test_whitespace_only_text_normalizes_to_empty():
    assert normalize_text(" \t\n ") == ""
'@

#endregion

#region Prompts

$environmentBlock = @'

## Environment and rules (applies to every task in this project)

- Work only inside the current workspace. Read README.md and the existing code first.
- Process profile available to `vcp_exec` (no shell; literal arguments): `python` runs Python
  {{PY}} in a prepared virtual environment where this project is installed in editable mode with
  scikit-learn, numpy, joblib and pytest. Examples: `["-m", "pytest", "-q"]`,
  `["-m", "textlab", "train", "--train", "data/tickets_train.csv", "--dev", "data/tickets_dev.csv",
  "--model-dir", "models", "--seed", "13"]`. Environment variables cannot be set for processes.
- Add a dependency only if essential: declare it in pyproject.toml with a lower bound, install it
  with `["-m", "pip", "install", "-e", ".[dev]"]`, and explain why.
- Determinism: the same data and `--seed` must produce identical predictions and an identical
  `metadata.json` (no timestamps, absolute paths or host names in model outputs).
- CLI exit codes: 0 success, 2 usage error (argparse), 4 input error (missing file or required
  column; message on stderr). Machine-readable outputs are UTF-8 JSON or CSV.
- Protected files (never edit, rename or delete): `data/tickets_train.csv`, `data/tickets_dev.csv`{{PROTECTED}}.
- Before finishing, run the tests and a training run on the provided data and fix any failure.
  Finish with a short summary of changed files, dev metrics and command results.
'@
$environmentBlock = $environmentBlock.Replace('{{PY}}', [string]$pyVersion)
function New-Prompt([string]$Body, [string]$Protected = '') { return $Body + $environmentBlock.Replace('{{PROTECTED}}', $Protected) }

$promptT1 = New-Prompt @'
# Task T1 - Data loading, preprocessing and a category classifier

Implement the first version of textlab in `src/textlab`:

- `textlab.preprocess.normalize_text(text: str) -> str`: Unicode NFKC, casefold, collapse all
  whitespace to single spaces, strip.
- Data loading for CSV files with columns `id,text,category,sentiment` (UTF-8, optional BOM, quoted
  fields may contain commas and newlines). Missing file or missing `text` column -> exit 4.
- A category classifier (scikit-learn pipeline of your choice, e.g. TF-IDF + linear model) with a
  fixed random seed.
- `python -m textlab train --train <csv> --dev <csv> --model-dir <dir> --seed <int>` writes
  `<dir>/category.joblib` and `<dir>/metadata.json` =
  `{"version":1,"seed":13,"labels":{"category":[sorted labels]},"dev_metrics":{"category_macro_f1":x}}`
  and prints the same metrics as one JSON line.
- `python -m textlab evaluate --model-dir <dir> --data <csv> --output <metrics.json>` writes
  `{"n":N,"category":{"accuracy":a,"macro_f1":f,"labels":[...],"per_label":{"<label>":{"precision":p,
  "recall":r,"f1":f,"support":s}},"confusion_matrix":[[...]]}}` (rows = true labels, columns =
  predicted, both in `labels` order).
- `python -m textlab predict --model-dir <dir> --text "<text>"` prints
  `{"category":"...","category_confidence":c}`; `predict --model-dir <dir> --input <csv> --output <csv>`
  writes columns `id,category,category_confidence` in input order.
- pytest tests for normalization, loading, training and the CLI (at least 6 tests).
'@
$promptT2 = New-Prompt @'
# Task T2 - Sentiment model and keyword extraction

- Train a sentiment model (negative, neutral, positive) alongside the category model:
  `train` also writes `<dir>/sentiment.joblib`; metadata gains `labels.sentiment` and
  `dev_metrics.sentiment_accuracy`.
- `evaluate` adds `"sentiment":{"accuracy":a,"macro_f1":f}`.
- `predict --text` output becomes
  `{"category","category_confidence","sentiment","sentiment_confidence","keywords":[up to 5 terms]}`;
  batch predict adds columns `sentiment,sentiment_confidence`.
- `python -m textlab keywords --data <csv> --top <k> --output <json>` writes
  `{"<category>":[k terms], ...}` for every category: lowercase single words, no English stop words,
  ordered by how strongly they indicate that category (for example chi-squared or class-conditional
  TF-IDF weight).
- Tests for both models, keyword extraction and the extended CLI output.
'@
$promptT3 = New-Prompt @'
# Task T3 - Robustness and calibrated confidences

Make prediction safe for real inbound tickets:

- Empty or whitespace-only text -> `category "unknown"`, `sentiment "neutral"`, confidences 0.0 (no
  exception). Texts longer than 5000 characters are truncated before vectorizing.
- Confidences are probabilities in [0, 1] (calibrate or use `predict_proba`).
- `predict --min-confidence <x>` (default 0): when category_confidence < x the category is
  `needs_review` (empty-text rows stay `unknown`).
- Batch input tolerates a UTF-8 BOM, quoted multi-line fields, emoji and non-English text, and
  preserves row order and ids. A CSV without a `text` column -> exit 4 with a clear message.
- Improve accuracy where you can without touching the data files; report dev metrics before and after.
- Tests for each edge case above.
'@
$promptT4 = New-Prompt @'
# Task T4 - Make the protected preprocessing regression tests pass

A teammate added `tests/test_regressions.py`. It specifies normalization required for production
tickets: masking of URLs (`<url>`), email addresses (`<email>`) and order numbers like `#48213`
(`<order>`), in addition to NFKC, casefolding and whitespace handling. Make every test in it pass by
changing the implementation only; the file is protected. Retrain and confirm dev metrics did not
regress; keep all other tests passing.
'@ '`, `tests/test_regressions.py`'
$promptT5 = New-Prompt @'
# Task T5 - Evaluation report and model card

- `python -m textlab report --model-dir <dir> --data <csv> --output <html>` writes one self-contained
  HTML file (inline CSS only, no scripts, no external URLs) with: overall metrics for both models,
  a per-label precision/recall/F1 table, the category confusion matrix as an HTML table (true labels
  as rows), top keywords per category, and up to 10 misclassified examples. All ticket text must be
  HTML-escaped.
- Add `MODEL_CARD.md` with the sections `## Intended use`, `## Training data`, `## Metrics`,
  `## Limitations` and `## Ethical considerations`, filled with this project's facts (dev metrics from
  an actual training run).
- Tests for the report (structure and escaping).
'@ '`, `tests/test_regressions.py`'
$promptReview = @'
# Task T6 - Read-only ML engineering review

Do not modify, create or delete any file. Review this project before it is used to route real
customer tickets: data leakage and evaluation methodology, reproducibility, robustness to unusual
input, calibration and thresholds, privacy (PII in logs, reports and models), packaging and tests.
Cite file paths and line numbers.

End your answer with one fenced ```json block of the form
{"findings":[{"severity":"high|medium|low","file":"path","line":n,"title":"...","recommendation":"..."}]}
listing at most 10 findings ordered by severity.
'@

#endregion

#region Gates

function Test-Pytest([string]$Stage, [int]$MinTests, [string[]]$Required = @()) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'pytest' -Description "pytest passes with >= $MinTests tests" -Test {
            $xml = Join-Path $ctx.Logs "$Stage\pytest-junit.xml"
            if (Test-Path -LiteralPath $xml) { Remove-Item -LiteralPath $xml }
            $run = Invoke-Python $Stage 'pytest' @('-m', 'pytest', '-q', '-p', 'no:cacheprovider', "--junitxml=$xml")
            Assert-That (Test-Path -LiteralPath $xml) ("no junit report (exit {0})`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $report = [xml](Get-Content -LiteralPath $xml -Raw)
            $suites = @($report.SelectNodes('//testsuite'))
            $failures = ($suites | ForEach-Object { [int]$_.failures + [int]$_.errors } | Measure-Object -Sum).Sum
            Assert-That ($run.ExitCode -eq 0 -and $failures -eq 0) ("exit {0}, {1} failures`n{2}" -f $run.ExitCode, $failures, (Get-Tail $run.Output 40))
            $passed = @($report.SelectNodes('//testcase') | Where-Object { -not $_.SelectSingleNode('failure|error|skipped') } | ForEach-Object name)
            Assert-That ($passed.Count -ge $MinTests) "only $($passed.Count) passing tests"
            $missing = @($Required | Where-Object { $passed -notcontains $_ })
            Assert-That ($missing.Count -eq 0) ('required tests not passing: ' + ($missing -join ', ')); $true })
}

function New-GateDir([string]$Stage, [string]$Name) {
    $directory = Join-Path $ctx.Temp ("$Stage-$Name-" + [guid]::NewGuid().ToString('N').Substring(0, 6))
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    return $directory
}

function Invoke-Train([string]$Stage, [string]$ModelDir, [string]$Label = 'train') {
    return Invoke-Python $Stage $Label @('-m', 'textlab', 'train', '--train', 'data/tickets_train.csv', '--dev', 'data/tickets_dev.csv', '--model-dir', $ModelDir, '--seed', '13') 1800
}

function Assert-Probability($Value, [string]$Label) {
    $number = 0.0
    Assert-That ($null -ne $Value -and [double]::TryParse([string]$Value, [System.Globalization.NumberStyles]::Float,
            [System.Globalization.CultureInfo]::InvariantCulture, [ref]$number) -and
        [double]::IsFinite($number) -and $number -ge 0 -and $number -le 1) "$Label must be a finite probability in [0,1]"
}

function Get-PredictionMetrics([object[]]$Rows, [object[]]$Expected, [string[]]$Labels, [string]$Field) {
    Assert-That ($Rows.Count -eq $Expected.Count -and $Rows.Count -gt 0) 'batch output row count differs from input'
    $matrix = @{}
    foreach ($actual in $Labels) {
        foreach ($predicted in $Labels) { $matrix["$actual|$predicted"] = 0 }
    }
    for ($i = 0; $i -lt $Rows.Count; $i++) {
        Assert-That ($Rows[$i].id -ceq $Expected[$i].id) "batch output id/order differs at row $i"
        $truth = [string]$Expected[$i].$Field; $prediction = [string]$Rows[$i].$Field
        Assert-That ($Labels -ccontains $truth -and $Labels -ccontains $prediction) "invalid $Field label at row $i"
        Assert-Probability $Rows[$i].("${Field}_confidence") "$Field confidence at row $i"
        $matrix["$truth|$prediction"]++
    }
    $correct = 0; $f1Sum = 0.0
    $perLabel = @{}
    $confusion = [System.Collections.Generic.List[object]]::new()
    foreach ($label in $Labels) {
        $tp = $matrix["$label|$label"]; $correct += $tp
        $support = 0; $predictedCount = 0
        foreach ($other in $Labels) { $support += $matrix["$label|$other"]; $predictedCount += $matrix["$other|$label"] }
        $precision = if ($predictedCount) { $tp / $predictedCount } else { 0.0 }
        $recall = if ($support) { $tp / $support } else { 0.0 }
        $f1 = if ($support + $predictedCount) { 2.0 * $tp / ($support + $predictedCount) } else { 0.0 }
        $f1Sum += $f1
        $perLabel[$label] = @{ precision = $precision; recall = $recall; f1 = $f1; support = $support }
        $confusion.Add(@($Labels | ForEach-Object { $matrix["$label|$_"] }))
    }
    return @{ accuracy = $correct / $Rows.Count; macro_f1 = $f1Sum / $Labels.Count; per_label = $perLabel; confusion_matrix = $confusion }
}

function Assert-ReportedMetrics($Reported, $Computed, [string]$Label) {
    foreach ($field in 'accuracy', 'macro_f1') {
        Assert-Probability $Reported.$field "$Label $field"
        Assert-That ([math]::Abs([double]$Reported.$field - $Computed[$field]) -le 0.000001) "$Label $field differs from independently scored predictions"
    }
}

function Test-Model([string]$Stage, [double]$MinMacroF1, [switch]$Sentiment) {
    $models = New-GateDir $Stage 'models'
    $state = @{}
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'train' -Description 'train writes models and deterministic metadata' -Test {
            $run = Invoke-Train $Stage $models
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            Assert-That (Test-Path -LiteralPath (Join-Path $models 'category.joblib')) 'category.joblib missing'
            $meta = Get-Content -LiteralPath (Join-Path $models 'metadata.json') -Raw | ConvertFrom-Json -Depth 20
            Assert-That ($meta.version -eq 1 -and $meta.seed -eq 13) 'metadata version/seed differs from the training contract'
            Assert-That ((@($meta.labels.category) -join ',') -eq ($labels -join ',')) "labels.category = $(@($meta.labels.category) -join ',')"
            if ($Sentiment) {
                Assert-That (Test-Path -LiteralPath (Join-Path $models 'sentiment.joblib')) 'sentiment.joblib missing'
                Assert-That ((@($meta.labels.sentiment) -join ',') -eq 'negative,neutral,positive') "labels.sentiment = $(@($meta.labels.sentiment) -join ',')"
            }
            $state.trained = $true; $true })
    if (-not $state.trained) { return }
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'holdout.category' -Description "hidden holdout category macro-F1 >= $MinMacroF1" -Test {
            $metricsPath = Join-Path $ctx.Logs "$Stage\holdout-metrics.json"
            $run = Invoke-Python $Stage 'evaluate-holdout' @('-m', 'textlab', 'evaluate', '--model-dir', $models, '--data', $holdoutPath, '--output', $metricsPath)
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $metrics = Get-Content -LiteralPath $metricsPath -Raw | ConvertFrom-Json -Depth 20
            $state.metrics = $metrics
            Assert-That ([int]$metrics.n -eq $holdoutRows.Count) "n = $($metrics.n), expected $($holdoutRows.Count)"
            $predictionsPath = Join-Path $models 'scored-predictions.csv'
            $predictionRun = Invoke-Python $Stage 'predict-scored' @('-m', 'textlab', 'predict', '--model-dir', $models, '--input', $holdoutPath, '--output', $predictionsPath)
            Assert-That ($predictionRun.ExitCode -eq 0) "prediction exit $($predictionRun.ExitCode): $($predictionRun.Errors)"
            $state.predictions = @(Import-Csv -LiteralPath $predictionsPath)
            $computed = Get-PredictionMetrics $state.predictions $holdoutRows $labels 'category'
            Assert-ReportedMetrics $metrics.category $computed 'category'
            Assert-That ((@($metrics.category.labels) -join ',') -ceq ($labels -join ',')) 'evaluation category labels/order differ'
            $matrix = @($metrics.category.confusion_matrix)
            Assert-That ($matrix.Count -eq $labels.Count) 'confusion matrix has the wrong row count'
            for ($i = 0; $i -lt $labels.Count; $i++) {
                Assert-That ((@($matrix[$i]) -join ',') -ceq ($computed.confusion_matrix[$i] -join ',')) "confusion matrix row $i differs from predictions"
                $label = $labels[$i]
                $reportedLabel = $metrics.category.per_label.$label
                Assert-That ($null -ne $reportedLabel -and $reportedLabel.support -eq $computed.per_label[$label].support) "support for $label differs"
                foreach ($field in 'precision', 'recall', 'f1') {
                    Assert-Probability $reportedLabel.$field "$label $field"
                    Assert-That ([math]::Abs([double]$reportedLabel.$field - $computed.per_label[$label][$field]) -le 0.000001) "$label $field differs from predictions"
                }
            }
            Assert-That ($computed.macro_f1 -ge $MinMacroF1) ('independent macro_f1 {0:N4} < {1}' -f $computed.macro_f1, $MinMacroF1); $true })
    if ($Sentiment) {
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'holdout.sentiment' -Description "hidden holdout sentiment accuracy >= $TargetSentimentAccuracy" -Test {
                Assert-That ($null -ne $state.metrics) 'no holdout metrics'
                $computed = Get-PredictionMetrics $state.predictions $holdoutRows @('negative', 'neutral', 'positive') 'sentiment'
                Assert-ReportedMetrics $state.metrics.sentiment $computed 'sentiment'
                Assert-That ($computed.accuracy -ge $TargetSentimentAccuracy) ('independent sentiment accuracy {0:N4}' -f $computed.accuracy); $true })
    }
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'determinism' -Description 'retraining with the same seed reproduces predictions and metadata byte-for-byte' -Test {
            $again = New-GateDir $Stage 'models-repro'
            $run = Invoke-Train $Stage $again 'train-repro'
            Assert-That ($run.ExitCode -eq 0) "retrain exit $($run.ExitCode)"
            $a = Join-Path $models 'holdout-predictions.csv'; $b = Join-Path $again 'holdout-predictions.csv'
            $p1 = Invoke-Python $Stage 'predict-a' @('-m', 'textlab', 'predict', '--model-dir', $models, '--input', $holdoutPath, '--output', $a)
            $p2 = Invoke-Python $Stage 'predict-b' @('-m', 'textlab', 'predict', '--model-dir', $again, '--input', $holdoutPath, '--output', $b)
            Assert-That ($p1.ExitCode -eq 0 -and $p2.ExitCode -eq 0) "predict exits $($p1.ExitCode)/$($p2.ExitCode): $($p1.Errors)"
            Assert-That ((Get-Sha256 $a) -eq (Get-Sha256 $b)) 'batch predictions differ between identical training runs'
            Assert-That ((Get-Sha256 (Join-Path $models 'metadata.json')) -eq (Get-Sha256 (Join-Path $again 'metadata.json'))) 'metadata.json differs between identical training runs'
            $rows = @(Import-Csv -LiteralPath $a)
            [void](Get-PredictionMetrics $rows $holdoutRows $labels 'category')
            if ($Sentiment) { [void](Get-PredictionMetrics $rows $holdoutRows @('negative', 'neutral', 'positive') 'sentiment') }
            $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'predict.single' -Description 'predict --text returns JSON with category and confidence in [0,1]' -Test {
            $run = Invoke-Python $Stage 'predict-text' @('-m', 'textlab', 'predict', '--model-dir', $models, '--text', 'Please refund the double charge on my invoice')
            $json = $run.Output | ConvertFrom-Json -Depth 10
            Assert-That ($run.ExitCode -eq 0 -and $json.category -eq 'billing') "exit $($run.ExitCode): $($run.Output)"
            Assert-Probability $json.category_confidence 'category confidence'
            if ($Sentiment) {
                Assert-That ($json.sentiment -in 'negative', 'neutral', 'positive' -and @($json.keywords).Count -ge 1 -and @($json.keywords).Count -le 5) "sentiment/keywords missing: $($run.Output)"
                Assert-Probability $json.sentiment_confidence 'sentiment confidence'
            }
            $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'input-errors' -Description 'missing file and missing text column -> exit 4' -Test {
            $missing = Invoke-Python $Stage 'evaluate-missing' @('-m', 'textlab', 'evaluate', '--model-dir', $models, '--data', (Join-Path $ctx.Temp 'nope.csv'), '--output', (Join-Path $ctx.Temp 'nope.json'))
            $column = Invoke-Python $Stage 'predict-missing-column' @('-m', 'textlab', 'predict', '--model-dir', $models, '--input', $missingColumnPath, '--output', (Join-Path $ctx.Temp 'nope.csv'))
            Assert-That ($missing.ExitCode -eq 4 -and $column.ExitCode -eq 4) "exits: missing file $($missing.ExitCode), missing column $($column.ExitCode)"; $true })
    return $models
}

function Test-Keywords([string]$Stage) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'keywords' -Description 'keywords: 10 lowercase terms per category, >= 2 known signal words each' -Test {
            $out = Join-Path $ctx.Logs "$Stage\keywords.json"
            $run = Invoke-Python $Stage 'keywords' @('-m', 'textlab', 'keywords', '--data', 'data/tickets_train.csv', '--top', '10', '--output', $out)
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $keywords = Get-Content -LiteralPath $out -Raw | ConvertFrom-Json -Depth 10
            foreach ($label in $labels) {
                $terms = @($keywords.$label)
                Assert-That ($terms.Count -eq 10) "$label has $($terms.Count) terms"
                Assert-That (@($terms | Where-Object { $_ -cne $_.ToLowerInvariant() -or $_ -in 'the', 'and', 'my', 'is', 'to' }).Count -eq 0) "$label has uppercase or stop words: $($terms -join ', ')"
                $hits = @($terms | Where-Object { $signalWords[$label] -contains $_ })
                Assert-That ($hits.Count -ge 2) "$label signal words: $($terms -join ', ')"
            }
            $true })
}

function Test-Robustness([string]$Stage, [string]$Models) {
    if (-not $Models) { return }
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'edge-cases' -Description 'BOM, empty, emoji, non-English, 20k chars, multi-line input handled' -Test {
            $out = Join-Path $ctx.Logs "$Stage\edge-predictions.csv"
            $run = Invoke-Python $Stage 'predict-edge' @('-m', 'textlab', 'predict', '--model-dir', $Models, '--input', $edgePath, '--output', $out)
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $rows = @(Import-Csv -LiteralPath $out)
            Assert-That ((($rows | ForEach-Object id) -join ',') -eq 'e1,e2,e3,e4,e5,e6') "ids: $(($rows | ForEach-Object id) -join ',')"
            Assert-That ($rows[0].category -eq 'unknown' -and $rows[1].category -eq 'unknown') "empty text categories: $($rows[0].category), $($rows[1].category)"
            Assert-That ($rows[4].category -eq 'billing') "20k-char refund text -> $($rows[4].category)"
            foreach ($row in $rows) {
                Assert-Probability $row.category_confidence "category confidence for $($row.id)"
                Assert-Probability $row.sentiment_confidence "sentiment confidence for $($row.id)"
            }
            foreach ($row in $rows[0..1]) {
                Assert-That ($row.sentiment -eq 'neutral' -and [double]$row.category_confidence -eq 0 -and [double]$row.sentiment_confidence -eq 0) "empty text sentiment/confidence wrong for $($row.id)"
            }
            $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'min-confidence' -Description '--min-confidence 0.999 yields needs_review rows; 0 yields none' -Test {
            $strict = Join-Path $ctx.Logs "$Stage\strict-predictions.csv"
            $open = Join-Path $ctx.Logs "$Stage\open-predictions.csv"
            $a = Invoke-Python $Stage 'predict-strict' @('-m', 'textlab', 'predict', '--model-dir', $Models, '--input', $holdoutPath, '--output', $strict, '--min-confidence', '0.999')
            $b = Invoke-Python $Stage 'predict-open' @('-m', 'textlab', 'predict', '--model-dir', $Models, '--input', $holdoutPath, '--output', $open, '--min-confidence', '0')
            Assert-That ($a.ExitCode -eq 0 -and $b.ExitCode -eq 0) "exits $($a.ExitCode)/$($b.ExitCode)"
            $review = @(Import-Csv -LiteralPath $strict | Where-Object category -eq 'needs_review').Count
            $none = @(Import-Csv -LiteralPath $open | Where-Object category -eq 'needs_review').Count
            Assert-That ($review -ge 1 -and $none -eq 0) "needs_review strict=$review open=$none"; $true })
}

function Test-Report([string]$Stage, [string]$Models) {
    if (-not $Models) { return }
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'report.html' -Description 'self-contained HTML report with tables, every label and escaped text' -Test {
            $out = Join-Path $ctx.Logs "$Stage\report.html"
            $run = Invoke-Python $Stage 'report' @('-m', 'textlab', 'report', '--model-dir', $Models, '--data', $holdoutPath, '--output', $out)
            Assert-That ($run.ExitCode -eq 0 -and (Test-Path -LiteralPath $out)) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $html = Get-Content -LiteralPath $out -Raw
            Assert-That ($html -match '(?i)<html' -and $html -match '(?i)</html>') 'not an HTML document'
            Assert-That ([regex]::Matches($html, '(?i)<table').Count -ge 2) 'fewer than 2 tables'
            $missing = @($labels | Where-Object { $html -notmatch "\b$_\b" })
            Assert-That ($missing.Count -eq 0) "labels missing: $($missing -join ', ')"
            Assert-That (-not $html.Contains('<script>alert(1)</script>')) 'unescaped ticket text (XSS) in report'
            Assert-That ($html -notmatch '(?i)<script|<link[^>]+href="https?:|src="https?:') 'report is not self-contained'; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'report.escaping' -Description 'a guaranteed misclassification renders ticket text as escaped HTML' -Test {
            $payload = '<script>alert(1)</script> my invoice amount is wrong'
            $prediction = Invoke-Python $Stage 'predict-xss' @('-m', 'textlab', 'predict', '--model-dir', $Models, '--text', $payload)
            Assert-That ($prediction.ExitCode -eq 0) "XSS probe prediction exit $($prediction.ExitCode)"
            $predicted = ($prediction.Output | ConvertFrom-Json).category
            $different = $labels | Where-Object { $_ -ne $predicted } | Select-Object -First 1
            $inputPath = Join-Path (New-GateDir $Stage 'report-xss') 'input.csv'
            [pscustomobject]@{ id = 'xss'; text = $payload; category = $different; sentiment = 'neutral' } |
                Export-Csv -LiteralPath $inputPath -NoTypeInformation -Encoding utf8NoBOM
            $out = Join-Path $ctx.Logs "$Stage\report-xss.html"
            $run = Invoke-Python $Stage 'report-xss' @('-m', 'textlab', 'report', '--model-dir', $Models, '--data', $inputPath, '--output', $out)
            Assert-That ($run.ExitCode -eq 0) "XSS report exit $($run.ExitCode): $($run.Errors)"
            $html = Get-Content -LiteralPath $out -Raw
            Assert-That ($html.Contains('&lt;script&gt;alert(1)&lt;/script&gt;') -and $html -notmatch '(?i)<script\b') 'misclassified ticket text is missing or not HTML-escaped'
            $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'model-card' -Description 'MODEL_CARD.md has the required sections' -Test {
            $path = Join-Path $ws 'MODEL_CARD.md'
            Assert-That (Test-Path -LiteralPath $path) 'MODEL_CARD.md missing'
            $card = Get-Content -LiteralPath $path -Raw
            $missing = @('## Intended use', '## Training data', '## Metrics', '## Limitations', '## Ethical considerations' | Where-Object { -not $card.Contains($_) })
            Assert-That ($missing.Count -eq 0) "missing: $($missing -join ', ')"; $true })
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

$regressionNames = @('test_fullwidth_and_case_are_normalized', 'test_urls_are_masked', 'test_emails_are_masked', 'test_order_numbers_are_masked', 'test_whitespace_only_text_normalizes_to_empty')

$exitCode = 1
try {
    Invoke-CommonPreflight $ctx

    # --- B0 ---------------------------------------------------------------
    $stage = 'B0-baseline'
    Write-Step $ctx "B0 seed package, data and venv (Python $pyVersion)" 'phase'
    Write-SeedFiles -Root $ws -Files $seed
    New-Item -ItemType Directory -Force -Path (Join-Path $ws 'data') | Out-Null
    $trainRows | Export-Csv -LiteralPath (Join-Path $ws 'data\tickets_train.csv') -NoTypeInformation -UseQuotes AsNeeded -Encoding utf8NoBOM
    $devRows | Export-Csv -LiteralPath (Join-Path $ws 'data\tickets_dev.csv') -NoTypeInformation -UseQuotes AsNeeded -Encoding utf8NoBOM
    $holdoutRows | Export-Csv -LiteralPath $holdoutPath -NoTypeInformation -UseQuotes AsNeeded -Encoding utf8NoBOM
    $edge = "id,text`r`ne1,`r`ne2,`"   `"`r`ne3,🙂🙂🙂`r`ne4,Mi paquete no ha llegado y el seguimiento no muestra nada`r`ne5,$(('please refund my invoice ' * 800).Trim())`r`ne6,`"Line one`r`nline two about my invoice`"`r`n"
    [System.IO.File]::WriteAllText($edgePath, $edge, [System.Text.UTF8Encoding]::new($true))
    Write-Utf8File $missingColumnPath "id,body`nx1,hello`n"
    $create = Invoke-Tool -Ctx $ctx -Stage $stage -Label 'venv' -FilePath $basePython -ArgumentList @('-m', 'venv', $venv)
    if ($create.ExitCode -ne 0) { throw "venv creation failed: $($create.Errors)" }
    $install = Invoke-Python $stage 'pip-install' @('-m', 'pip', 'install', '-e', "$($ws)[dev]") 1800
    if ($install.ExitCode -ne 0) { throw "pip install failed:`n$(Get-Tail ($install.Output + $install.Errors))" }
    $freeze = Invoke-Python $stage 'pip-freeze' @('-m', 'pip', 'freeze', '--exclude-editable')
    if ($freeze.ExitCode -ne 0) { throw "pip freeze failed: $($freeze.Errors)" }
    Write-Utf8File (Join-Path $ctx.Results 'environment-freeze.txt') $freeze.Output
    Test-Pytest $stage 1
    if ((Get-FailedGates $ctx $stage).Count) { throw 'Baseline Python environment does not pass its smoke test; fix it before spending on VCP turns.' }
    Initialize-GitCheckpoint $ctx
    $protected = @{
        'data/tickets_train.csv' = (Get-Sha256 (Join-Path $ws 'data\tickets_train.csv'))
        'data/tickets_dev.csv'   = (Get-Sha256 (Join-Path $ws 'data\tickets_dev.csv'))
    }

    # --- Profiles ---------------------------------------------------------
    $stage = 'P1-profiles'
    $pythonProcess = New-ProcessProfile -Name 'python' -Executable $python -Ctx $ctx -ExtraPath @((Split-Path -Parent $basePython)) -MaxTimeoutMs 1800000
    $affected = @('README.md', 'MODEL_CARD.md', 'pyproject.toml', 'src', 'tests', 'data', 'models', 'reports')
    $profileMain = New-ScenarioProfile -Ctx $ctx -Name 'profile-main' -AffectedPaths $affected -Processes @($pythonProcess)
    $profileShort = New-ScenarioProfile -Ctx $ctx -Name 'profile-short' -AffectedPaths $affected -Processes @($pythonProcess) -DeadlineSeconds $ctx.ShortDeadlineSeconds
    $profileReview = New-ScenarioProfile -Ctx $ctx -Name 'profile-review' -AffectedPaths $affected -MaximumAutonomy 'plan' -AutomaticEffects @('read')
    $profileBounds = New-ScenarioProfile -Ctx $ctx -Name 'profile-bad-bounds' -AffectedPaths $affected -Processes @($pythonProcess)
    Write-Utf8File $profileBounds ([regex]::Replace([System.IO.File]::ReadAllText($profileBounds), '"max_requests":\s*\d+', '"max_requests": 0'))
    foreach ($pair in @(@('main', $profileMain), @('short', $profileShort), @('review', $profileReview))) { [void](Test-ProfileCheck $ctx $stage $pair[1] $pair[0]) }

    # --- G0: zero-spend guardrail: resource bounds -------------------------
    $guardPrompt = Join-Path $ctx.Logs 'G0-guardrail\prompt.md'
    Write-Utf8File $guardPrompt 'Guardrail probe. This task must be rejected before execution.'
    Invoke-GuardrailRun -Ctx $ctx -Stage 'G0-guardrail' -Id 'profile-bounds' -Config $profileBounds `
        -Description 'run with max_requests 0 in the profile is rejected, exit 2, no task' `
        -Arguments @('run', '--file', $guardPrompt, '--budget-usd', '0.01', '--autonomy', 'autonomous') -ExpectStderr 'bounds'

    if ($ctx.SkipPaidStages) {
        Write-Step $ctx 'Dry run (-SkipPaidStages): toolchain, seed, baseline, profiles and guardrail verified; stopping before paid stages.' 'ok'
        throw 'VCP_SCENARIO_DRY_RUN_COMPLETE'
    }

    # --- T1 ----------------------------------------------------------------
    $gatesT1 = { param($s) Test-Pytest $s 7; [void](Test-Model $s $BaselineMacroF1); Test-ProtectedUnchanged $s $protected }
    $t1 = Invoke-VcpTask -Ctx $ctx -Stage 'T1-baseline' -Title 'Preprocessing, loading, category classifier' -Prompt $promptT1 -Config $profileMain
    if ($t1) { Test-StageExit $ctx $t1 'T1-baseline'; & $gatesT1 'T1-baseline'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T1-baseline' -Config $profileMain -GateScript $gatesT1) }
    Save-Checkpoint $ctx 'T1: baseline classifier'

    # --- T2 ----------------------------------------------------------------
    $gatesT2 = { param($s) Test-Pytest $s 10; [void](Test-Model $s $BaselineMacroF1 -Sentiment); Test-Keywords $s; Test-ProtectedUnchanged $s $protected }
    $t2 = Invoke-VcpTask -Ctx $ctx -Stage 'T2-sentiment' -Title 'Sentiment model and keywords' -Prompt $promptT2 -Config $profileMain
    if ($t2) { Test-StageExit $ctx $t2 'T2-sentiment'; & $gatesT2 'T2-sentiment'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T2-sentiment' -Config $profileMain -GateScript $gatesT2) }
    Save-Checkpoint $ctx 'T2: sentiment and keywords'

    # --- T3 ----------------------------------------------------------------
    $gatesT3 = { param($s) Test-Pytest $s 14; $m = Test-Model $s $TargetMacroF1 -Sentiment; Test-Robustness $s $m; Test-Keywords $s; Test-ProtectedUnchanged $s $protected }
    $t3 = Invoke-VcpTask -Ctx $ctx -Stage 'T3-robustness' -Title 'Robustness and calibrated confidences' -Prompt $promptT3 -Config $profileMain
    if ($t3) { Test-StageExit $ctx $t3 'T3-robustness'; & $gatesT3 'T3-robustness'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T3-robustness' -Config $profileMain -GateScript $gatesT3) }
    Save-Checkpoint $ctx 'T3: robustness'

    # --- T4: protected regression tests --------------------------------------
    Write-Utf8File (Join-Path $ws 'tests\test_regressions.py') $regressionTests
    Save-Checkpoint $ctx 'T4 setup: protected regression tests added by harness'
    $protected['tests/test_regressions.py'] = Get-Sha256 (Join-Path $ws 'tests\test_regressions.py')
    $gatesT4 = { param($s) Test-Pytest $s 19 $regressionNames; $m = Test-Model $s $TargetMacroF1 -Sentiment; Test-Robustness $s $m; Test-ProtectedUnchanged $s $protected }
    $t4 = Invoke-VcpTask -Ctx $ctx -Stage 'T4-regressions' -Title 'Make protected preprocessing tests pass' -Prompt $promptT4 -Config $profileMain
    if ($t4) { Test-StageExit $ctx $t4 'T4-regressions'; & $gatesT4 'T4-regressions'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T4-regressions' -Config $profileMain -GateScript $gatesT4) }
    Save-Checkpoint $ctx 'T4: preprocessing regression fixes'

    # --- T5: short deadline, discover, stale-revision guardrail, resume -----
    $gatesT5 = { param($s) Test-Pytest $s 20 $regressionNames; $m = Test-Model $s $TargetMacroF1 -Sentiment; Test-Report $s $m; Test-ProtectedUnchanged $s $protected }
    $t5 = Invoke-VcpTask -Ctx $ctx -Stage 'T5-report' -Title 'HTML report and model card (short deadline)' -Prompt $promptT5 -Config $profileShort -AcceptExit @(0, 3, 8)
    if ($t5) {
        Test-StageExit $ctx $t5 'T5-report'
        if ($t5.exit_code -eq 8 -and $t5.task) {
            $discover = Invoke-WorkspaceDiscover $ctx 'T5-resume'
            $candidate = @($discover.Result.data.candidates | Where-Object { [string]$_.task -eq $t5.task }) | Select-Object -First 1
            [void](Invoke-Gate -Ctx $ctx -Stage 'T5-resume' -Id 'discover-lists-paused' -Description 'workspace discover lists the paused T5 task with an expected revision' -Test {
                    Assert-That ($null -ne $candidate -and $null -ne $candidate.expected_revision) "candidates: $($discover.Result.data | ConvertTo-Json -Depth 6 -Compress)"; $true })
            if ($candidate) {
                $revision = [uint64]([string]$candidate.expected_revision)
                $staleRevision = if ($revision -eq 0) { '1' } else { '0' }
                $stale = Invoke-Vcp -Ctx $ctx -Stage 'T5-resume' -Label 'resume-stale-revision' -Config $profileMain -Arguments @('resume', $t5.task, '--expected-revision', $staleRevision)
                [void](Invoke-Gate -Ctx $ctx -Stage 'T5-resume' -Id 'stale-revision-rejected' -Description 'resume with a stale --expected-revision is rejected without continuing' -Test {
                        $errorText = Get-Content -LiteralPath $stale.StderrPath -Raw
                        Assert-That ($stale.ExitCode -eq 2 -and $errorText -match '(?i)revision|selection') "exit $($stale.ExitCode): $errorText"
                        Assert-That (@($stale.Frames | Where-Object { $_.type -eq 'event' }).Count -eq 0) 'task events emitted for a stale selection'; $true })
                $resumed = Invoke-VcpContinuation -Ctx $ctx -Stage 'T5-resume' -Title "resume $($t5.task) --expected-revision $revision" `
                    -Arguments @('resume', $t5.task, '--expected-revision', [string]$revision) -Config $profileMain -AcceptExit @(0, 3)
                if ($resumed) {
                    Test-StageExit $ctx $resumed 'T5-resume'
                    [void](Invoke-Gate -Ctx $ctx -Stage 'T5-resume' -Id 'resume-same-task' -Description 'resume continued the paused T5 task' -Test {
                            Assert-That ($resumed.task -eq $t5.task) "resumed '$($resumed.task)' != paused '$($t5.task)'"; $true })
                }
            }
        }
        else { [void](Skip-Gate $ctx 'T5-resume' 'resume-same-task' 'resume continued the paused T5 task' "T5 ended with exit $($t5.exit_code); continuation not exercised") }
        & $gatesT5 'T5-report'
        [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T5-report' -Config $profileMain -GateScript $gatesT5)
    }
    Save-Checkpoint $ctx 'T5: report and model card'

    # --- T6: plan-mode review, then fork the review session -----------------
    $review = Invoke-PlanModeReview -Ctx $ctx -Stage 'T6-review' -Config $profileReview -Prompt $promptReview
    if ($review -and $review.session) {
        $turns = Get-CompletedTurnIds $review.Run.Frames
        if ($turns.Count) {
            $before = Get-WorkspaceManifest $ws -IncludeGenerated
            $fork = Invoke-VcpContinuation -Ctx $ctx -Stage 'T7-fork' -Title "sessions fork $($review.session) --through-turn $($turns[-1])" `
                -Arguments @('sessions', 'fork', $review.session, '--through-turn', $turns[-1]) -Config $profileReview -AcceptExit @(0, 3, 4)
            if ($fork) {
                Test-StageExit $ctx $fork 'T7-fork'
                [void](Invoke-Gate -Ctx $ctx -Stage 'T7-fork' -Id 'fork-new-session' -Description 'fork created a new session and root task' -Test {
                        Assert-That ($fork.session -and $fork.session -ne $review.session) "fork session '$($fork.session)' vs source '$($review.session)'"
                        Assert-That ($fork.task -and $fork.task -ne $review.task) 'fork reused the source task'; $true })
                [void](Invoke-Gate -Ctx $ctx -Stage 'T7-fork' -Id 'immutable' -Description 'forked review left the workspace byte-identical' -Test {
                        $diff = Compare-WorkspaceManifest $before (Get-WorkspaceManifest $ws -IncludeGenerated)
                        Assert-That ($diff.Changed -eq 0) ('changed: ' + (($diff.Added + $diff.Modified + $diff.Removed) -join ', ')); $true })
                [void](Invoke-Gate -Ctx $ctx -Stage 'T7-fork' -Id 'review-consistency' -Description 'forked review cites an overlapping set of files (Jaccard >= 0.3)' -Advisory -Test {
                        $first = Join-Path $ctx.Logs 'T6-review\final-message.md'; $second = Join-Path $ctx.Logs 'T7-fork\final-message.md'
                        Assert-That ((Test-Path -LiteralPath $first) -and (Test-Path -LiteralPath $second)) 'final messages unavailable'
                        $pattern = '(?:src|tests|data)/[A-Za-z0-9_./-]+\.(?:py|csv|toml)'
                        $a = @([regex]::Matches((Get-Content -LiteralPath $first -Raw), $pattern) | ForEach-Object Value | Sort-Object -Unique)
                        $b = @([regex]::Matches((Get-Content -LiteralPath $second -Raw), $pattern) | ForEach-Object Value | Sort-Object -Unique)
                        $union = @($a + $b | Sort-Object -Unique).Count
                        $common = @($a | Where-Object { $b -contains $_ }).Count
                        $score = if ($union) { $common / $union } else { 0 }
                        $ctx.Notes.Add(('Review consistency (Jaccard of cited files) = {0:N2} ({1} common of {2}).' -f $score, $common, $union))
                        Assert-That ($score -ge 0.3) ('Jaccard {0:N2}' -f $score); $true })
            }
        }
        else { [void](Skip-Gate $ctx 'T7-fork' 'fork-new-session' 'fork created a new session and root task' 'no completed turn id found in the review stream') }
    }

    # --- FINAL --------------------------------------------------------------
    $stage = 'FINAL'
    Write-Step $ctx 'FINAL retrain in workspace, holdout metrics, wheel, bytecode' 'phase'
    Test-Pytest $stage 20 $regressionNames
    $finalModels = Test-Model $stage $TargetMacroF1 -Sentiment
    Test-Robustness $stage $finalModels
    Test-Keywords $stage
    Test-Report $stage $finalModels
    Test-ProtectedUnchanged $stage $protected
    $artifacts = Join-Path $ws 'artifacts'
    New-Item -ItemType Directory -Force -Path $artifacts | Out-Null
    [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'final-artifacts' -Description 'preserve the verified model bytes and generate all final outputs successfully' -Test {
        Assert-That (-not [string]::IsNullOrWhiteSpace($finalModels)) 'final model training did not succeed'
        New-Item -ItemType Directory -Force -Path (Join-Path $ws 'models') | Out-Null
        foreach ($name in 'category.joblib', 'sentiment.joblib', 'metadata.json') {
            Copy-Item -LiteralPath (Join-Path $finalModels $name) -Destination (Join-Path $ws "models\$name") -Force
        }
        foreach ($name in 'category.joblib', 'sentiment.joblib', 'metadata.json') { Add-Asset $ctx (Join-Path $ws "models\$name") 'Trained model artifact (seed 13)' }
        Copy-Item -LiteralPath (Join-Path $ctx.Logs 'FINAL\holdout-metrics.json') -Destination (Join-Path $artifacts 'holdout-metrics.json') -Force
        Add-Asset $ctx (Join-Path $artifacts 'holdout-metrics.json') 'Hidden holdout metrics of the final models'
        New-Item -ItemType Directory -Force -Path (Join-Path $ws 'reports') | Out-Null
        $reportRun = Invoke-Python $stage 'report-final' @('-m', 'textlab', 'report', '--model-dir', 'models', '--data', 'data/tickets_dev.csv', '--output', 'reports/report.html')
        Assert-That ($reportRun.ExitCode -eq 0 -and (Test-Path -LiteralPath (Join-Path $ws 'reports\report.html'))) "final report exit $($reportRun.ExitCode): $($reportRun.Errors)"
        Add-Asset $ctx (Join-Path $ws 'reports\report.html') 'Evaluation report on the dev set'
        $keywordsRun = Invoke-Python $stage 'keywords-final' @('-m', 'textlab', 'keywords', '--data', 'data/tickets_train.csv', '--top', '10', '--output', (Join-Path $artifacts 'keywords.json'))
        Assert-That ($keywordsRun.ExitCode -eq 0 -and (Test-Path -LiteralPath (Join-Path $artifacts 'keywords.json'))) "final keywords exit $($keywordsRun.ExitCode): $($keywordsRun.Errors)"
        Add-Asset $ctx (Join-Path $artifacts 'keywords.json') 'Top keywords per category'
        $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'wheel' -Description 'pip wheel builds a fresh textlab wheel' -Test {
            $wheelDir = New-GateDir $stage 'wheel'
            $run = Invoke-Python $stage 'wheel' @('-m', 'pip', 'wheel', '.', '--no-deps', '-w', $wheelDir) 900
            $wheel = Get-ChildItem -LiteralPath $wheelDir -Filter 'textlab-*.whl' -ErrorAction SilentlyContinue | Select-Object -First 1
            Assert-That ($run.ExitCode -eq 0 -and $wheel) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $dist = Join-Path $ws 'dist'
            New-Item -ItemType Directory -Force -Path $dist | Out-Null
            Copy-Item -LiteralPath $wheel.FullName -Destination $dist -Force
            Add-Asset $ctx (Join-Path $dist $wheel.Name) 'Python wheel'; $true })
    [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'bytecode' -Description 'python -m compileall compiles the package' -Test {
            $run = Invoke-Python $stage 'compileall' @('-m', 'compileall', '-q', 'src')
            Assert-That ($run.ExitCode -eq 0) "exit $($run.ExitCode): $($run.Output)"
            Add-Asset $ctx (Join-Path $ws 'src\textlab\__pycache__') 'Compiled bytecode (.pyc)'; $true })
    Add-Asset $ctx (Join-Path $ws 'MODEL_CARD.md') 'Model card'
    Save-Checkpoint $ctx 'FINAL: verified state'

    Invoke-FinalEvidenceSweep $ctx 'holdout'
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
