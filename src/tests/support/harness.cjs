// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawn, spawnSync, execFileSync } = require('node:child_process');
const { StringDecoder } = require('node:string_decoder');

const digest = value => crypto.createHash('sha256').update(value).digest('hex');
const has = (object, key) => Object.hasOwn(object, key);
function validateRegistry(registry) {
  if (registry.schema_version !== 1 || !registry.suites || !registry.cases) throw Error('Invalid registry version or shape.');
  for (const [id, spec] of Object.entries(registry.cases)) {
    if (!/^[a-z][a-z0-9-]*$/.test(id) || !Array.isArray(spec.args) || spec.args.length === 0 ||
        spec.args.some(a => typeof a !== 'string') || !Array.isArray(spec.task_ids) ||
        !Array.isArray(spec.backends) || spec.backends.length === 0 ||
        spec.backends.some(b => !['none', 'sqlite', 'files'].includes(b)) ||
        !Array.isArray(spec.requires) || spec.requires.some(r => !r || !['file', 'platform'].includes(r.kind) || typeof r.value !== 'string') ||
        !Number.isInteger(spec.timeout_ms) || spec.timeout_ms < 1 ||
        !Number.isInteger(spec.max_output_bytes) || spec.max_output_bytes < 1) throw Error('Invalid registered case: ' + id);
  }
  for (const [name, ids] of Object.entries(registry.suites)) {
    if (!/^[a-z][a-z0-9-]*$/.test(name) || !Array.isArray(ids) || ids.length === 0 ||
        new Set(ids).size !== ids.length || ids.some(id => !has(registry.cases, id))) throw Error('Invalid suite: ' + name);
  }
}
function parseSelection(argv, registry) {
  validateRegistry(registry);
  const options = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    if (!['--suite', '--case', '--backend', '--output-root'].includes(key) || has(options, key) ||
        !argv[i + 1] || argv[i + 1].startsWith('--')) throw Error('Unknown, duplicated or missing option: ' + key);
    options[key] = argv[i + 1];
  }
  const suite = options['--suite'] || 'fast';
  if (!has(registry.suites, suite)) throw Error('Unknown suite: ' + suite);
  const ids = options['--case'] ? [options['--case']] : registry.suites[suite];
  if (ids.some(id => !registry.suites[suite].includes(id))) throw Error('Case is not registered in suite: ' + ids.join(','));
  const backend = options['--backend'] || 'none';
  if (!['none', 'sqlite', 'files', 'both'].includes(backend)) throw Error('Unknown backend: ' + backend);
  const backends = backend === 'both' ? ['sqlite', 'files'] : [backend];
  for (const id of ids) if (backends.some(b => !registry.cases[id].backends.includes(b))) throw Error('Unsupported backend for case: ' + id);
  return { suite, ids, backends, outputRoot: options['--output-root'] };
}
function hashCommand(executable, args, { cwd, signal } = {}) {
  return new Promise((resolve, reject) => {
    const hash = crypto.createHash('sha256');
    const child = spawn(executable, args, { cwd, signal, windowsHide: true,
      env: environment(), stdio: ['ignore', 'pipe', 'pipe'] });
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; child.kill(); }, 120000);
    child.stdout.on('data', bytes => hash.update(bytes));
    // Drain diagnostics without buffering source-dependent output.
    child.stderr.resume();
    child.on('error', error => { clearTimeout(timer); reject(error); });
    child.on('close', (code, exitSignal) => {
      clearTimeout(timer);
      if (timedOut || code !== 0 || exitSignal) reject(Error('Source hashing command failed: ' + (timedOut ? 'timeout' : code ?? exitSignal)));
      else resolve(hash.digest('hex'));
    });
  });
}
async function sourceIdentity(root, signal) {
  const git = args => execFileSync('git', args, { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 64 * 1024 * 1024 });
  const status = git(['status', '--porcelain=v1', '-z', '--untracked-files=all']);
  const untracked = git(['ls-files', '--others', '--exclude-standard', '-z']).toString().split('\0').filter(Boolean).sort();
  return {
    commit: git(['rev-parse', 'HEAD']).toString().trim(),
    git_version: git(['--version']).toString().trim(),
    dirty: status.length > 0,
    status_sha256: digest(status),
    tracked_diff_sha256: await hashCommand('git', ['diff', '--no-ext-diff', '--no-textconv', '--binary', 'HEAD'], { cwd: root, signal }),
    untracked_sha256: digest(JSON.stringify(untracked.map(p => [p, digest(fs.readFileSync(path.join(root, p)))])))
  };
}
function writeManifest(file, manifest) {
  const temporary = file + '.' + crypto.randomUUID() + '.tmp';
  const fd = fs.openSync(temporary, 'wx', 0o600);
  try {
    fs.writeFileSync(fd, JSON.stringify(manifest, null, 2) + '\n');
    fs.fsyncSync(fd);
  } finally { fs.closeSync(fd); }
  fs.renameSync(temporary, file);
}
function environment() {
  const permitted = new Set(['PATH', 'SYSTEMROOT', 'WINDIR', 'COMSPEC', 'PATHEXT', 'TEMP', 'TMP', 'HOME', 'TMPDIR', 'LANG', 'LC_ALL']);
  return Object.fromEntries(Object.entries(process.env).filter(([key]) => permitted.has(key.toUpperCase())));
}
function redactor(write, sensitiveValues = []) {
  const secrets = sensitiveValues.filter(Boolean).sort((a, b) => b.length - a.length);
  const tail = Math.max(0, ...secrets.map(s => s.length - 1));
  const decoder = new StringDecoder('utf8');
  let carry = '';
  function flush(final) {
    let i = 0, result = '';
    while (i < carry.length && (final || i < carry.length - tail)) {
      const match = secrets.find(s => carry.startsWith(s, i));
      if (match) { result += '[REDACTED]'; i += match.length; }
      else { result += carry[i]; i++; }
    }
    carry = carry.slice(i);
    if (result) write(result);
  }
  return {
    push(bytes) { carry += decoder.write(bytes); flush(false); },
    end() { carry += decoder.end(); flush(true); }
  };
}
function stopTree(child) {
  if (!child.pid) return;
  if (process.platform === 'win32') {
    if (child.exitCode !== null || child.signalCode !== null) return;
    // This only terminates the child this harness just started, not arbitrary PIDs.
    const killer = spawnSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], {
      windowsHide: true, stdio: 'ignore', timeout: 5000
    });
    if (killer.status !== 0) child.kill();
  } else {
    try { process.kill(-child.pid, 'SIGKILL'); } catch { child.kill('SIGKILL'); }
  }
}
async function execute(spec, options) {
  const { root, directory, attempt, signal, sensitiveValues = [] } = options;
  const files = ['stdout.log', 'stderr.log'].map(name => path.join(directory, attempt.attempt_id + '-' + name));
  const handles = [];
  let captureError, reason, observed = 0, child;
  try {
    for (const file of files) handles.push(fs.openSync(file, 'wx', 0o600));
    const sinks = handles.map(fd => redactor(text => fs.writeSync(fd, text), sensitiveValues));
    return await new Promise((resolve, reject) => {
      let finished = false, timer, abandonTimer;
      function stop(why) {
        reason ||= why;
        stopTree(child);
        // Retain an explicit failure if the host cannot close a stopped child's pipes.
        abandonTimer ||= setTimeout(() => { captureError = true; finish(null, null); }, 6000);
      }
      const cancel = () => stop('cancelled');
      function finish(code, childSignal) {
        if (finished) return;
        finished = true;
        clearTimeout(timer); clearTimeout(abandonTimer);
        signal?.removeEventListener('abort', cancel);
        for (const stream of [child.stdout, child.stderr]) stream?.destroy();
        child.unref();
        try { for (const sink of sinks) sink.end(); }
        catch { captureError = true; }
        try {
          for (const fd of handles) fs.fsyncSync(fd);
          resolve({
          status: code === 0 && !reason && !captureError ? 'pass' : 'fail',
          exit_code: code, signal: childSignal,
          reason: captureError ? 'capture_failed' : reason || (code === 0 ? null : 'child_failed'),
          observed_output_bytes: observed,
          capture_complete: !captureError && reason !== 'output_limit',
          artifacts: files.map(file => ({ path: path.basename(file), sha256: digest(fs.readFileSync(file)), bytes: fs.statSync(file).size }))
          });
        } catch (error) { reject(error); }
      }
      child = spawn(process.execPath, spec.args.map(a => a.replaceAll('{backend}', attempt.backend)), {
        cwd: root, env: environment(), shell: false, windowsHide: true,
        detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe']
      });
      [child.stdout, child.stderr].forEach((stream, i) => stream.on('data', bytes => {
        if (finished) return;
        observed += bytes.length;
        if (observed > spec.max_output_bytes) { stop('output_limit'); return; }
        try { sinks[i].push(bytes); }
        catch { captureError = true; stop('capture_failed'); }
      }));
      child.once('error', () => { reason = 'launch_failed'; finish(null, null); });
      child.once('close', finish);
      signal?.addEventListener('abort', cancel, { once: true });
      timer = setTimeout(() => stop('timeout'), spec.timeout_ms);
      if (signal?.aborted) cancel();
    });
  } finally { for (const fd of handles) fs.closeSync(fd); }
}
async function runSuite({ root, registry, selection, outputRoot, source, signal, announce = () => {}, sensitiveValues = [] }) {
  validateRegistry(registry);
  // Selection is validated before allocating evidence or launching any process.
  const verified = parseSelection(['--suite', selection.suite, ...(selection.ids.length === 1 ? ['--case', selection.ids[0]] : []),
    '--backend', selection.backends.length === 2 ? 'both' : selection.backends[0]], registry);
  if (JSON.stringify(verified.ids) !== JSON.stringify(selection.ids) ||
      JSON.stringify(verified.backends) !== JSON.stringify(selection.backends)) throw Error('Invalid case selection.');
  fs.mkdirSync(outputRoot, { recursive: true });
  const runId = crypto.randomUUID();
  const directory = path.join(outputRoot, runId);
  fs.mkdirSync(directory);
  const manifestPath = path.join(directory, 'manifest.json');
  const now = () => new Date().toISOString();
  const manifest = {
    schema_version: 1, run_id: runId, suite: selection.suite,
    registry_sha256: digest(JSON.stringify(registry)), source,
    started_at: now(), ended_at: null, status: 'running',
    environment: { platform: process.platform, os_release: os.release(), architecture: process.arch, node: process.version, cpu_count: os.cpus().length, total_memory_bytes: os.totalmem() },
    provider_mode: 'none', spend: { known: '0', currency: 'USD', model_requests: 0 },
    limitations: ['Delivery harness evidence only; no product or hardware power-loss qualification.'],
    attempts: []
  };
  writeManifest(manifestPath, manifest);
  for (const id of selection.ids) for (const backend of selection.backends) {
    const spec = registry.cases[id];
    const attempt = {
      attempt_id: crypto.randomUUID(), case_id: id, task_ids: spec.task_ids, backend,
      command: ['node', ...spec.args.map(a => a.replaceAll('{backend}', backend))],
      fixture_sha256: digest(JSON.stringify(spec)), started_at: now(), ended_at: null,
      status: 'running', exit_code: null
    };
    manifest.attempts.push(attempt);
    const missing = spec.requires.filter(r => r.kind === 'platform' ? r.value !== process.platform : !fs.existsSync(path.resolve(root, r.value)));
    if (missing.length || signal?.aborted) {
      attempt.status = 'not_run';
      attempt.reason = signal?.aborted ? 'cancelled_before_start' : 'missing_prerequisite';
      attempt.missing = missing;
    } else {
      writeManifest(manifestPath, manifest);
      announce(attempt.command);
      Object.assign(attempt, await execute(spec, { root, directory, attempt, signal, sensitiveValues }));
    }
    attempt.ended_at = now();
    attempt.duration_ms = Date.parse(attempt.ended_at) - Date.parse(attempt.started_at);
    writeManifest(manifestPath, manifest);
  }
  manifest.status = manifest.attempts.some(a => a.status === 'fail') ? 'fail' :
    manifest.attempts.some(a => a.status === 'not_run') ? 'not_run' : 'pass';
  manifest.ended_at = now();
  manifest.duration_ms = Date.parse(manifest.ended_at) - Date.parse(manifest.started_at);
  const firstFailure = manifest.attempts.find(a => a.status === 'fail');
  const exitCode = signal?.aborted ? 130 : firstFailure ? (firstFailure.exit_code > 0 && firstFailure.exit_code < 256 ? firstFailure.exit_code : 1) : manifest.status === 'not_run' ? 3 : 0;
  manifest.exit_code = exitCode;
  writeManifest(manifestPath, manifest);
  return { manifest, manifestPath, exitCode };
}
module.exports = { digest, parseSelection, sourceIdentity, hashCommand, writeManifest, redactor, runSuite, environment };
