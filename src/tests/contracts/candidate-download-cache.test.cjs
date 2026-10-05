// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const repository = path.resolve(__dirname, '../../..');
const workflow = fs.readFileSync(path.join(repository, '.github/workflows/beta-candidate.yml'), 'utf8');
const steps = workflow.split(/^      - /m).slice(1);
const cacheSteps = steps.filter(step => step.includes('uses: actions/cache@'));

test('candidate caches only isolated download stores with pinned actions and exact platform/tool/lock keys', () => {
  assert.equal(cacheSteps.length, 2);
  const initialize = steps.findIndex(step => step.startsWith('name: Initialize isolated download homes'));
  assert(initialize >= 0 && initialize < steps.indexOf(cacheSteps[0]) && initialize < steps.indexOf(cacheSteps[1]));
  assert.match(steps[initialize], /CARGO_HOME=\$\(Join-Path \$env:RUNNER_TEMP 'vcp-candidate-cargo'\)/);
  assert.match(steps[initialize], /npm_config_cache=\$\(Join-Path \$env:RUNNER_TEMP 'vcp-candidate-npm'\)/);
  assert.match(steps[initialize], /\$env:GITHUB_ENV/);
  assert.doesNotMatch(workflow.split(/^    steps:/m)[0], /\$\{\{\s*runner\./, 'runner context is unavailable in job env');
  const paths = cacheSteps.map(step => {
    assert.match(step, /uses: actions\/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9/);
    assert.match(step, /key: beta-(?:npm|cargo)-downloads-v1-\$\{\{ runner.os \}\}-\$\{\{ runner.arch \}\}-/);
    assert(step.includes("'release/candidate-tools.json'"));
    const section = step.split('path: ')[1].split(/\r?\n          key:/)[0];
    return section.replace(/^\|\r?\n/, '').split(/\r?\n/).map(line => line.trim());
  });
  assert.deepEqual(paths, [
    ['${{ env.npm_config_cache }}/_cacache/'],
    ['${{ env.CARGO_HOME }}/registry/index/', '${{ env.CARGO_HOME }}/registry/cache/', '${{ env.CARGO_HOME }}/git/db/'],
  ]);
  for (const lock of ['src/tests/package-lock.json', 'src/packages/sdk-ts/package-lock.json', 'src/packages/vscode/package-lock.json']) {
    assert(cacheSteps[0].includes("'" + lock + "'"));
  }
  for (const pin of ['src/third_party/codex/codex-rs/rust-toolchain.toml', 'src/third_party/codex/codex-rs/Cargo.lock']) {
    assert(cacheSteps[1].includes("'" + pin + "'"));
  }
  assert(!workflow.includes('cache-hit'), 'a cache hit must never bypass installation or verification');
  assert.match(workflow, /npm\.cmd ci --prefix \$package --ignore-scripts --no-audit --no-fund --prefer-offline/);
  const install = steps.findIndex(step => step.startsWith('name: Provision locked packaging'));
  assert(steps.indexOf(cacheSteps[0]) < install && steps.indexOf(cacheSteps[1]) < install);
  assert.match(workflow, /if: github.ref == 'refs\/heads\/main'/);
  assert.match(workflow, /\$env:REVIEWED_COMMIT -cne \$env:GITHUB_SHA/);
});

test('download fallback survives product lock version bumps but isolates platform and tool pins', () => {
  const digest = value => crypto.createHash('sha256').update(value).digest('hex');
  // Substitute deterministic file digests in the actual workflow expressions;
  // this checks prefix relationships, without claiming to execute Actions.
  function render(template, files, platform = { os: 'Windows', arch: 'X64' }) {
    return template.replace(/\$\{\{ runner\.(os|arch) \}\}/g, (_match, key) => platform[key])
      .replace(/\$\{\{ hashFiles\((.*?)\) \}\}/g, (_match, args) => {
        const names = [...args.matchAll(/'([^']+)'/g)].map(match => match[1]);
        assert(names.length > 0);
        return digest(names.map(name => digest(files[name])).join(''));
      });
  }
  const tools = 'release/candidate-tools.json', rust = 'src/third_party/codex/codex-rs/rust-toolchain.toml';
  for (const step of cacheSteps) {
    const key = step.match(/^          key: (.+)$/m)[1].trim();
    const prefixes = [...step.matchAll(/^          restore-keys: (.+)$/gm)].map(match => match[1].trim());
    assert.equal(prefixes.length, 1, 'only one platform/tool-scoped fallback allowed');
    const prefix = prefixes[0];
    assert(key.startsWith(prefix));
    assert.match(prefix, /^beta-(?:npm|cargo)-downloads-v1-\$\{\{ runner.os \}\}-\$\{\{ runner.arch \}\}-\$\{\{ hashFiles\(/);
    assert.equal(prefix.includes('package-lock.json') || prefix.includes('Cargo.lock'), false);
    assert(prefix.includes("'" + tools + "'"));
    assert.equal(prefix.includes("'" + rust + "'"), step.includes('beta-cargo-downloads'));
    const names = [...key.matchAll(/'([^']+)'/g)].map(match => match[1]);
    const files = Object.fromEntries(names.map(name => [name, fs.readFileSync(path.join(repository, name), 'utf8')]));
    const original = render(key, files), fallback = render(prefix, files);
    const updated = { ...files };
    for (const name of names.filter(name => /(?:Cargo\.lock|package-lock\.json)$/.test(name))) {
      // A synchronized candidate version changes lock bytes, without changing
      // the tools pin or the hashes governing downloaded third-party packages.
      if (name.endsWith('Cargo.lock')) {
        updated[name] = files[name].replace(/(name = "vcp-cli"\r?\nversion = ")(\d+\.\d+\.)(\d+)(")/,
          (_match, before, series, patch, after) => before + series + (Number(patch) + 1) + after);
        assert.notEqual(updated[name], files[name]);
      } else if (name !== 'src/tests/package-lock.json') {
        const lock = JSON.parse(files[name]), version = lock.version.replace(/\d+$/, patch => Number(patch) + 1);
        assert.notEqual(version, lock.version);
        lock.version = version;
        lock.packages[''].version = version;
        updated[name] = JSON.stringify(lock);
      }
    }
    assert.notEqual(render(key, updated), original, 'new locks must save under a new immutable key');
    assert.equal(render(prefix, updated), fallback, 'new candidate locks must retain download fallback');
    assert(original.startsWith(render(prefix, updated)));
    for (const changed of [{ ...files, [tools]: files[tools] + '\nchanged tools pin\n' },
      ...(files[rust] ? [{ ...files, [rust]: files[rust] + '\nchanged Rust pin\n' }] : [])]) {
      assert(!original.startsWith(render(prefix, changed)), 'tool changes must isolate restored downloads');
    }
    for (const platform of [{ os: 'Linux', arch: 'X64' }, { os: 'Windows', arch: 'ARM64' }]) {
      assert(!original.startsWith(render(prefix, files, platform)), 'platform changes must isolate restored downloads');
    }
  }
});

test('actual provision body installs qualification compiler only for its selected checkpoint and retains locked fetch/editor checks',
  { skip: process.platform !== 'win32' }, t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-candidate-provision-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const runner = path.join(root, 'exercise.ps1');
    fs.writeFileSync(runner, String.raw`param([string]$Candidate)
$ErrorActionPreference='Stop'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($Candidate,[ref]$tokens,[ref]$errors)
if($errors.Count){throw ($errors.Message -join ', ')}
$selection=@($ast.FindAll({param($item) $item -is [Management.Automation.Language.IfStatementAst] -and $item.Clauses[0].Item1.Extent.Text -ceq "Selected-Stage 'provision'"},$true))
if($selection.Count -ne 1){throw 'Expected one provision selection'}
function Selected-Stage([string]$Id){return $Id -ceq 'provision'}
function Stage([string]$Id,[string[]]$Command,[string]$Description,[scriptblock]$Body){$script:recorded=$Command;& $Body}
function Checked([string]$Program,[string[]]$Arguments){$script:calls+=@{program=$Program;arguments=$Arguments}}
function Download([string]$Url,[string]$Destination,[string]$Sha256){$script:downloads+=@{url=$Url;sha256=$Sha256}}
function Expand-Archive([string]$LiteralPath,[string]$DestinationPath){}
function Resolve-BetaEditor([string]$Code){return @{version='1.138.0';code=$Code;code_sha256=('c'*64)}}
$out=$PSScriptRoot;$workspace=$PSScriptRoot
$channel=@{installer=@{version='6.5.4';download_url='https://example.invalid/setup';sha256=('a'*64)};vscode_version='1.138.0'}
$tools=@{editor=@{version='1.138.0';url='https://example.invalid/editor'}};$EditorArchiveSha256='b'*64
$rows=@()
foreach($StopAfter in @('portable-contracts','production-build','pair','installed-editor')){
 $script:calls=@();$script:downloads=@();$run=@{environment=@{}}
 & ([scriptblock]::Create($selection[0].Extent.Text))
 $rows+=@{stop_after=$StopAfter;commands=$script:calls;recorded=$script:recorded;downloads=$script:downloads;editor=$run.environment.editor_layout.version}
}
$rows | ConvertTo-Json -Depth 8 -Compress
`);
    const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', runner,
      '-Candidate', path.join(repository, 'scripts/release/candidate.ps1')],
    { encoding: 'utf8', windowsHide: true, timeout: 15000 });
    assert.ifError(result.error); assert.equal(result.status, 0, result.stderr + result.stdout);
    const rows = JSON.parse(result.stdout);
    assert.equal(rows.length, 4);
    for (const row of rows) {
      const versions = row.stop_after === 'installed-editor' ? ['1.95.0', '1.98.0'] : ['1.95.0'];
      assert.deepEqual(row.commands.filter(call => call.program === 'rustup').map(call => call.arguments),
        versions.map(version => ['toolchain', 'install', version, '--profile', 'minimal']));
      assert.deepEqual(row.commands.filter(call => call.program === 'cargo').map(call => call.arguments),
        [['+1.95.0', 'fetch', '--locked', '--target', 'x86_64-pc-windows-msvc']]);
      assert.deepEqual(row.recorded, ['rustup', 'toolchain', 'install', ...versions, '--profile', 'minimal', ';',
        'cargo', '+1.95.0', 'fetch', '--locked', '--target', 'x86_64-pc-windows-msvc']);
      assert.deepEqual(row.downloads.map(download => download.sha256), ['a'.repeat(64), 'b'.repeat(64)]);
      assert.equal(row.editor, '1.138.0');
    }
  });
