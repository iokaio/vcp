// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');

const repository = path.resolve(__dirname, '../..');
const fixtures = path.join(repository, 'src/evals/skills/builtin');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function contained(root, relative) {
  if (typeof relative !== 'string' || !relative || relative.includes('\\') || relative.split('/').some(p => !p || p === '.' || p === '..') || path.isAbsolute(relative) || relative.includes(':')) throw Error('Unsafe fixture path');
  const target = path.resolve(root, relative);
  let current = root;
  for (const part of relative.split('/')) {
    current = path.join(current, part);
    if (fs.existsSync(current) && fs.lstatSync(current).isSymbolicLink()) throw Error('Linked fixture input');
  }
  return target;
}
function snapshot(root) {
  const files = [];
  function visit(dir, prefix = '') {
    if (fs.lstatSync(dir).isSymbolicLink()) throw Error('Linked fixture input');
    for (const name of fs.readdirSync(dir).sort()) {
      const relative = prefix + name, filename = path.join(dir, name), stat = fs.lstatSync(filename);
      if (stat.isSymbolicLink()) throw Error('Linked fixture input');
      if (stat.isDirectory()) visit(filename, relative + '/');
      else if (stat.isFile()) files.push({ path: relative, bytes: stat.size, sha256: hash(fs.readFileSync(filename)) });
      else throw Error('Unsupported fixture input');
    }
  }
  visit(root);
  return { files, sha256: hash(JSON.stringify(files)) };
}
function materialize(source, fixture, target) {
  const project = contained(source, fixture.project);
  // Verify the entire declared inventory before creating any execution workspace.
  const inputs = fixture.expected.preserve_files.map(file => {
    const filename = contained(project, file.path), bytes = fs.readFileSync(filename);
    if (bytes.length !== file.bytes || hash(bytes) !== file.sha256) throw Error('Frozen fixture hash mismatch: ' + fixture.id + '/' + file.path);
    return { file, bytes };
  });
  fs.mkdirSync(target); // Never overwrite or reuse a previous execution workspace.
  for (const { file, bytes } of inputs) {
    const filename = contained(target, file.path);
    fs.mkdirSync(path.dirname(filename), { recursive: true });
    fs.writeFileSync(filename, bytes, { flag: 'wx' });
  }
}
function execute(command, args, cwd, options = {}) {
  const started = Date.now();
  const environment = { ...process.env, CARGO_NET_OFFLINE: 'true', RUSTUP_AUTO_INSTALL: '0',
    DOTNET_SKIP_FIRST_TIME_EXPERIENCE: '1', DOTNET_CLI_TELEMETRY_OPTOUT: '1',
    GOTOOLCHAIN: 'local', GOPROXY: 'off', GOSUMDB: 'off', ...options.env };
  // A parent node:test worker flag would suppress execution in nested --test runs.
  delete environment.NODE_TEST_CONTEXT;
  const result = spawnSync(command, args, {
    cwd, encoding: 'utf8', windowsHide: true, shell: false, timeout: options.timeout ?? 120000,
    maxBuffer: 4 * 1024 * 1024,
    env: environment,
  });
  return { command, arguments: args, cwd, exit_code: result.status, signal: result.signal,
    error: result.error?.code ?? null, duration_ms: Date.now() - started,
    stdout: result.stdout ?? '', stderr: result.stderr ?? '',
    status: !result.error && result.status === 0 ? 'passed' : 'failed' };
}
const probes = {
  node: [process.execPath, ['--version']], pwsh: ['pwsh', ['-NoLogo', '-NoProfile', '-Command', '$PSVersionTable.PSVersion.ToString()']],
  rustup: ['rustup', ['toolchain', 'list']], python: ['python', ['--version']], dotnet: ['dotnet', ['--list-sdks']],
  java: ['java', ['-version']], go: ['go', ['version']], cmake: ['cmake', ['--version']],
  ninja: ['ninja', ['--version']], ctest: ['ctest', ['--version']],
  ruby: ['ruby', ['--version']], php: ['php', ['--version']], swift: ['swift', ['--version']], dart: ['dart', ['--version']],
};
function plan(fixture, inventory) {
  if (fixture.kind !== 'normal') return { reason: 'Negative/scenario fixture: analysis-only; synthetic missing-tool state is not host discovery.' };
  const available = tool => inventory[tool]?.status === 'passed';
  const command = (tool, args, cwd = '.') => ({ command: probes[tool][0], args, cwd });
  switch (fixture.skill) {
    case 'review-debug': return { steps: [command('node', ['check.cjs'])] };
    case 'testing': return { steps: [command('node', ['--test', 'unit.cjs']), command('node', ['--test', 'integration.cjs'])] };
    case 'javascript-typescript': return { steps: [command('node', ['--test', 'test.mjs'], 'packages/web')], limitations: ['Direct invocation of declared test script; TypeScript dependencies are not provisioned; typecheck not-run.'] };
    case 'shell': return available('pwsh') ? { steps: [command('pwsh', ['-NoLogo', '-NoProfile', '-File', 'copy-name.ps1', '-InputPath', 'folder with spaces/value.txt'])] } : { reason: 'PowerShell unavailable.' };
    case 'dotnet-powershell': return available('pwsh') ? { steps: [command('pwsh', ['-NoLogo', '-NoProfile', '-File', 'scripts/path-check.ps1', '-FixturePath', 'folder with spaces/sentinel.txt'])], limitations: ['Only PowerShell literal-path check; .NET test adapter dependencies are not provisioned; .NET build/test not-run. Existing obj files are excluded from the fresh copy.'] } : { reason: 'PowerShell unavailable; .NET test adapter dependencies not provisioned.' };
    case 'rust': {
      const installed = inventory.rustup?.stdout.split(/\r?\n/).map(s => s.split(/\s/)[0]).filter(s => /^1\.95\.0(?:-|$)/.test(s)) ?? [];
      return installed.length ? { steps: [{ command: 'rustup', args: ['run', installed[0], 'cargo', 'test', '--locked', '--offline', '-p', 'fixture-math', '--features', 'checked', '--test', 'checked'], cwd: '.' }] } : { reason: 'Pinned Rust 1.95.0 is not installed; automatic installation prohibited.' };
    }
    case 'python': return { reason: 'Requires a provisioned Python 3.12 project environment and pytest; no environment is supplied to this bounded runner.' };
    case 'jvm': return { reason: 'Declared Maven wrapper is an intentionally unavailable stub; Java availability alone cannot run the fixture.' };
    case 'cpp': return available('cmake') && available('ninja') && available('ctest') ? {
      steps: [command('cmake', ['-S', '.', '-B', 'build', '-G', 'Ninja', '-DCMAKE_BUILD_TYPE=Debug']),
        command('cmake', ['--build', 'build']), command('ctest', ['--test-dir', 'build', '--output-on-failure', '--no-tests=error'])],
      limitations: ['Requires a provisioned native compiler environment; no compiler or dependency installation.']
    } : { reason: 'CMake, Ninja and CTest must all be provisioned; no installation attempted.' };
    case 'go': case 'ruby': case 'php': case 'swift': case 'dart': {
      const tool = fixture.skill;
      return { reason: available(tool) ? 'Tool detected, but this runner has no qualified offline execution recipe for this fixture.' : tool + ' is unavailable; no installation attempted.' };
    }
    default: return { reason: 'Analysis-only fixture; no bounded native check is declared by this runner.' };
  }
}
function run(outputRoot = path.join(repository, 'artifacts/p7-builtin-toolchain')) {
  outputRoot = path.resolve(outputRoot);
  for (const protectedRoot of [path.join(repository, 'src'), path.join(repository, 'scripts')]) {
    if (outputRoot === protectedRoot || outputRoot.startsWith(protectedRoot + path.sep)) throw Error('Output must be outside source directories');
  }
  fs.mkdirSync(outputRoot, { recursive: true });
  const directory = fs.mkdtempSync(path.join(outputRoot, 'run-'));
  const filename = path.join(directory, 'manifest.json');
  const record = { schema: 'p7-builtin-toolchain-qualification/1', started_at: new Date().toISOString(),
    status: 'running', host: { platform: process.platform, release: os.release(), architecture: process.arch },
    runner_sha256: hash(fs.readFileSync(__filename)), model_calls: 0, live_usefulness: 'not_run', inventory: {}, cases: [] };
  const save = () => fs.writeFileSync(filename, JSON.stringify(record, null, 2) + '\n');
  save();
  try {
    record.source_before = snapshot(fixtures);
    const git = execute('git', ['-c', 'safe.directory=' + repository.replaceAll('\\', '/'), 'rev-parse', 'HEAD'], repository);
    if (git.status !== 'passed') throw Error('Source commit identity unavailable');
    record.commit = git.stdout.trim();
    const manifest = JSON.parse(fs.readFileSync(path.join(fixtures, 'manifest.json'), 'utf8'));
    record.fixture_revision = manifest.revision;
    for (const [name, [command, args]] of Object.entries(probes)) {
      record.inventory[name] = execute(command, args, directory, { timeout: 15000 });
      save();
    }
    for (const fixture of manifest.cases) {
      const result = { id: fixture.id, skill: fixture.skill, status: 'not_run', steps: [] };
      record.cases.push(result);
      try {
        const workspace = contained(directory, fixture.id);
        materialize(fixtures, fixture, workspace);
        const selected = plan(fixture, record.inventory);
        result.reason = selected.reason ?? null;
        result.limitations = selected.limitations ?? [];
        for (const step of selected.steps ?? []) {
          const receipt = execute(step.command, step.args, path.resolve(workspace, step.cwd));
          result.steps.push(receipt);
          save(); // Preserve all attempted failures, including a later stage interruption.
        }
        if (result.steps.length) result.status = result.steps.every(s => s.status === 'passed') ? 'passed' : 'failed';
      } catch (error) { result.status = 'error'; result.error = error.message; }
      save();
    }
    record.counts = Object.fromEntries(['passed', 'failed', 'not_run', 'error'].map(status => [status, record.cases.filter(c => c.status === status).length]));
    record.status = record.counts.error ? 'error' : record.counts.failed ? 'completed_with_failures' : 'completed';
    record.qualification = 'partial';
  } catch (error) { record.status = 'error'; record.error = error.message; }
  finally {
    try {
      record.source_after = snapshot(fixtures);
      record.runner_sha256_after = hash(fs.readFileSync(__filename));
      record.source_unchanged = record.source_before?.sha256 === record.source_after.sha256 && record.runner_sha256 === record.runner_sha256_after;
      if (!record.source_unchanged) { record.status = 'error'; record.source_error = 'Fixture or runner bytes changed, or initial identity unavailable'; }
    } catch (error) { record.status = 'error'; record.source_error = error.message; }
    record.finished_at = new Date().toISOString(); save();
  }
  return { filename, record };
}
if (require.main === module) {
  if (process.argv.length > 3) throw Error('Usage: node builtin-toolchain-qualification.cjs [output-root]');
  const result = run(process.argv[2]);
  process.stdout.write(result.filename + '\n');
  if (result.record.status !== 'completed') process.exitCode = 1;
}
module.exports = { contained, snapshot, materialize, execute, plan, run };
