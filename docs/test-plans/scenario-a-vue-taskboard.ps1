#Requires -Version 7.4
# SPDX-License-Identifier: Apache-2.0
<#
.SYNOPSIS
Scenario A - TaskBoard: TypeScript + Node (Express 5) + Vue 3 (Vite), built over
multiple VCP CLI turns and assessed with deterministic HTTP, build and test gates.

.DESCRIPTION
See docs/test-plans/cli-test-plans.md, "Scenario A". The script seeds a Vite/Vue
scaffold, runs a zero-spend preflight and guardrail, then six VCP turns (API,
UI, cross-stack feature, protected regression tests, production build with an
interrupted-and-resumed task, plan-mode review) with repair turns when gates
fail. It finishes with an independent clean build, regression of every API
contract, a zipped client bundle, and results/scorecard.json + summary.md.

Run from a console where OPENROUTER_API_KEY is already set. Nothing here prints it.

.EXAMPLE
pwsh -File .\scenario-a-vue-taskboard.ps1 -ProviderGeneration C:\vcp-private\provider-20261002
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
    [ValidateRange(1, 65535)][int]$ApiPort = 41731,
    [switch]$SkipPaidStages
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VcpScenarioHarness.psm1') -Force

$ctx = Initialize-VcpScenario -Name 'a-vue-taskboard' -RunRoot $RunRoot -ProjectPath $ProjectPath -Vcp $Vcp -ProviderGeneration $ProviderGeneration `
    -TurnBudgetUsd $TurnBudgetUsd -MaxScenarioUsd $MaxScenarioUsd -MaxRepairTurns $MaxRepairTurns -OutputTokens $OutputTokens `
    -MaxRequests $MaxRequests -DeadlineSeconds $DeadlineSeconds -ShortDeadlineSeconds $ShortDeadlineSeconds -SkipPaidStages:$SkipPaidStages
$ws = $ctx.Workspace
$base = "http://127.0.0.1:$ApiPort"

#region Toolchain

$node = Find-Executable -Name 'node' -Candidates @("$env:ProgramFiles\nodejs\node.exe")
if (-not $node) { throw 'Node.js (node.exe) 22.18+ is required.' }
$nodeVersion = [version]((& $node --version).Trim().TrimStart('v'))
if ($nodeVersion -lt [version]'22.18.0') { throw "Node.js $nodeVersion found; 22.18+ is required for TypeScript type stripping." }
$npmCli = Join-Path (Split-Path -Parent $node) 'node_modules\npm\bin\npm-cli.js'
if (-not (Test-Path -LiteralPath $npmCli)) { throw "npm-cli.js not found next to $node." }

function Invoke-Npm([string]$Stage, [string]$Label, [string[]]$Arguments, [int]$TimeoutSeconds = 900, [hashtable]$Environment = @{}) {
    return Invoke-Tool -Ctx $ctx -Stage $Stage -Label $Label -FilePath $node -ArgumentList (@($npmCli) + $Arguments) -TimeoutSeconds $TimeoutSeconds -Environment $Environment
}
function Get-Tail([string]$Text, [int]$Count = 40) { return (($Text -split "`r?`n") | Select-Object -Last $Count) -join "`n" }

#endregion

#region Seed

$seed = [ordered]@{}
$seed['README.md'] = @'
# TaskBoard

A small team task board: a REST API (Node.js + Express 5, TypeScript run by
Node's built-in type stripping) and a single-page UI (Vue 3 + Vite + TypeScript).

## Layout

- `server/` - API source. `server/app.ts` exports `createApp(options?)`; `server/index.ts` starts it.
- `src/` - Vue single-page application.
- `tests/` - API tests run by `node --test` (TypeScript, erasable syntax only).
- `src/**/*.spec.ts` - UI component tests run by Vitest.

## Scripts

| Script | Purpose |
|---|---|
| `npm run dev:api` | API with watch on port 41731 |
| `npm run dev` | Vite dev server; proxies `/api` to the API |
| `npm run typecheck` | `vue-tsc` for the app and `tsc` for server and tests |
| `npm test` | API tests: `node --test` with explicit test files |
| `npm run test:unit` | Vitest component tests |
| `npm run build` | Typecheck, then production client bundle in `dist/client` |
| `npm start` | Run the API (and later the built client) |

Server TypeScript must use erasable syntax only (no enums, namespaces or
parameter properties) and import local modules with explicit `.ts` extensions.
'@
$seed['.gitignore'] = @'
node_modules/
dist/
data/
artifacts/
coverage/
*.log
'@
$seed['package.json'] = @'
{
  "name": "taskboard",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "engines": { "node": ">=22.18" },
  "scripts": {
    "dev": "vite",
    "dev:api": "node --watch server/index.ts",
    "typecheck": "vue-tsc --noEmit -p tsconfig.app.json && tsc --noEmit -p tsconfig.server.json",
    "build": "npm run typecheck && vite build",
    "test": "node --test tests/health.test.ts",
    "test:unit": "vitest run",
    "start": "node server/index.ts"
  },
  "dependencies": {
    "express": "^5.1.0",
    "vue": "^3.5.13"
  },
  "devDependencies": {
    "@types/express": "^5.0.3",
    "@types/node": "^22.15.0",
    "@vitejs/plugin-vue": "^6.0.0",
    "@vue/test-utils": "^2.4.6",
    "@vue/tsconfig": "^0.7.0",
    "jsdom": "^26.1.0",
    "typescript": "^5.8.3",
    "vite": "^7.0.0",
    "vitest": "^3.2.4",
    "vue-tsc": "^3.0.0"
  }
}
'@
$seed['tsconfig.app.json'] = @'
{
  "extends": "@vue/tsconfig/tsconfig.dom.json",
  "compilerOptions": {
    "noEmit": true,
    "types": ["vite/client"]
  },
  "include": ["src/**/*.ts", "src/**/*.vue"]
}
'@
$seed['tsconfig.server.json'] = @'
{
  "compilerOptions": {
    "target": "ES2023",
    "lib": ["ES2023"],
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "allowImportingTsExtensions": true,
    "noEmit": true,
    "strict": true,
    "erasableSyntaxOnly": true,
    "verbatimModuleSyntax": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "types": ["node"]
  },
  "include": ["server/**/*.ts", "tests/**/*.ts"]
}
'@
$seed['vite.config.ts'] = @'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  server: { proxy: { '/api': 'http://127.0.0.1:41731' } },
  build: { outDir: 'dist/client', emptyOutDir: true },
})
'@
$seed['vitest.config.ts'] = @'
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  test: { environment: 'jsdom', include: ['src/**/*.spec.ts'] },
})
'@
$seed['vite.config.ts'] = $seed['vite.config.ts'].Replace('127.0.0.1:41731', "127.0.0.1:$ApiPort")
$seed['index.html'] = @'
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>TaskBoard</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
'@
$seed['src/main.ts'] = @'
import { createApp } from 'vue'
import App from './App.vue'

createApp(App).mount('#app')
'@
$seed['src/App.vue'] = @'
<script setup lang="ts">
const title = 'TaskBoard'
</script>

<template>
  <main>
    <h1>{{ title }}</h1>
    <p>Project scaffold. The board UI is not implemented yet.</p>
  </main>
</template>
'@
$seed['src/App.spec.ts'] = @'
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import App from './App.vue'

describe('App', () => {
  it('renders the product name', () => {
    expect(mount(App).text()).toContain('TaskBoard')
  })
})
'@
$seed['server/app.ts'] = @'
import express from 'express'
import type { Express } from 'express'

export interface AppOptions {
  dataFile?: string
}

export function createApp(_options: AppOptions = {}): Express {
  const app = express()
  app.use(express.json())
  app.get('/api/health', (_req, res) => {
    res.json({ status: 'ok' })
  })
  return app
}
'@
$seed['server/index.ts'] = @'
import { createApp } from './app.ts'

const port = Number(process.env.PORT ?? 41731)
createApp().listen(port, '127.0.0.1', () => {
  console.log(`taskboard listening on http://127.0.0.1:${port}`)
})
'@
$seed['tests/health.test.ts'] = @'
import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { AddressInfo } from 'node:net'
import { createApp } from '../server/app.ts'

test('GET /api/health returns ok', async () => {
  const server = createApp().listen(0, '127.0.0.1')
  await new Promise<void>((resolve) => server.once('listening', () => resolve()))
  try {
    const { port } = server.address() as AddressInfo
    const response = await fetch(`http://127.0.0.1:${port}/api/health`)
    assert.equal(response.status, 200)
    assert.deepEqual(await response.json(), { status: 'ok' })
  } finally {
    server.close()
  }
})
'@

$regressionTest = @'
// PROTECTED FILE - added by the scenario harness as an acceptance test. Do not edit.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { AddressInfo } from 'node:net'
import type { Server } from 'node:http'
import { createApp } from '../server/app.ts'

async function start(): Promise<{ server: Server; url: string }> {
  const dataFile = join(mkdtempSync(join(tmpdir(), 'taskboard-regressions-')), 'tasks.json')
  const server = createApp({ dataFile }).listen(0, '127.0.0.1')
  await new Promise<void>((resolve) => server.once('listening', () => resolve()))
  const { port } = server.address() as AddressInfo
  return { server, url: `http://127.0.0.1:${port}` }
}

async function send(url: string, method: string, body?: unknown): Promise<{ status: number; json: any }> {
  const response = await fetch(url, {
    method,
    headers: { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const text = await response.text()
  return { status: response.status, json: text ? JSON.parse(text) : null }
}

test('POST /api/tasks trims title whitespace', async () => {
  const { server, url } = await start()
  try {
    const created = await send(`${url}/api/tasks`, 'POST', { title: '   Plan sprint   ' })
    assert.equal(created.status, 201)
    assert.equal(created.json.title, 'Plan sprint')
  } finally { server.close() }
})

test('POST /api/tasks rejects whitespace-only title', async () => {
  const { server, url } = await start()
  try {
    const created = await send(`${url}/api/tasks`, 'POST', { title: ' \t ' })
    assert.equal(created.status, 400)
    assert.equal(created.json.error.code, 'VALIDATION_ERROR')
    assert.equal(created.json.error.field, 'title')
  } finally { server.close() }
})

test('PATCH /api/tasks/:id rejects unknown fields', async () => {
  const { server, url } = await start()
  try {
    const created = await send(`${url}/api/tasks`, 'POST', { title: 'Patch target' })
    const patched = await send(`${url}/api/tasks/${created.json.id}`, 'PATCH', { colour: 'red' })
    assert.equal(patched.status, 400)
    assert.equal(patched.json.error.code, 'UNKNOWN_FIELD')
    assert.equal(patched.json.error.field, 'colour')
  } finally { server.close() }
})

test('PATCH /api/tasks/:id cannot change read-only fields', async () => {
  const { server, url } = await start()
  try {
    const created = await send(`${url}/api/tasks`, 'POST', { title: 'Read only target' })
    const patched = await send(`${url}/api/tasks/${created.json.id}`, 'PATCH', { createdAt: '2001-01-01T00:00:00.000Z' })
    assert.equal(patched.status, 400)
    assert.equal(patched.json.error.code, 'READ_ONLY_FIELD')
    assert.equal(patched.json.error.field, 'createdAt')
    const fetched = await send(`${url}/api/tasks/${created.json.id}`, 'GET')
    assert.equal(fetched.json.createdAt, created.json.createdAt)
  } finally { server.close() }
})
'@

#endregion

#region Prompts

$environmentBlock = @'

## Environment and rules (applies to every task in this project)

- Work only inside the current workspace. Read README.md and the existing code first.
- Process profiles available to `vcp_exec` (no shell; pass literal arguments):
  - `node` runs Node.js {{NODE_VERSION}}. Run npm through it: profile `node`, arguments
    `["{{NPM_CLI}}", "run", "typecheck"]`, `["{{NPM_CLI}}", "test"]`, `["{{NPM_CLI}}", "run", "test:unit"]`,
    `["{{NPM_CLI}}", "run", "build"]`. Run single tools directly, for example
    `["--test", "tests/tasks.api.test.ts"]`.
- Dependencies are already installed. Add a dependency only when essential, with
  `["{{NPM_CLI}}", "install", "--save-exact", "<package>@<version>"]`, and explain why.
- Server and test TypeScript must remain runnable by Node type stripping: erasable syntax
  only and explicit `.ts` extensions on relative imports.
- The `npm test` script must stay in the form `node --test <explicit test files>`; add every
  new API test file to it.
- Protected files (never edit, rename or delete): `tests/health.test.ts`{{PROTECTED}}.
- Before finishing, run typecheck, `npm test`, `npm run test:unit` and `npm run build` and
  fix any failure. Finish with a short summary of changed files and command results.
'@
$environmentBlock = $environmentBlock.Replace('{{NODE_VERSION}}', [string]$nodeVersion).Replace('{{NPM_CLI}}', $npmCli.Replace('\', '\\'))

function New-Prompt([string]$Body, [string]$Protected = '') {
    if ($ctx.ReuseProject -and (Test-Path -LiteralPath (Join-Path $ws 'tests/regressions.test.ts'))) {
        $Protected = '`, `tests/regressions.test.ts`'
    }
    return $Body + $environmentBlock.Replace('{{PROTECTED}}', $Protected)
}

$promptT1 = New-Prompt @'
# Task T1 - Task REST API with file persistence

Implement the TaskBoard REST API in `server/` with this exact contract.

Task object:
`{ "id": string, "title": string, "description": string, "status": "todo"|"doing"|"done",
   "priority": "low"|"medium"|"high", "dueDate": "YYYY-MM-DD"|null, "createdAt": ISO-8601 UTC,
   "updatedAt": ISO-8601 UTC }`
Defaults: description "", status "todo", priority "medium", dueDate null.

Endpoints (JSON in and out):
- `GET /api/health` -> 200 `{"status":"ok"}` (keep as is).
- `GET /api/tasks?status=&q=` -> 200 `{"items":[...],"total":n}`, sorted by createdAt ascending.
  `status` filters exactly; `q` is a case-insensitive substring match on title and description.
- `POST /api/tasks` -> 201 with the created task.
- `GET /api/tasks/:id` -> 200 task, or 404.
- `PATCH /api/tasks/:id` with any subset of title, description, status, priority, dueDate
  -> 200 updated task with a refreshed updatedAt.
- `DELETE /api/tasks/:id` -> 204 with no body, or 404.

Validation: title is required, 1-120 characters; status and priority must be one of the listed
values; dueDate must be a real calendar date in YYYY-MM-DD form or null.
Errors: 400 `{"error":{"code":"VALIDATION_ERROR","message":"...","field":"<field>"}}`;
404 `{"error":{"code":"NOT_FOUND","message":"..."}}`; malformed JSON body -> 400 with code
`INVALID_JSON`.

Persistence: tasks are stored in a JSON file. `createApp(options?: { dataFile?: string })` keeps its
signature; the file is `options.dataFile`, else environment variable `TASKBOARD_DATA`, else
`data/tasks.json`. Create missing directories, write atomically (temporary file then rename) and
reload existing data on start so tasks survive a restart. `server/index.ts` listens on `PORT`
(default 41731) at 127.0.0.1.

Tests: add `tests/tasks.api.test.ts` using `node:test`, starting the app on port 0 with a temporary
data file. Use exactly these test names (more tests are welcome):
- `POST /api/tasks creates a task`
- `POST /api/tasks rejects an empty title`
- `PATCH /api/tasks/:id updates status`
- `DELETE /api/tasks/:id removes the task`
- `GET /api/tasks filters by status and query`
'@

$promptT2 = New-Prompt @'
# Task T2 - Vue board UI

Build the TaskBoard single-page UI in `src/` against the existing API (do not change the API
contract).

- `src/api/tasks.ts`: a typed client for the API (list with filters, create, update, delete) using
  `fetch` and relative `/api/...` URLs; surface API validation errors to the caller.
- `src/composables/useTasks.ts`: reactive state (tasks, loading, error) and actions.
- Components: a board with three columns (todo, doing, done), task cards, a create form with
  client-side validation mirroring the API (title required, max 120), a search box filtering by
  text, and buttons on each card to move it to the previous/next column and to delete it.
- Keep the heading text `TaskBoard`.
- Use these exact `data-testid` attributes: `column-todo`, `column-doing`, `column-done`,
  `task-card`, `task-form`, `search-input`.
- Vitest component tests (`src/**/*.spec.ts`, at least three new tests) covering: rendering tasks
  into the right columns (mock the API client), form validation preventing an empty title, and
  moving a task calling update with the next status.
- Keep the Vite dev proxy for `/api` and keep `npm run build` emitting to `dist/client`.
'@

$promptT3 = New-Prompt @'
# Task T3 - Labels and sorting across API and UI

Extend the API and the UI together:

- Tasks gain `labels: string[]` (default `[]`) on create and PATCH. Each label must match
  `^[a-z0-9-]{1,20}$`, at most 5 labels, no duplicates; any violation -> 400 VALIDATION_ERROR with
  field `labels`. Existing stored tasks without labels must load as `[]`.
- `GET /api/tasks` accepts `label=<label>` (tasks having that label) and
  `sort=createdAt|priority|dueDate` (default createdAt). `priority` orders high, medium, low, then
  createdAt ascending. `dueDate` orders ascending with null due dates last, then createdAt.
  Any other sort value -> 400 VALIDATION_ERROR with field `sort`.
- UI: show labels as chips on cards (`data-testid="label-chip"`), allow entering labels in the
  create form, filter by clicking a chip, and a sort selector (`data-testid="sort-select"`).
- Tests: add API tests named exactly `POST /api/tasks validates labels` and
  `GET /api/tasks sorts by priority`, plus Vitest coverage for the sort selector.
- All earlier behavior and tests must keep passing.
'@

$promptT4 = New-Prompt @'
# Task T4 - Make the protected regression tests pass

A teammate added `tests/regressions.test.ts`. Ensure it is listed in the `npm test` script,
preserving every existing test entry. It describes required API behavior. Make every test in it pass by changing the
implementation only; the test file is protected. Keep all other tests and behavior passing, and
keep validation messages consistent with the existing error format.
'@ '`, `tests/regressions.test.ts`'

$promptT5 = New-Prompt @'
# Task T5 - Production build, single-process serving and stats

Make TaskBoard deployable as one Node process:

- When `dist/client/index.html` exists, `npm start` (`node server/index.ts`) also serves the built
  client: static files from `dist/client` and an SPA fallback returning `index.html` for any GET that
  is not under `/api` and has no file extension. Unknown `/api/...` routes return 404 JSON
  `{"error":{"code":"NOT_FOUND",...}}`, never HTML.
- Add `GET /api/stats` -> 200 `{"total":n,"byStatus":{"todo":a,"doing":b,"done":c},"overdue":k}` where
  overdue counts tasks whose dueDate is before today's UTC date and whose status is not done.
- Show the stats in the UI header (`data-testid="stats-bar"`).
- Add an API test named exactly `GET /api/stats counts tasks by status`.
- Update README.md with production instructions (`npm run build` then `npm start`, the `PORT` and
  `TASKBOARD_DATA` variables).
'@ '`, `tests/regressions.test.ts`'

$promptReview = @'
# Task T6 - Read-only engineering review

Do not modify, create or delete any file. Review this repository as a senior reviewer before a
first production release: API input validation, persistence durability and concurrency, error
handling, security (path handling, request size limits, XSS in the UI), test coverage gaps and
build configuration. Cite file paths and line numbers.

End your answer with one fenced ```json block of the form
{"findings":[{"severity":"high|medium|low","file":"path","line":n,"title":"...","recommendation":"..."}]}
listing at most 10 findings ordered by severity.
'@

#endregion

#region Gates

function Test-Typecheck([string]$Stage) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'typecheck' -Description 'npm run typecheck (vue-tsc + tsc) succeeds' -Test {
            $run = Invoke-Npm $Stage 'typecheck' @('run', 'typecheck')
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + "`n" + $run.Errors)))
            $true
        })
}

function Test-NodeTests([string]$Stage, [string[]]$Expected) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'node-test' -Description "node --test (npm test files) passes incl. $($Expected.Count) named tests" -Test {
            $package = Get-Content -LiteralPath (Join-Path $ws 'package.json') -Raw | ConvertFrom-Json
            $words = @(([string]$package.scripts.test) -split '\s+' | Where-Object { $_ })
            Assert-That ($words.Count -ge 3 -and $words[0] -eq 'node' -and $words[1] -eq '--test') "test script is '$($package.scripts.test)'"
            $files = $words[2..($words.Count - 1)]
            $run = Invoke-Tool -Ctx $ctx -Stage $Stage -Label 'node-test' -FilePath $node `
                -ArgumentList (@('--test', '--test-reporter=tap', '--test-concurrency=1') + $files)
            Assert-That ($run.ExitCode -eq 0) ("node --test exit {0}`n{1}" -f $run.ExitCode, (Get-Tail $run.Output 50))
            $missing = @($Expected | Where-Object { $run.Output -notmatch ('(?m)^\s*ok \d+ - ' + [regex]::Escape($_) + '\s*$') })
            Assert-That ($missing.Count -eq 0) ('named tests not passing: ' + ($missing -join '; '))
            $true
        })
}

function Test-UnitAndBuild([string]$Stage, [string[]]$TestIds, [int]$MinUnitTests = 1) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'vitest' -Description "Vitest passes with >= $MinUnitTests tests" -Test {
            $xml = Join-Path $ctx.Logs "$Stage\vitest-$([guid]::NewGuid().ToString('N')).xml"
            $run = Invoke-Tool -Ctx $ctx -Stage $Stage -Label 'vitest' -FilePath $node `
                -ArgumentList @('node_modules/vitest/vitest.mjs', 'run', '--reporter=junit', "--outputFile=$xml")
            Assert-That ($run.ExitCode -eq 0) ("vitest exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors)))
            $report = [xml](Get-Content -LiteralPath $xml -Raw)
            $suite = $report.testsuites
            $cases = @($report.SelectNodes('//testcase'))
            $skipped = @($report.SelectNodes('//testcase/skipped'))
            Assert-That ($cases.Count -ge $MinUnitTests) "only $($cases.Count) test cases"
            Assert-That ($skipped.Count -eq 0) "$($skipped.Count) skipped tests"
            Assert-That (([int]$suite.failures + [int]$suite.errors) -eq 0) "$($suite.failures) failures, $($suite.errors) errors"
            $true
        })
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'build' -Description 'npm run build emits dist/client with a JS bundle' -Test {
            $run = Invoke-Npm $Stage 'build' @('run', 'build')
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + "`n" + $run.Errors)))
            Assert-That (Test-Path -LiteralPath (Join-Path $ws 'dist\client\index.html')) 'dist/client/index.html missing'
            Assert-That (@(Get-ChildItem -LiteralPath (Join-Path $ws 'dist\client\assets') -Filter '*.js' -ErrorAction SilentlyContinue).Count -ge 1) 'no JS bundle'
            $true
        })
    if ($TestIds.Count) {
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'testids' -Description "bundle contains data-testid hooks: $($TestIds -join ', ')" -Test {
                $bundle = (Get-ChildItem -LiteralPath (Join-Path $ws 'dist\client\assets') -Filter '*.js' | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
                $missing = @($TestIds | Where-Object { -not $bundle.Contains($_) })
                Assert-That ($missing.Count -eq 0) ('missing: ' + ($missing -join ', '))
                $true
            })
    }
}

function Start-Api([string]$Stage, [string]$Label, [string]$DataFile) {
    return Start-BackgroundServer -Ctx $ctx -Stage $Stage -Label $Label -FilePath $node -ArgumentList @('server/index.ts') `
        -Environment @{ PORT = [string]$ApiPort; TASKBOARD_DATA = $DataFile; NODE_ENV = 'production' } -ReadyUrl "$base/api/health"
}

function New-DataFile([string]$Stage, [string]$Name) {
    $directory = Join-Path $ctx.Temp "$Stage-$Name-$([guid]::NewGuid().ToString('N').Substring(0, 6))"
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    return Join-Path $directory 'tasks.json'
}

function Test-ApiContract([string]$Stage) {
    # T1 contract, replayed after later turns as a regression suite.
    $dataFile = New-DataFile $Stage 'contract'
    $server = $null
    try {
        $server = Start-Api $Stage 'api-contract' $dataFile
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'api.start' -Description 'API starts with node server/index.ts' -Outcome 'pass' -Required $true)
    }
    catch {
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'api.start' -Description 'API starts with node server/index.ts' -Outcome 'fail' -Detail $_.Exception.Message -Required $true)
        return
    }
    $state = @{}
    try {
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.health' -Description 'GET /api/health -> 200 {status:ok}' -Test {
                $r = Invoke-Http GET "$base/api/health"
                Assert-That ($r.Status -eq 200 -and $r.Json.status -eq 'ok') "status $($r.Status) body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.create' -Description 'POST /api/tasks -> 201 with defaults' -Test {
                $r = Invoke-Http POST "$base/api/tasks" @{ title = 'Write release notes'; priority = 'high' }
                Assert-That ($r.Status -eq 201) "status $($r.Status) body $($r.Content)"
                Assert-That (-not [string]::IsNullOrWhiteSpace([string]$r.Json.id)) 'missing id'
                Assert-That ($r.Json.status -eq 'todo' -and $r.Json.priority -eq 'high' -and $r.Json.description -eq '' -and $null -eq $r.Json.dueDate) "defaults wrong: $($r.Content)"
                [void](ConvertTo-Instant $r.Json.createdAt)
                $state.task = $r.Json; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.validate-title' -Description 'empty title -> 400 VALIDATION_ERROR field title' -Test {
                $r = Invoke-Http POST "$base/api/tasks" @{ title = '' }
                Assert-That ($r.Status -eq 400 -and $r.Json.error.code -eq 'VALIDATION_ERROR' -and $r.Json.error.field -eq 'title') "status $($r.Status) body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.validate-status' -Description 'unknown status -> 400 field status' -Test {
                $r = Invoke-Http POST "$base/api/tasks" @{ title = 'x'; status = 'blocked' }
                Assert-That ($r.Status -eq 400 -and $r.Json.error.field -eq 'status') "status $($r.Status) body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.validate-date' -Description 'impossible dueDate 2026-02-30 -> 400 field dueDate' -Test {
                $r = Invoke-Http POST "$base/api/tasks" @{ title = 'x'; dueDate = '2026-02-30' }
                Assert-That ($r.Status -eq 400 -and $r.Json.error.field -eq 'dueDate') "status $($r.Status) body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.invalid-json' -Description 'malformed JSON -> 400 INVALID_JSON' -Test {
                $r = Invoke-Http POST "$base/api/tasks" '{"title": '
                Assert-That ($r.Status -eq 400 -and $r.Json.error.code -eq 'INVALID_JSON') "status $($r.Status) body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.not-found' -Description 'unknown id -> 404 NOT_FOUND' -Test {
                $r = Invoke-Http GET "$base/api/tasks/does-not-exist"
                Assert-That ($r.Status -eq 404 -and $r.Json.error.code -eq 'NOT_FOUND') "status $($r.Status) body $($r.Content)"; $true })
        if (-not $state.ContainsKey('task')) {
            # Keep these required checks visible without issuing requests with an empty ID.
            foreach ($id in 'api.list', 'api.patch', 'api.filter', 'api.restart', 'api.persistence', 'api.delete') {
                [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id $id -Description 'Created-task contract check' -Outcome 'skip' -Required $true -Detail 'Blocked by failed api.create; no valid task was captured.')
            }
            return
        }
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.list' -Description 'GET /api/tasks -> {items,total} with the created task only' -Test {
                $r = Invoke-Http GET "$base/api/tasks"
                Assert-That ($r.Status -eq 200 -and [int]$r.Json.total -eq 1 -and @($r.Json.items).Count -eq 1 -and $r.Json.items[0].id -eq $state.task.id) "body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.patch' -Description 'PATCH status doing -> 200 with refreshed updatedAt' -Test {
                Start-Sleep -Milliseconds 20
                $r = Invoke-Http PATCH "$base/api/tasks/$($state.task.id)" @{ status = 'doing' }
                Assert-That ($r.Status -eq 200 -and $r.Json.status -eq 'doing') "status $($r.Status) body $($r.Content)"
                Assert-That ((ConvertTo-Instant $r.Json.updatedAt) -gt (ConvertTo-Instant $state.task.updatedAt)) 'updatedAt not refreshed'
                $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.filter' -Description 'status and q filters' -Test {
                $doing = Invoke-Http GET "$base/api/tasks?status=doing"
                $done = Invoke-Http GET "$base/api/tasks?status=done"
                $query = Invoke-Http GET "$base/api/tasks?q=RELEASE"
                Assert-That ($doing.Status -eq 200 -and $done.Status -eq 200 -and $query.Status -eq 200 -and [int]$doing.Json.total -eq 1 -and [int]$done.Json.total -eq 0 -and [int]$query.Json.total -eq 1) "doing=$($doing.Json.total) done=$($done.Json.total) q=$($query.Json.total)"; $true })
        Stop-BackgroundServer $server
        $server = $null
        try {
            $server = Start-Api $Stage 'api-contract-restart' $dataFile
            [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'api.restart' -Description 'API restarts with persisted data' -Outcome 'pass' -Required $true)
        }
        catch {
            [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'api.restart' -Description 'API restarts with persisted data' -Outcome 'fail' -Detail $_.Exception.Message -Required $true)
            return
        }
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.persistence' -Description 'task survives a server restart (file persistence)' -Test {
                $r = Invoke-Http GET "$base/api/tasks/$($state.task.id)"
                Assert-That ($r.Status -eq 200 -and $r.Json.status -eq 'doing') "status $($r.Status) body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'api.delete' -Description 'DELETE -> 204 then GET -> 404' -Test {
                $d = Invoke-Http DELETE "$base/api/tasks/$($state.task.id)"
                $g = Invoke-Http GET "$base/api/tasks/$($state.task.id)"
                Assert-That ($d.Status -eq 204 -and $g.Status -eq 404) "delete $($d.Status), get $($g.Status)"; $true })
    }
    finally { Stop-BackgroundServer $server }
}

function Test-LabelsAndSort([string]$Stage) {
    $server = $null
    try {
        $server = Start-Api $Stage 'api-labels' (New-DataFile $Stage 'labels')
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'labels.start' -Description 'API starts' -Outcome 'pass' -Required $true)
    }
    catch {
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'labels.start' -Description 'API starts' -Outcome 'fail' -Detail $_.Exception.Message -Required $true)
        return
    }
    try {
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'labels.validation' -Description 'labels accepted when valid; uppercase, duplicate and >5 rejected' -Test {
                $ok = Invoke-Http POST "$base/api/tasks" @{ title = 'Labelled'; labels = @('ui', 'backend') }
                Assert-That ($ok.Status -eq 201 -and (@($ok.Json.labels) -join ',') -eq 'ui,backend') "valid: $($ok.Status) $($ok.Content)"
                foreach ($bad in @(@('UI'), @('a', 'a'), @('a', 'b', 'c', 'd', 'e', 'f'))) {
                    $r = Invoke-Http POST "$base/api/tasks" @{ title = 'Bad'; labels = $bad }
                    Assert-That ($r.Status -eq 400 -and $r.Json.error.field -eq 'labels') "labels [$($bad -join ',')]: $($r.Status) $($r.Content)"
                }
                $true })
        $ids = @{}
        foreach ($seedTask in @(
                @{ key = 'A'; title = 'Alpha'; priority = 'low'; dueDate = '2026-05-01'; labels = @('ops') },
                @{ key = 'B'; title = 'Bravo'; priority = 'high'; dueDate = $null; labels = @('ui') },
                @{ key = 'C'; title = 'Charlie'; priority = 'medium'; dueDate = '2026-04-01'; labels = @() },
                @{ key = 'D'; title = 'Delta'; priority = 'high'; dueDate = '2026-03-01'; labels = @('ui', 'ops') })) {
            Start-Sleep -Milliseconds 15
            $body = @{ title = $seedTask.title; priority = $seedTask.priority; dueDate = $seedTask.dueDate; labels = $seedTask.labels }
            $ids[[string](Invoke-Http POST "$base/api/tasks" $body).Json.id] = $seedTask.key
        }
        $order = { param($query) ((Invoke-Http GET "$base/api/tasks?$query").Json.items | ForEach-Object { $ids[[string]$_.id] } | Where-Object { $_ }) -join '' }
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'labels.sort-priority' -Description 'sort=priority -> B D C A' -Test {
                $value = & $order 'sort=priority'; Assert-That ($value -eq 'BDCA') "got $value"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'labels.sort-due' -Description 'sort=dueDate -> D C A B (nulls last)' -Test {
                $value = & $order 'sort=dueDate'; Assert-That ($value -eq 'DCAB') "got $value"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'labels.sort-default' -Description 'default order is createdAt -> A B C D' -Test {
                $value = & $order ''; Assert-That ($value -eq 'ABCD') "got $value"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'labels.filter' -Description 'label=ui -> B D' -Test {
                $value = & $order 'label=ui'; Assert-That ($value -eq 'BD') "got $value"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'labels.sort-invalid' -Description 'sort=bogus -> 400 field sort' -Test {
                $r = Invoke-Http GET "$base/api/tasks?sort=bogus"
                Assert-That ($r.Status -eq 400 -and $r.Json.error.field -eq 'sort') "status $($r.Status) body $($r.Content)"; $true })
    }
    finally { Stop-BackgroundServer $server }
}

function Test-Production([string]$Stage) {
    $server = $null
    try {
        $server = Start-Api $Stage 'production' (New-DataFile $Stage 'production')
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'prod.start' -Description 'production server starts' -Outcome 'pass' -Required $true)
    }
    catch {
        [void](Add-GateResult -Ctx $ctx -Stage $Stage -Id 'prod.start' -Description 'production server starts' -Outcome 'fail' -Detail $_.Exception.Message -Required $true)
        return
    }
    try {
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'prod.index' -Description 'GET / serves the built index.html' -Test {
                $r = Invoke-Http GET "$base/"
                Assert-That ($r.Status -eq 200 -and $r.Content -match 'id="app"') "status $($r.Status)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'prod.spa-fallback' -Description 'GET /board falls back to index.html' -Test {
                $r = Invoke-Http GET "$base/board"
                Assert-That ($r.Status -eq 200 -and $r.Content -match 'id="app"') "status $($r.Status)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'prod.asset' -Description 'hashed JS asset referenced by index.html is served as JavaScript' -Test {
                $index = (Invoke-Http GET "$base/").Content
                $match = [regex]::Match($index, 'src="(?<p>/assets/[^"]+\.js)"')
                Assert-That $match.Success 'no /assets/*.js reference in index.html'
                $r = Invoke-WebRequest -Uri ($base + $match.Groups['p'].Value) -SkipHttpErrorCheck -TimeoutSec 30
                Assert-That ($r.StatusCode -eq 200 -and ([string]$r.Headers['Content-Type']) -match 'javascript') "status $($r.StatusCode) type $($r.Headers['Content-Type'])"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'prod.api-404' -Description 'unknown /api route -> 404 JSON, not HTML' -Test {
                $r = Invoke-Http GET "$base/api/nope"
                Assert-That ($r.Status -eq 404 -and $r.Json.error.code -eq 'NOT_FOUND') "status $($r.Status) body $($r.Content)"; $true })
        [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'prod.stats' -Description 'GET /api/stats counts by status and overdue' -Test {
                foreach ($body in @(@{ title = 's1' }, @{ title = 's2'; status = 'doing' }, @{ title = 's3'; status = 'done'; dueDate = '2020-01-01' }, @{ title = 's4'; dueDate = '2020-01-01' })) {
                    [void](Invoke-Http POST "$base/api/tasks" $body)
                }
                $r = Invoke-Http GET "$base/api/stats"
                $s = $r.Json
                Assert-That ($r.Status -eq 200 -and [int]$s.total -eq 4 -and [int]$s.byStatus.todo -eq 2 -and [int]$s.byStatus.doing -eq 1 -and [int]$s.byStatus.done -eq 1 -and [int]$s.overdue -eq 1) "body $($r.Content)"; $true })
    }
    finally { Stop-BackgroundServer $server }
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'prod.readme' -Description 'README documents npm run build / npm start' -Advisory -Test {
            $readme = Get-Content -LiteralPath (Join-Path $ws 'README.md') -Raw
            Assert-That ($readme -match 'npm run build' -and $readme -match 'npm start' -and $readme -match 'TASKBOARD_DATA') 'missing production instructions'; $true })
}

function Test-ProtectedUnchanged([string]$Stage, [hashtable]$Hashes) {
    [void](Invoke-Gate -Ctx $ctx -Stage $Stage -Id 'protected-files' -Description 'protected test files are byte-identical' -Test {
            foreach ($path in $Hashes.Keys) {
                $full = Join-Path $ws $path
                Assert-That (Test-Path -LiteralPath $full) "$path deleted"
                Assert-That ((Get-Sha256 $full) -eq $Hashes[$path]) "$path modified"
            }
            $true })
}

#endregion

$namesT1 = @('GET /api/health returns ok', 'POST /api/tasks creates a task', 'POST /api/tasks rejects an empty title',
    'PATCH /api/tasks/:id updates status', 'DELETE /api/tasks/:id removes the task', 'GET /api/tasks filters by status and query')
$namesT3 = $namesT1 + @('POST /api/tasks validates labels', 'GET /api/tasks sorts by priority')
$namesT4 = $namesT3 + @('POST /api/tasks trims title whitespace', 'POST /api/tasks rejects whitespace-only title',
    'PATCH /api/tasks/:id rejects unknown fields', 'PATCH /api/tasks/:id cannot change read-only fields')
$namesT5 = $namesT4 + @('GET /api/stats counts tasks by status')
$uiIds = @('column-todo', 'column-doing', 'column-done', 'task-card', 'task-form', 'search-input')

$exitCode = 1
try {
    Invoke-CommonPreflight $ctx

    # --- B0: seed and baseline (no VCP) ---------------------------------
    $stage = 'B0-baseline'
    Write-Step $ctx 'B0 seed scaffold, npm install, baseline build' 'phase'
    if ($ctx.ReuseProject) {
        foreach ($required in @('package.json', 'server/app.ts', 'server/index.ts', 'src/App.vue', 'tests/health.test.ts')) {
            if (-not (Test-Path -LiteralPath (Join-Path $ws $required) -PathType Leaf)) {
                throw "Existing project is not a TaskBoard scenario project: missing $required. Select its workspace directory or a new empty project directory."
            }
        }
        Write-Step $ctx "Reusing TaskBoard project: $ws (existing source and tests retained)." 'ok'
    }
    else { Write-SeedFiles -Root $ws -Files $seed }
    $installCommand = if ($ctx.ReuseProject -and (Test-Path -LiteralPath (Join-Path $ws 'package-lock.json'))) { 'ci' } else { 'install' }
    $install = Invoke-Npm $stage $installCommand @($installCommand, '--no-audit', '--no-fund') 1800
    if ($install.ExitCode -ne 0) { throw "npm install failed:`n$(Get-Tail ($install.Output + $install.Errors))" }
    Test-Typecheck $stage
    Test-NodeTests $stage @('GET /api/health returns ok')
    Test-UnitAndBuild $stage @() 1
    if ((Get-FailedGates $ctx $stage).Count) { throw 'Baseline scaffold does not build; fix the toolchain before spending on VCP turns.' }
    Initialize-GitCheckpoint $ctx
    $protected = @{ 'tests/health.test.ts' = (Get-Sha256 (Join-Path $ws 'tests\health.test.ts')) }
    if (Test-Path -LiteralPath (Join-Path $ws 'tests/regressions.test.ts')) {
        $protected['tests/regressions.test.ts'] = Get-Sha256 (Join-Path $ws 'tests/regressions.test.ts')
    }

    # --- Profiles ---------------------------------------------------------
    $stage = 'P1-profiles'
    $nodeProcess = New-ProcessProfile -Name 'node' -Executable $node -Ctx $ctx -MaxTimeoutMs 900000
    $affected = @('README.md', 'package.json', 'server', 'src', 'tests', 'vite.config.ts')
    function New-NodeCheck([string[]]$Names, [int]$DeadlineSeconds) {
        return [ordered]@{ manifest = 'package.json'; runner = 'node'; profile = 'node'; timeout_ms = [math]::Min(300000, [long]$DeadlineSeconds * 1000)
            expected_tests = $Names; rationale = 'Owner acceptance: named TaskBoard API tests must pass under node --test.' }
    }
    $profiles = @{}
    foreach ($pair in @(@('T1', $namesT1), @('T2', $namesT1), @('T3', $namesT3), @('T4', $namesT4), @('T5', $namesT5))) {
        $profiles[$pair[0]] = New-ScenarioProfile -Ctx $ctx -Name "profile-$($pair[0])" -AffectedPaths $affected -Processes @($nodeProcess) -Checks @(New-NodeCheck $pair[1] $ctx.DeadlineSeconds)
    }
    $profiles['T5-short'] = New-ScenarioProfile -Ctx $ctx -Name 'profile-T5-short' -AffectedPaths $affected -Processes @($nodeProcess) `
        -Checks @(New-NodeCheck $namesT5 $ctx.ShortDeadlineSeconds) -DeadlineSeconds $ctx.ShortDeadlineSeconds
    $profiles['review'] = New-ScenarioProfile -Ctx $ctx -Name 'profile-review' -AffectedPaths $affected -MaximumAutonomy 'plan' -AutomaticEffects @('read')
    $profiles['guardrail'] = New-ScenarioProfile -Ctx $ctx -Name 'profile-guardrail' -AffectedPaths $affected -MaximumAutonomy 'workspace' -AutomaticEffects @('read', 'write')
    foreach ($key in 'T1', 'T5-short', 'review', 'guardrail') { [void](Test-ProfileCheck $ctx $stage $profiles[$key] $key) }

    # --- G0: zero-spend guardrail ----------------------------------------
    $guardPrompt = Join-Path $ctx.Logs 'G0-guardrail\prompt.md'
    Write-Utf8File $guardPrompt 'Guardrail probe. This task must be rejected before execution.'
    Invoke-GuardrailRun -Ctx $ctx -Stage 'G0-guardrail' -Id 'autonomy-ceiling' -Config $profiles['guardrail'] `
        -Description 'run --autonomy autonomous above the profile ceiling (workspace) is rejected, exit 2, no task' `
        -Arguments @('run', '--file', $guardPrompt, '--budget-usd', '0.01', '--autonomy', 'autonomous') -ExpectStderr 'exceeds'

    if ($ctx.SkipPaidStages) {
        Write-Step $ctx 'Dry run (-SkipPaidStages): toolchain, seed, baseline, profiles and guardrail verified; stopping before paid stages.' 'ok'
        throw 'VCP_SCENARIO_DRY_RUN_COMPLETE'
    }

    # --- T1: API -----------------------------------------------------------
    $gatesT1 = { param($s) Test-Typecheck $s; Test-NodeTests $s $namesT1; Test-ApiContract $s; Test-ProtectedUnchanged $s $protected }
    $t1 = Invoke-VcpTask -Ctx $ctx -Stage 'T1-api' -Title 'Task REST API with persistence' -Prompt $promptT1 -Config $profiles['T1'] -AcceptExit @(0)
    if ($t1) { Test-StageExit $ctx $t1 'T1-api'; & $gatesT1 'T1-api'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T1-api' -Config $profiles['T1'] -GateScript $gatesT1) }
    Save-Checkpoint $ctx 'T1: task REST API'

    # --- T2: UI ------------------------------------------------------------
    $gatesT2 = { param($s) Test-Typecheck $s; Test-UnitAndBuild $s $uiIds 4; Test-NodeTests $s $namesT1; Test-ProtectedUnchanged $s $protected }
    $t2 = Invoke-VcpTask -Ctx $ctx -Stage 'T2-ui' -Title 'Vue board UI' -Prompt $promptT2 -Config $profiles['T2']
    if ($t2) { Test-StageExit $ctx $t2 'T2-ui'; & $gatesT2 'T2-ui'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T2-ui' -Config $profiles['T2'] -GateScript $gatesT2) }
    Save-Checkpoint $ctx 'T2: Vue board UI'

    # --- T3: cross-stack feature -------------------------------------------
    $gatesT3 = { param($s) Test-Typecheck $s; Test-NodeTests $s $namesT3; Test-UnitAndBuild $s ($uiIds + @('label-chip', 'sort-select')) 5; Test-LabelsAndSort $s; Test-ApiContract $s; Test-ProtectedUnchanged $s $protected }
    $t3 = Invoke-VcpTask -Ctx $ctx -Stage 'T3-labels' -Title 'Labels and sorting across API and UI' -Prompt $promptT3 -Config $profiles['T3']
    if ($t3) { Test-StageExit $ctx $t3 'T3-labels'; & $gatesT3 'T3-labels'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T3-labels' -Config $profiles['T3'] -GateScript $gatesT3) }
    Save-Checkpoint $ctx 'T3: labels and sorting'

    # --- T4: protected regression tests ------------------------------------
    Write-SeedFiles -Root $ws -Files @{ 'tests/regressions.test.ts' = $regressionTest } -MissingOnly
    $packagePath = Join-Path $ws 'package.json'
    $packageText = [System.IO.File]::ReadAllText($packagePath)
    if (-not $ctx.ReuseProject -and $packageText -notmatch 'tests/regressions\.test\.ts') {
        $packageText = [regex]::Replace($packageText, '("test"\s*:\s*"node --test)', '$1 tests/regressions.test.ts', 1)
        Write-Utf8File $packagePath $packageText
    }
    Save-Checkpoint $ctx 'T4 setup: protected regression tests added by harness'
    if (-not $protected.ContainsKey('tests/regressions.test.ts')) {
        $protected['tests/regressions.test.ts'] = Get-Sha256 (Join-Path $ws 'tests/regressions.test.ts')
    }
    $gatesT4 = { param($s) Test-Typecheck $s; Test-UnitAndBuild $s ($uiIds + @('label-chip', 'sort-select')) 5; Test-NodeTests $s $namesT4; Test-ProtectedUnchanged $s $protected; Test-ApiContract $s; Test-LabelsAndSort $s }
    $t4 = Invoke-VcpTask -Ctx $ctx -Stage 'T4-regressions' -Title 'Make protected regression tests pass' -Prompt $promptT4 -Config $profiles['T4']
    if ($t4) { Test-StageExit $ctx $t4 'T4-regressions'; & $gatesT4 'T4-regressions'; [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T4-regressions' -Config $profiles['T4'] -GateScript $gatesT4) }
    Save-Checkpoint $ctx 'T4: regression fixes'

    # --- T5: production, under a short deadline, then resume --last --------
    $gatesT5 = { param($s) Test-Typecheck $s; Test-NodeTests $s $namesT5; Test-UnitAndBuild $s ($uiIds + @('stats-bar')) 5; Test-Production $s; Test-ProtectedUnchanged $s $protected }
    $t5 = Invoke-VcpTask -Ctx $ctx -Stage 'T5-production' -Title 'Production serving and stats (short deadline)' -Prompt $promptT5 `
        -Config $profiles['T5-short'] -AcceptExit @(0, 3, 8)
    if ($t5) {
        Test-StageExit $ctx $t5 'T5-production'
        if ($t5.exit_code -eq 8) {
            $resumed = Invoke-VcpContinuation -Ctx $ctx -Stage 'T5-resume' -Title 'resume --last after deadline pause' `
                -Arguments @('resume', '--last') -Config $profiles['T5'] -AcceptExit @(0, 3)
            if ($resumed) {
                Test-StageExit $ctx $resumed 'T5-resume'
                [void](Invoke-Gate -Ctx $ctx -Stage 'T5-resume' -Id 'resume-same-task' -Description 'resume --last continued the paused T5 task' -Test {
                        Assert-That ($resumed.task -eq $t5.task) "resumed task '$($resumed.task)' != paused task '$($t5.task)'"; $true })
            }
        }
        else {
            [void](Skip-Gate $ctx 'T5-resume' 'resume-same-task' 'resume --last continued the paused T5 task' "T5 ended with exit $($t5.exit_code) before the short deadline; continuation not exercised")
        }
        & $gatesT5 'T5-production'
        [void](Invoke-RepairLoop -Ctx $ctx -Stage 'T5-production' -Config $profiles['T5'] -GateScript $gatesT5)
    }
    Save-Checkpoint $ctx 'T5: production serving and stats'

    # --- T6: plan-mode review -------------------------------------------------
    [void](Invoke-PlanModeReview -Ctx $ctx -Stage 'T6-review' -Config $profiles['review'] -Prompt $promptReview)

    # --- FINAL: clean install, full regression, packaged assets -------------
    $stage = 'FINAL'
    Write-Step $ctx 'FINAL independent verification and packaging' 'phase'
    [void](Invoke-Gate -Ctx $ctx -Stage $stage -Id 'npm-ci' -Description 'npm ci reproduces node_modules from package-lock.json' -Test {
            $run = Invoke-Npm $stage 'ci' @('ci', '--no-audit', '--no-fund') 1800
            Assert-That ($run.ExitCode -eq 0) ("exit {0}`n{1}" -f $run.ExitCode, (Get-Tail ($run.Output + $run.Errors))); $true })
    Test-Typecheck $stage
    Test-NodeTests $stage $namesT5
    Test-UnitAndBuild $stage ($uiIds + @('label-chip', 'sort-select', 'stats-bar')) 5
    Test-ApiContract $stage
    Test-LabelsAndSort $stage
    Test-Production $stage
    Test-ProtectedUnchanged $stage $protected
    $artifacts = Join-Path $ws 'artifacts'
    New-Item -ItemType Directory -Force -Path $artifacts | Out-Null
    if (Test-Path -LiteralPath (Join-Path $ws 'dist\client\index.html')) {
        $zip = Join-Path $artifacts "taskboard-client-$($ctx.RunId).zip"
        Compress-Archive -Path (Join-Path $ws 'dist\client\*') -DestinationPath $zip -Force
        Add-Asset $ctx $zip 'Production client bundle (vite build of dist/client)'
        Add-Asset $ctx (Join-Path $ws 'dist\client\index.html') 'Built SPA entry point'
    }
    Add-Asset $ctx (Join-Path $ws 'package-lock.json') 'Resolved dependency lockfile'
    Save-Checkpoint $ctx 'FINAL: verified state'

    Invoke-FinalEvidenceSweep $ctx 'tasks'
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
