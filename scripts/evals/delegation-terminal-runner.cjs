// SPDX-License-Identifier: Apache-2.0
'use strict';

// Frozen P7-06 U06 terminal campaign. Preparation is offline. Execution is a
// one-shot native run through the supplied PTY driver. A failed or interrupted
// case is never replayed, and its complete allocation remains held.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {spawn, spawnSync} = require('node:child_process');
const readline = require('node:readline');
const prior = require('./p6-live-runner.cjs');
const builtin = require('./builtin-live-runner.cjs');
const {
  plain, read, within, privateDirectory, noParentInstructions, filesUnder, noSecrets,
} = prior.boundaries;
const repo = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = value => JSON.stringify(value, null, 2) + '\n';
const CASES = new Set(['pause-resume', 'hard-close-reopen']);
const MAX_RUN_MS = 115000;
const RUST_MIN_STACK = '16777216';
const PARENT_DRAINED = 'Parent turn interrupted; inspect current state before explicit /resume.';
const PARENT_COMPLETED = 'Parent turn ended while children remain active or paused. Inspect /agents; parent completion still requires integrated verification.';
const RESUME_DRAIN_REJECTION = 'Command rejected: wait for the current turn, steering, shadow evaluation and MCP control to drain before /resume';
const RESUME_QUIESCENCE_REJECTION = 'Command rejected: recovery waits for scheduler and result producers to become quiescent';
const MAX_RESUME_ATTEMPTS = 8;
const MAX_RESUME_WAIT_MS = 5000;
const SYNTHETIC = Object.freeze({
  'README.synthetic.txt': 'U06 synthetic read-only delegation workspace.\n',
  'observations/source.txt': 'The child should report this bounded synthetic input without editing it.\n',
});

function micros(value) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)\.[0-9]{1,6}$/.test(value)) {
    throw Error('USD caps require exact decimal strings');
  }
  const [whole, fraction] = value.split('.');
  const result = BigInt(whole) * 1000000n + BigInt(fraction.padEnd(6, '0'));
  if (result <= 0n || result > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw Error('USD cap must be positive and bounded');
  }
  return Number(result);
}

function boundedText(value, name, max = 512) {
  if (typeof value !== 'string' || !value || value.length > max || value.includes('\0')) {
    throw Error(`invalid ${name}`);
  }
}

function exactKeys(value, expected, name) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
      || Object.keys(value).sort().join(',') !== [...expected].sort().join(',')) {
    throw Error(`${name} fields changed`);
  }
}

function exactFile(file, limit = 1024 * 1024 * 1024) {
  return {path: plain(path.resolve(file)), bytes: read(path.resolve(file), limit)};
}

function writeExclusive(file, value) {
  fs.writeFileSync(file, Buffer.isBuffer(value) ? value : typeof value === 'string' ? value : json(value), {
    flag: 'wx', mode: 0o600,
  });
}

function profileReasons(profile, now = Date.now()) {
  const reasons = builtin.fixedProfileReasons(profile, now);
  if (profile?.maximum_autonomy !== 'plan' || profile?.automatic_effects?.length) {
    reasons.push('read-only plan authority required');
  }
  return reasons;
}

function markerSeen(text, markers) {
  return markers.every(marker => text.includes(marker));
}

function finalTerminalResult(text, offset) {
  const normalized = text.slice(offset)
    .replace(/(.)\r?\n\x1b\[\d+;140H\1/g, '$1')
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '');
  return normalized.split(/\r?\n/).flatMap(line => {
    const value = line.trim();
    if (!value.startsWith('{"conditions":')) return [];
    try { return [JSON.parse(value)]; } catch (_) { return []; }
  }).findLast(frame => frame.type === 'result') || null;
}

function terminalResultReason(frame, exitCode, rootTaskId) {
  if (!frame || frame.type !== 'result' || frame.exit_code !== exitCode
      || frame.scope?.task !== rootTaskId) {
    return 'final terminal result is missing or differs from the structured exit/root task';
  }
  if (![7, 8].includes(exitCode) || frame.conditions?.internal_failure !== false
      || frame.conditions?.durably_paused !== true
      || frame.conditions?.unresolved_effect !== (exitCode === 7)) {
    return 'final terminal result is not a non-failing durable pause';
  }
  return null;
}

function validateSpec(spec) {
  exactKeys(spec, ['schema', 'overall_cap_usd', 'prior_exposure_micros', 'cases'], 'spec');
  if (spec.schema !== 'p7-delegation-terminal-spec/1' || !Array.isArray(spec.cases)
      || spec.cases.length !== 2) {
    throw Error('spec requires exactly two terminal cases');
  }
  if (!Number.isSafeInteger(spec.prior_exposure_micros) || spec.prior_exposure_micros < 0) {
    throw Error('prior exposure is required');
  }
  const overall = micros(spec.overall_cap_usd);
  if (overall > 100000000) throw Error('overall cap exceeds $100');
  const ids = new Set();
  let total = 0;
  const fields = [
    'id', 'executable', 'pty_driver', 'state_exporter', 'profile', 'catalog', 'task_file',
    'git', 'cap_usd', 'child_cap_usd', 'startup_markers', 'child_markers',
    'paused_marker', 'resumed_marker',
  ];
  for (const item of spec.cases) {
    exactKeys(item, fields, `case ${item?.id || '<unknown>'}`);
    if (!CASES.has(item.id) || ids.has(item.id)) {
      throw Error('exact pause-resume and hard-close-reopen cases required');
    }
    ids.add(item.id);
    for (const field of ['executable', 'pty_driver', 'state_exporter', 'profile', 'catalog', 'task_file', 'git']) {
      boundedText(item[field], field, 4096);
    }
    for (const field of ['startup_markers', 'child_markers']) {
      if (!Array.isArray(item[field]) || !item[field].length
          || item[field].some(value => typeof value !== 'string' || !value || value.length > 256)) {
        throw Error('startup and child markers are required');
      }
    }
    if (item.id === 'pause-resume') {
      boundedText(item.paused_marker, 'paused marker', 256);
      boundedText(item.resumed_marker, 'resumed marker', 256);
      if (item.paused_marker.includes('/pause') || item.resumed_marker.includes('/resume')) {
        throw Error('pause and resume markers must identify acknowledgements, not echoed commands');
      }
    } else if (item.paused_marker !== null || item.resumed_marker !== null) {
      throw Error('hard-close-reopen does not accept pause markers');
    }
    const cap = micros(item.cap_usd);
    const child = micros(item.child_cap_usd);
    if (child >= cap) throw Error('child cap must be below root cap');
    total += cap;
  }
  if (spec.prior_exposure_micros + total > overall) {
    throw Error('prior exposure plus case caps exceeds overall cap');
  }
  return {overall, total};
}

function git(workspace, executable, args) {
  const result = spawnSync(executable, args, {
    cwd: workspace, encoding: 'utf8', windowsHide: true, timeout: 15000,
    env: withoutProviderCredential(),
  });
  if (result.error || result.status !== 0) {
    throw Error(`synthetic Git preparation failed: ${result.error?.message || result.stderr || result.status}`);
  }
}

function initializeWorkspace(workspace, gitExecutable) {
  for (const [relative, content] of Object.entries(SYNTHETIC)) {
    const target = path.join(workspace, relative);
    fs.mkdirSync(path.dirname(target), {recursive: true});
    writeExclusive(target, content);
  }
  git(workspace, gitExecutable, ['init', '--quiet']);
  git(workspace, gitExecutable, ['add', '--all']);
  git(workspace, gitExecutable, [
    '-c', 'user.name=VCP U06 fixture', '-c', 'user.email=u06@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'U06 synthetic base',
  ]);
}

function workspaceInputs(workspace) {
  return Object.fromEntries(filesUnder(workspace)
    .filter(relative => relative !== '.git' && !relative.startsWith(`.git${path.sep}`) && !relative.startsWith('.git/'))
    .map(relative => [relative.replaceAll('\\', '/'), sha(read(path.join(workspace, relative)))]));
}

function delegationSpec(base, gitExecutable, childCapMicros) {
  return {
    version: 1,
    git: gitExecutable,
    disposable_parent: path.join(base, 'children'),
    objective: 'Read observations/source.txt in the bounded synthetic U06 workspace and report observations without edits.',
    acceptance: ['Report observations from observations/source.txt and any limitations without modifying the workspace.'],
    mode: 'read_only',
    read_paths: [''],
    write_paths: [],
    untracked_inputs: [],
    allocation_usd: capUsd(childCapMicros),
    seconds: 120,
    required_checks: [],
  };
}

function prepare(specFile, destination) {
  const specBytes = read(specFile, 1024 * 1024);
  const spec = JSON.parse(specBytes);
  noSecrets(spec);
  const caps = validateSpec(spec);
  destination = plain(path.resolve(destination));
  if (fs.existsSync(destination) || within(repo, destination) || within(destination, repo)) {
    throw Error('new private destination outside repository required');
  }
  noParentInstructions(path.dirname(destination));

  // Qualify and freeze every source before creating a Git repository or process.
  const inputs = spec.cases.map(item => {
    const sources = {};
    for (const [name, limit] of Object.entries({
      executable: 1024 * 1024 * 1024,
      pty_driver: 1024 * 1024 * 1024,
      state_exporter: 1024 * 1024 * 1024,
      profile: 4 * 1024 * 1024,
      catalog: 4 * 1024 * 1024,
      task_file: 4 * 1024 * 1024,
      git: 1024 * 1024 * 1024,
    })) sources[name] = exactFile(item[name], limit);
    const profile = JSON.parse(sources.profile.bytes);
    noSecrets(profile);
    const reasons = profileReasons(profile);
    if (reasons.length) throw Error(`profile qualification failed for ${item.id}: ${reasons.join('; ')}`);
    if (typeof profile.catalog !== 'string'
        || plain(path.resolve(profile.catalog)) !== sources.catalog.path) {
      throw Error(`profile catalog does not match frozen catalog for ${item.id}`);
    }
    if (!sources.task_file.bytes.length || sources.task_file.bytes.includes(0)) {
      throw Error(`plain prompt file required for ${item.id}`);
    }
    return {item, sources, profile};
  });

  privateDirectory(destination);
  fs.mkdirSync(destination, {recursive: false, mode: 0o700});
  const plan = {
    schema: 'p7-delegation-terminal-plan/1', directory: destination,
    runner_sha256: sha(read(__filename)), spec_sha256: sha(specBytes),
    dependency_sha256: {
      builtin_live_runner: sha(read(require.resolve('./builtin-live-runner.cjs'))),
      p6_live_runner: sha(read(require.resolve('./p6-live-runner.cjs'))),
    },
    overall_cap_micros: caps.overall, allocated_cap_micros: caps.total,
    prior_exposure_micros: spec.prior_exposure_micros, cases: [],
  };
  for (const {item, sources, profile} of inputs) {
    const base = path.join(destination, item.id);
    const workspace = path.join(base, 'workspace');
    const dataDir = path.join(base, 'data');
    const children = path.join(base, 'children');
    fs.mkdirSync(workspace, {recursive: true, mode: 0o700});
    fs.mkdirSync(dataDir);
    fs.mkdirSync(children);
    initializeWorkspace(workspace, sources.git.path);
    const derivedProfile = {
      ...profile,
      workspace,
      catalog: sources.catalog.path,
      budget_usd: capUsd(micros(item.cap_usd)),
      affected_paths: Object.keys(SYNTHETIC),
    };
    writeExclusive(path.join(base, 'profile.json'), derivedProfile);
    writeExclusive(path.join(base, 'prompt.txt'), sources.task_file.bytes);
    const childSpec = delegationSpec(base, sources.git.path, micros(item.child_cap_usd));
    writeExclusive(path.join(base, 'delegation.json'), childSpec);
    const cliArguments = [
      '--workspace', workspace,
      '--data-dir', dataDir,
      '--config', path.join(base, 'profile.json'),
      'run', '--file', path.join(base, 'prompt.txt'), '--autonomy', 'plan',
    ];
    const driver = {executable: sources.executable.path, workspace, arguments: cliArguments};
    writeExclusive(path.join(base, 'driver-spec.json'), driver);
    const generated = {};
    for (const name of ['profile.json', 'prompt.txt', 'delegation.json', 'driver-spec.json']) {
      generated[name] = sha(read(path.join(base, name)));
    }
    plan.cases.push({
      id: item.id,
      cap_micros: micros(item.cap_usd),
      child_cap_micros: micros(item.child_cap_usd),
      source: Object.fromEntries(Object.entries(sources).map(([name, value]) => [name, value.path])),
      source_sha256: Object.fromEntries(Object.entries(sources).map(([name, value]) => [name, sha(value.bytes)])),
      frozen: {generated, workspace: workspaceInputs(workspace)},
      markers: {
        startup: item.startup_markers, child: item.child_markers,
        paused: item.paused_marker, resumed: item.resumed_marker,
      },
      data_dir: dataDir,
    });
  }
  writeExclusive(path.join(destination, 'plan.json'), plan);
  return {
    plan: path.join(destination, 'plan.json'),
    plan_sha256: sha(read(path.join(destination, 'plan.json'))),
    overall_cap_micros: caps.overall,
    allocated_cap_micros: caps.total,
    prior_exposure_micros: spec.prior_exposure_micros,
    model_calls: 0,
  };
}

function capUsd(microsValue) {
  return `${Math.floor(microsValue / 1000000)}.${String(microsValue % 1000000).padStart(6, '0')}`;
}

function expectedPrepared(plan, item) {
  const base = path.join(plan.directory, item.id);
  const workspace = path.join(base, 'workspace');
  const sourceProfile = JSON.parse(read(item.source.profile, 4 * 1024 * 1024));
  const derivedProfile = {
    ...sourceProfile,
    workspace,
    catalog: item.source.catalog,
    budget_usd: capUsd(item.cap_micros),
    affected_paths: Object.keys(SYNTHETIC),
  };
  const cliArguments = [
    '--workspace', workspace,
    '--data-dir', item.data_dir,
    '--config', path.join(base, 'profile.json'),
    'run', '--file', path.join(base, 'prompt.txt'), '--autonomy', 'plan',
  ];
  return {
    base, workspace, derivedProfile,
    childSpec: delegationSpec(base, item.source.git, item.child_cap_micros),
    driver: {executable: item.source.executable, workspace, arguments: cliArguments},
  };
}

function validatePlan(file, authorization) {
  const bytes = read(file, 8 * 1024 * 1024);
  const plan = JSON.parse(bytes);
  if (sha(bytes) !== authorization || plan.schema !== 'p7-delegation-terminal-plan/1'
      || plan.runner_sha256 !== sha(read(__filename))
      || plan.dependency_sha256?.builtin_live_runner !== sha(read(require.resolve('./builtin-live-runner.cjs')))
      || plan.dependency_sha256?.p6_live_runner !== sha(read(require.resolve('./p6-live-runner.cjs')))
      || path.resolve(path.dirname(file)) !== path.resolve(plan.directory)) {
    throw Error('plan identity or authorization mismatch');
  }
  const total = Array.isArray(plan.cases)
    ? plan.cases.reduce((sum, item) => sum + item.cap_micros, 0)
    : NaN;
  if (!Number.isSafeInteger(plan.overall_cap_micros) || plan.overall_cap_micros > 100000000
      || !Number.isSafeInteger(plan.prior_exposure_micros)
      || !Array.isArray(plan.cases) || plan.cases.length !== 2
      || plan.prior_exposure_micros + total > plan.overall_cap_micros
      || plan.allocated_cap_micros !== total) {
    throw Error('plan cap or case inventory invalid');
  }
  if (fs.existsSync(path.join(plan.directory, 'execution-claim.json'))
      || fs.existsSync(path.join(plan.directory, 'result.json'))) {
    throw Error('one-shot plan already claimed');
  }
  const ids = new Set();
  for (const item of plan.cases) {
    if (!CASES.has(item.id) || ids.has(item.id)) throw Error('prepared case inventory changed');
    ids.add(item.id);
    const expected = expectedPrepared(plan, item);
    for (const [name, digest] of Object.entries(item.source_sha256)) {
      if (!item.source[name] || sha(read(item.source[name], 1024 * 1024 * 1024)) !== digest) {
        throw Error(`frozen ${item.id} source changed: ${name}`);
      }
    }
    if (profileReasons(JSON.parse(read(item.source.profile, 4 * 1024 * 1024))).length
        || profileReasons(JSON.parse(read(path.join(expected.base, 'profile.json'), 4 * 1024 * 1024))).length
        || JSON.stringify(JSON.parse(read(path.join(expected.base, 'profile.json')))) !== JSON.stringify(expected.derivedProfile)) {
      throw Error(`profile qualification expired or changed for ${item.id}`);
    }
    for (const [name, digest] of Object.entries(item.frozen.generated)) {
      if (sha(read(path.join(expected.base, name), 4 * 1024 * 1024)) !== digest) {
        throw Error(`prepared ${item.id} input changed: ${name}`);
      }
    }
    const driver = JSON.parse(read(path.join(expected.base, 'driver-spec.json')));
    exactKeys(driver, ['executable', 'workspace', 'arguments'], 'PTY driver spec');
    if (JSON.stringify(driver) !== JSON.stringify(expected.driver)) throw Error(`PTY driver spec changed for ${item.id}`);
    const childSpec = JSON.parse(read(path.join(expected.base, 'delegation.json')));
    if (JSON.stringify(childSpec) !== JSON.stringify(expected.childSpec)) {
      throw Error(`delegation spec changed for ${item.id}`);
    }
    if (JSON.stringify(workspaceInputs(expected.workspace)) !== JSON.stringify(item.frozen.workspace)) {
      throw Error(`synthetic workspace changed for ${item.id}`);
    }
    if (filesUnder(item.data_dir).length || filesUnder(path.join(expected.base, 'children')).length) {
      throw Error(`prepared ${item.id} execution directories are not fresh`);
    }
  }
  return plan;
}

function invokeCanonical(executable, args, cwd, timeout = 15000) {
  const result = spawnSync(executable, args, {
    cwd, encoding: 'utf8', windowsHide: true, timeout, maxBuffer: 4 * 1024 * 1024,
    env: withoutProviderCredential(),
  });
  return {
    status: result.status,
    error: result.error ? String(result.error.message || result.error) : null,
    stdout: result.stdout || '', stderr: result.stderr || '',
  };
}

function withoutProviderCredential() {
  const env = {...process.env, RUST_MIN_STACK};
  for (const key of Object.keys(env)) {
    if (key.toUpperCase() === 'OPENROUTER_API_KEY') delete env[key];
  }
  return env;
}

function preflight(file, authorization, call = spawnSync) {
  const plan = validatePlan(file, authorization);
  const outputFile = path.join(plan.directory, 'preflight-result.json');
  if (fs.existsSync(outputFile)) throw Error('offline CLI preflight already attempted');
  const report = {
    schema: 'p7-delegation-terminal-preflight/1',
    plan_sha256: authorization,
    runner_sha256: plan.runner_sha256,
    status: 'passed',
    model_calls: 0,
    cases: [],
  };
  for (const item of plan.cases) {
    const base = path.join(plan.directory, item.id);
    const driver = JSON.parse(read(path.join(base, 'driver-spec.json')));
    const preflightData = path.join(base, 'preflight-data');
    fs.mkdirSync(preflightData, {recursive: false});
    const args = ['--format', 'jsonl', ...driver.arguments];
    args[args.indexOf('--data-dir') + 1] = preflightData;
    const execution = call(item.source.executable, args, {
      cwd: driver.workspace,
      encoding: 'utf8',
      windowsHide: true,
      timeout: 30000,
      maxBuffer: 4 * 1024 * 1024,
      env: withoutProviderCredential(),
    });
    const result = {
      id: item.id,
      status: execution.status,
      error: execution.error ? String(execution.error.message || execution.error) : null,
      stdout: execution.stdout || '',
      stderr: execution.stderr || '',
      profile_sha256: item.frozen.generated['profile.json'],
    };
    const frames = [];
    try {
      for (const line of result.stdout.trim().split(/\r?\n/).filter(Boolean)) frames.push(JSON.parse(line));
    } catch (_) { /* reported as a failed preflight below */ }
    const final = frames.findLast(frame => frame.type === 'result');
    const text = `${result.stdout}\n${result.stderr}`;
    const preflightFiles = filesUnder(preflightData);
    const inert = preflightFiles.length === 0 || (preflightFiles.length === 1
      && /^workspaces[\\/][0-9a-f]{64}[\\/]selection\.lock$/.test(preflightFiles[0])
      && fs.statSync(path.join(preflightData, preflightFiles[0])).size === 0);
    if (!preflightFiles.length) fs.rmdirSync(preflightData);
    result.preflight_files = preflightFiles;
    result.accepted_profile = execution.status === 2
      && !result.error
      && inert
      && text.includes('OPENROUTER_API_KEY is required')
      && !text.includes('profile resource or acceptance bounds rejected')
      && final?.scope == null
      && final?.exit_code === 2
      && final?.conditions?.invalid_configuration === true;
    if (!result.accepted_profile) report.status = 'failed';
    report.cases.push(result);
  }
  writeExclusive(outputFile, report);
  return report;
}

function requirePreflight(plan, authorization) {
  const file = path.join(plan.directory, 'preflight-result.json');
  if (!fs.existsSync(file)) throw Error('exact offline CLI preflight is required before run');
  const report = JSON.parse(read(file, 8 * 1024 * 1024));
  if (report.schema !== 'p7-delegation-terminal-preflight/1'
      || report.plan_sha256 !== authorization
      || report.runner_sha256 !== plan.runner_sha256
      || report.status !== 'passed'
      || report.model_calls !== 0
      || !Array.isArray(report.cases)
      || report.cases.length !== plan.cases.length
      || report.cases.some((row, index) => row.id !== plan.cases[index].id
        || row.profile_sha256 !== plan.cases[index].frozen.generated['profile.json']
        || row.accepted_profile !== true)) {
    throw Error('offline CLI preflight evidence is missing, failed, or changed');
  }
  return report;
}

function commandData(execution) {
  if (execution.error || execution.status !== 0) {
    throw Error(execution.error || execution.stderr || 'canonical command failed');
  }
  const frames = execution.stdout.trim().split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  const final = frames.findLast(frame => frame.type === 'result');
  if (!final || final.exit_code !== 0 || !final.data) throw Error('canonical command result missing');
  return final.data;
}

function discoverWorkspace(dataDir) {
  const directory = path.join(dataDir, 'workspaces');
  if (!fs.existsSync(directory)) return null;
  const found = [];
  for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
    if (!entry.isDirectory()) continue;
    const file = path.join(directory, entry.name, 'workspace.json');
    if (!fs.existsSync(file)) continue;
    try {
      const descriptor = JSON.parse(read(file, 1024 * 1024));
      const rootTaskId = descriptor?.config?.root_task;
      if (typeof rootTaskId === 'string' && rootTaskId) found.push({path: file, root_task_id: rootTaskId});
    } catch (_) { /* startup may still be publishing the descriptor */ }
  }
  return found.length === 1 ? found[0] : null;
}

function inspectAgents(item, driverSpec, discovered, call) {
  const execution = call(item.source.executable, [
    '--format', 'jsonl', '--workspace', driverSpec.workspace,
    '--data-dir', item.data_dir, 'tasks', 'agents', discovered.root_task_id, '--offset', '0',
  ], driverSpec.workspace);
  const data = commandData(execution);
  if (!Array.isArray(data.items) || data.items.length < 1) throw Error('canonical child page is empty');
  return {execution, data};
}

function positiveCost(value) {
  if (typeof value === 'number') return Number.isSafeInteger(value) && value > 0;
  return typeof value === 'string' && /^[1-9][0-9]*$/.test(value);
}

function childAttemptObserved(data) {
  return data.items.every(row => row.registration != null
    && ['known', 'reserved', 'uncertain'].some(field => positiveCost(row.cost?.[field])));
}

function verifyFrozenWorkspace(item, base) {
  try {
    const observed = workspaceInputs(path.join(base, 'workspace'));
    return {
      status: JSON.stringify(observed) === JSON.stringify(item.frozen.workspace) ? 'passed' : 'failed',
      expected: item.frozen.workspace,
      observed,
    };
  } catch (error) {
    return {
      status: 'failed', expected: item.frozen.workspace, observed: null,
      error: error.message || String(error),
    };
  }
}

function runCase(plan, item, base, operations = {}) {
  const spawnProcess = operations.spawn || spawn;
  const call = operations.invokeCanonical || invokeCanonical;
  const discover = operations.discoverWorkspace || discoverWorkspace;
  const maxRunMs = Math.min(operations.maxRunMs || MAX_RUN_MS, MAX_RUN_MS);
  const driverSpecPath = path.join(base, 'driver-spec.json');
  const driverSpec = JSON.parse(read(driverSpecPath));
  const transcript = [];
  const output = [];
  const startedAt = Date.now();
  const result = {
    id: item.id, status: 'failed', started_at: new Date().toISOString(),
    cap_micros: item.cap_micros, child_cap_micros: item.child_cap_micros,
    failures: [], canonical: [],
  };
  return new Promise(resolve => {
    let child;
    let rl;
    let deadline;
    let killTimer;
    let forceFinishTimer;
    let readinessTimer;
    let resumeTimer;
    let finished = false;
    let driverStarted = false;
    let sentDelegate = false;
    let childObserved = false;
    let sentPause = false;
    let pauseObserved = false;
    let parentDrained = false;
    let sentResume = false;
    let resumeAttempts = 0;
    let resumeOffset = 0;
    let rejectedAttempt = 0;
    let rejectionOffset = null;
    let resumeRejection = null;
    let resumeObserved = false;
    let sentExit = false;
    let exitOutputOffset = null;
    let terminationRequested = false;
    let structuredExit = false;
    let structuredExitCode = null;
    let discovered = null;
    let readinessStartedAt = null;
    let readinessPolls = 0;
    let readinessError = null;
    const stderr = [];
    let outputBytes = 0;

    const addFailure = message => {
      if (!result.failures.includes(message)) result.failures.push(message);
    };
    const control = value => {
      transcript.push({at_ms: Date.now() - startedAt, control: value});
      if (!child?.stdin?.writable) {
        addFailure('PTY driver control channel closed');
        return;
      }
      child.stdin.write(JSON.stringify(value) + '\n');
    };
    const terminate = reason => {
      if (reason) addFailure(reason);
      if (terminationRequested) return;
      terminationRequested = true;
      control({action: 'terminate'});
      killTimer = setTimeout(() => {
        if (!finished) {
          child.kill();
          forceFinishTimer = setTimeout(() => {
            finish(null, 'PTY driver did not exit after terminate control');
          }, 750);
        }
      }, Math.min(3000, Math.max(250, maxRunMs / 4)));
    };
    const send = text => control({action: 'write', text});
    const attemptResume = textLength => {
      if (resumeAttempts === 0) {
        resumeTimer = setTimeout(
          () => terminate('resume readiness deadline exceeded'),
          Math.min(MAX_RESUME_WAIT_MS, maxRunMs),
        );
      }
      resumeAttempts += 1;
      resumeOffset = textLength;
      sentResume = true;
      send('/resume\r');
    };
    const captureWorkspace = () => {
      discovered ||= discover(item.data_dir);
      if (!discovered) throw Error('workspace descriptor/root task not discovered');
      result.discovered = discovered;
    };
    const canonicalAgents = requirePaused => {
      captureWorkspace();
      const inspected = inspectAgents(item, driverSpec, discovered, call);
      result.canonical.push({
        kind: requirePaused ? 'agents_paused' : 'agents_started',
        ...inspected.execution, data: inspected.data,
      });
      if (inspected.data.items.some(row => row.registration == null)) {
        throw Error('child workspace registration missing');
      }
      if (requirePaused && inspected.data.items.some(row => row.state !== 'paused')) {
        throw Error('canonical child is not paused');
      }
      return inspected.data;
    };
    const beginTerminalAction = () => {
      if (item.id === 'pause-resume') {
        sentPause = true;
        send('/pause\r');
      } else {
        terminate(null);
      }
    };
    const pollAttemptReadiness = () => {
      if (finished || sentPause || terminationRequested) return;
      readinessPolls += 1;
      try {
        const data = canonicalAgents(false);
        if (childAttemptObserved(data)) {
          result.readiness = {
            polls: readinessPolls,
            elapsed_ms: Date.now() - readinessStartedAt,
            basis: 'canonical child cost is known, reserved, or uncertain',
          };
          beginTerminalAction();
          return;
        }
        readinessError = 'canonical child has no observed attempt cost';
        result.canonical.pop();
      } catch (error) {
        readinessError = error.message || String(error);
      }
      if (Date.now() - readinessStartedAt >= Math.min(20000, Math.max(1000, maxRunMs / 2))) {
        terminate(`child attempt readiness deadline exceeded: ${readinessError}`);
        return;
      }
      readinessTimer = setTimeout(pollAttemptReadiness, 100);
    };
    const finish = (code, error = null) => {
      if (finished) return;
      finished = true;
      clearTimeout(deadline);
      clearTimeout(killTimer);
      clearTimeout(forceFinishTimer);
      clearTimeout(readinessTimer);
      clearTimeout(resumeTimer);
      rl?.close();
      if (error) addFailure(error);
      const text = output.join('');
      if (!driverStarted) addFailure('PTY driver did not emit started');
      if (!markerSeen(text, item.markers.startup)) addFailure('startup marker not observed');
      if (!sentDelegate || !childObserved) addFailure('child start was not observed');
      if (item.id === 'pause-resume'
          && (!sentPause || !pauseObserved || !parentDrained
            || !sentResume || !resumeObserved || !sentExit)) {
        addFailure('pause, canonical paused inspection, parent drain, resume acknowledgement, and exit are required');
      }
      if (item.id === 'hard-close-reopen' && !terminationRequested) {
        addFailure('hard close was not requested');
      }
      if (item.id === 'pause-resume') {
        const terminal = exitOutputOffset == null ? null : finalTerminalResult(text, exitOutputOffset);
        result.final_terminal_result = terminal;
        const reason = terminalResultReason(terminal, structuredExitCode, discovered?.root_task_id);
        if (!structuredExit || reason) {
          addFailure(reason || 'pause-resume requires a structured terminal exit');
        }
      }
      if (item.id === 'hard-close-reopen'
          && (!structuredExit || !Number.isInteger(structuredExitCode) || structuredExitCode === 0)) {
        addFailure('hard-close requires a structured nonzero exit');
      }
      result.structured_exit = structuredExit;
      result.exit_code = code;
      result.stderr = stderr.join('');
      result.transcript = transcript;
      result.output = output;
      result.output_bytes = outputBytes;
      result.ended_at = new Date().toISOString();
      result.duration_ms = Date.now() - startedAt;
      result.status = result.failures.length ? 'failed' : 'transport_observed';
      resolve(result);
    };

    try {
      child = spawnProcess(item.source.pty_driver, [driverSpecPath], {
        cwd: base, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
        env: {...process.env, RUST_MIN_STACK},
      });
    } catch (error) {
      finish(null, `PTY driver spawn failed: ${error.message || error}`);
      return;
    }
    rl = readline.createInterface({input: child.stdout});
    child.stderr.on('data', chunk => stderr.push(chunk.toString('utf8')));
    child.stdin.on('error', error => {
      if (!finished) addFailure(`PTY driver control error: ${error.message || error}`);
    });
    child.stdout.on('error', error => {
      if (!finished) terminate(`PTY driver output error: ${error.message || error}`);
    });
    child.stderr.on('error', error => {
      if (!finished) addFailure(`PTY driver stderr error: ${error.message || error}`);
    });
    deadline = setTimeout(() => terminate('case deadline exceeded'), maxRunMs);
    rl.on('line', line => {
      let event;
      try { event = JSON.parse(line); } catch (_) { addFailure('PTY driver emitted non-JSON'); return; }
      transcript.push({at_ms: Date.now() - startedAt, event});
      if (event.type === 'started') {
        driverStarted = true;
        return;
      }
      if (event.type === 'exit') {
        structuredExit = true;
        structuredExitCode = event.code;
        return;
      }
      if (event.type !== 'output' || typeof event.text !== 'string') {
        addFailure('PTY driver emitted an unknown event');
        return;
      }
      outputBytes += Buffer.byteLength(event.text);
      if (outputBytes > 16 * 1024 * 1024) {
        terminate('output byte cap exceeded');
        return;
      }
      output.push(event.text);
      const text = output.join('');
      try {
        if (driverStarted && !sentDelegate && markerSeen(text, item.markers.startup)) {
          send(`/agents delegate "${path.join(base, 'delegation.json').replaceAll('"', '\\"')}"\r`);
          sentDelegate = true;
        }
        if (sentDelegate && !childObserved && markerSeen(text, item.markers.child)) {
          childObserved = true;
          captureWorkspace();
          readinessStartedAt = Date.now();
          pollAttemptReadiness();
        }
        if (item.id === 'pause-resume' && sentPause && !pauseObserved
            && text.includes(item.markers.paused)) {
          canonicalAgents(true);
          pauseObserved = true;
        }
        if (item.id === 'pause-resume' && pauseObserved && !parentDrained
            && (text.includes(PARENT_DRAINED) || text.includes(PARENT_COMPLETED))) {
          parentDrained = true;
        }
        if (item.id === 'pause-resume' && pauseObserved && resumeAttempts === 0) {
          attemptResume(text.length);
        }
        if (item.id === 'pause-resume' && resumeAttempts > rejectedAttempt && !resumeObserved) {
          const anyRejection = text.indexOf('Command rejected:', resumeOffset);
          const drainIndex = text.indexOf(RESUME_DRAIN_REJECTION, resumeOffset);
          const quiescenceIndex = text.indexOf(RESUME_QUIESCENCE_REJECTION, resumeOffset);
          const transientIndex = drainIndex >= resumeOffset ? drainIndex : quiescenceIndex;
          if (anyRejection >= resumeOffset && anyRejection !== transientIndex) {
            terminate('resume rejected for a reason other than bounded transient readiness');
            return;
          }
          if (transientIndex >= resumeOffset) {
            rejectedAttempt = resumeAttempts;
            rejectionOffset = transientIndex;
            resumeRejection = transientIndex === drainIndex ? 'drain' : 'quiescence';
            canonicalAgents(true);
            if (resumeAttempts >= MAX_RESUME_ATTEMPTS) {
              terminate('resume rejected after bounded readiness retries');
              return;
            }
            if (resumeRejection === 'quiescence') attemptResume(text.length);
          }
        }
        if (item.id === 'pause-resume' && resumeRejection === 'drain'
            && rejectedAttempt === resumeAttempts) {
          const drainedAt = Math.max(text.lastIndexOf(PARENT_DRAINED), text.lastIndexOf(PARENT_COMPLETED));
          if (drainedAt > rejectionOffset) {
            parentDrained = true;
            resumeRejection = null;
            attemptResume(text.length);
          }
        }
        if (item.id === 'pause-resume' && sentResume && !resumeObserved
            && text.includes(item.markers.resumed)) {
          resumeObserved = true;
          parentDrained = true;
          clearTimeout(resumeTimer);
          exitOutputOffset = text.length;
          send('/exit\r');
          sentExit = true;
        }
      } catch (error) {
        terminate(error.message || String(error));
      }
    });
    child.on('error', error => finish(null, `PTY driver error: ${error.message || error}`));
    child.on('close', code => {
      if (!finished) finish(
        structuredExit ? structuredExitCode : code,
        structuredExit ? null : 'PTY driver closed without structured exit',
      );
    });
  });
}

function snapshotProjection(snapshot) {
  const value = typeof snapshot === 'string'
    ? JSON.parse(read(snapshot, 32 * 1024 * 1024))
    : snapshot;
  const records = Object.values(value.records || value.state?.records || {});
  const named = name => records.filter(row => String(row.collection).toLowerCase() === name);
  const attempts = named('attempt').map(row => ({
    id: row.id, task: row.value?.scope?.task, phase: row.value?.phase,
    charged: row.value?.charged, uncertain: row.value?.uncertain ?? null,
  })).sort((a, b) => String(a.id).localeCompare(String(b.id)));
  const tasks = named('task').map(row => ({
    id: row.id, state: row.value?.state, root: row.value?.root,
  })).sort((a, b) => String(a.id).localeCompare(String(b.id)));
  const ledgers = named('ledger').map(row => ({
    id: row.id, cap: row.value?.cap, settled: row.value?.settled,
    active: row.value?.active, unresolved: row.value?.unresolved,
  })).sort((a, b) => String(a.id).localeCompare(String(b.id)));
  return {attempts, tasks, ledgers};
}

function decimalMicros(value, name) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value)) {
    throw Error(`invalid canonical ${name}`);
  }
  const number = Number(value);
  if (!Number.isSafeInteger(number)) throw Error(`unbounded canonical ${name}`);
  return number;
}

function gradeSnapshots(first, second, item, rootTaskId) {
  const left = snapshotProjection(first);
  const right = snapshotProjection(second);
  const failures = [];
  if (JSON.stringify(left) !== JSON.stringify(right)) {
    failures.push('attempt IDs/phases, task states, or ledger counters changed across recovery exports');
  }
  if (right.attempts.length < 2) failures.push('main and child attempts were not both retained');
  const childTasks = right.tasks.filter(task => task.id !== rootTaskId);
  if (right.tasks.length < 2 || !childTasks.length) {
    failures.push('canonical child task was not retained');
  }
  if (right.tasks.some(task => task.state !== 'paused')) {
    failures.push('recovery did not leave the root and child tasks paused');
  }
  if (!right.attempts.some(attempt => attempt.task === rootTaskId)
      || !right.attempts.some(attempt => childTasks.some(task => task.id === attempt.task))) {
    failures.push('retained attempts do not include both the main task and a child task');
  }
  const rootLedgers = right.ledgers.filter(row => row.id === rootTaskId);
  const ledger = rootLedgers.length === 1 ? rootLedgers[0] : null;
  let accounting = null;
  if (!ledger) {
    failures.push('exact root ledger missing or ambiguous in recovery snapshot');
  } else {
    try {
      const cap = decimalMicros(ledger.cap, 'cap');
      const settled = decimalMicros(ledger.settled, 'settled');
      const active = decimalMicros(ledger.active, 'active');
      const unresolved = decimalMicros(ledger.unresolved, 'unresolved');
      const charged = right.attempts.reduce(
        (sum, attempt) => sum + decimalMicros(attempt.charged, 'attempt charge'), 0,
      );
      accounting = {
        cap_micros: cap, settled_micros: settled, active_micros: active,
        unresolved_micros: unresolved, charged_micros: charged,
      };
      if (cap !== item.cap_micros) failures.push('canonical root ledger cap differs from frozen case cap');
      if (active !== 0) failures.push('canonical root ledger retains active liability after recovery');
      if (settled + unresolved > item.cap_micros) {
        failures.push('settled plus unresolved liability exceeds case cap');
      }
      if (charged !== settled) failures.push('attempt charges do not match canonical settled ledger cost');
    } catch (error) {
      failures.push(error.message || String(error));
    }
  }
  return {failures, projection: right, accounting};
}

function modelCallCount(results) {
  return results.every(item => Number.isSafeInteger(item.actual_cost_micros))
    ? results.reduce((sum, item) => sum + item.final_projection.attempts.length, 0)
    : null;
}

async function run(file, authorization) {
  const plan = validatePlan(file, authorization);
  requirePreflight(plan, authorization);
  writeExclusive(path.join(plan.directory, 'execution-claim.json'), {
    schema: 'p7-delegation-terminal-claim/1', plan_sha256: authorization,
    overall_cap_micros: plan.overall_cap_micros,
    prior_exposure_micros: plan.prior_exposure_micros,
    case_caps_micros: Object.fromEntries(plan.cases.map(item => [item.id, item.cap_micros])),
    meaning: 'One shot. Every failed case retains its complete cap; never replay or recycle.',
  });
  const output = [];
  for (const item of plan.cases) {
    const base = path.join(plan.directory, item.id);
    const result = await runCase(plan, item, base);
    result.discovered ||= discoverWorkspace(item.data_dir);
    const exportA = path.join(base, 'snapshot-a.json');
    const exportB = path.join(base, 'snapshot-b.json');
    const unavailable = {
      status: null, error: 'workspace descriptor not discovered', stdout: '', stderr: '',
    };
    const first = result.discovered
      ? invokeCanonical(item.source.state_exporter, [result.discovered.path, exportA], base, 30000)
      : unavailable;
    const second = result.discovered && first.status === 0 && !first.error
      ? invokeCanonical(item.source.state_exporter, [result.discovered.path, exportB], base, 30000)
      : unavailable;
    result.exports = [first, second];
    if (first.status !== 0 || second.status !== 0 || first.error || second.error) {
      result.failures.push('canonical recovery export failed');
      result.cost_status = 'unknown';
      result.actual_cost_micros = null;
      result.unresolved_upper_bound_micros = item.cap_micros;
    } else {
      try {
        const grade = gradeSnapshots(exportA, exportB, item, result.discovered.root_task_id);
        result.failures.push(...grade.failures);
        result.final_projection = grade.projection;
        result.accounting = grade.accounting;
        result.cost_status = grade.accounting ? 'canonical' : 'unknown';
        result.actual_cost_micros = grade.accounting?.settled_micros ?? null;
        result.unresolved_upper_bound_micros = grade.accounting?.unresolved_micros ?? item.cap_micros;
      } catch (error) {
        result.failures.push(`canonical recovery snapshots invalid: ${error.message || error}`);
        result.cost_status = 'unknown';
        result.actual_cost_micros = null;
        result.unresolved_upper_bound_micros = item.cap_micros;
      }
    }
    result.workspace_verification = verifyFrozenWorkspace(item, base);
    if (result.workspace_verification.status !== 'passed') {
      result.failures.push('post-run workspace differs from frozen synthetic inputs');
    }
    result.held_upper_bound_micros = result.failures.length
      ? item.cap_micros
      : result.actual_cost_micros + result.unresolved_upper_bound_micros;
    result.status = result.failures.length ? 'failed' : 'observed';
    writeExclusive(path.join(base, 'result.json'), result);
    output.push(result);
  }
  const allCanonical = output.every(item => Number.isSafeInteger(item.actual_cost_micros));
  const report = {
    schema: 'p7-delegation-terminal-result/1', plan_sha256: authorization,
    status: output.every(item => item.status === 'observed') ? 'observed' : 'failed',
    cases: output,
    model_calls: modelCallCount(output),
    actual_cost_micros: allCanonical
      ? output.reduce((sum, item) => sum + item.actual_cost_micros, 0)
      : null,
    held_upper_bound_micros: output.reduce((sum, item) => sum + item.held_upper_bound_micros, 0),
    prior_exposure_micros: plan.prior_exposure_micros,
    overall_cap_micros: plan.overall_cap_micros,
    limitations: [
      'Observed terminal behavior and canonical recovery evidence require owner review before release qualification.',
    ],
  };
  if (report.prior_exposure_micros + report.held_upper_bound_micros > report.overall_cap_micros) {
    throw Error('reported held liability exceeds frozen overall cap');
  }
  writeExclusive(path.join(plan.directory, 'result.json'), report);
  return report;
}

function parseArgs(argv) {
  const [command, file, extra, ...rest] = argv;
  if (rest.length || !command || !file || !extra || !['prepare', 'preflight', 'run'].includes(command)) {
    throw Error('Usage: delegation-terminal-runner.cjs prepare <spec.json> <new-private-dir> | preflight|run <plan.json> <authorized-plan-sha256>');
  }
  return {command, file, extra};
}

module.exports = {
  micros, profileReasons, validateSpec, prepare, validatePlan, discoverWorkspace,
  preflight, requirePreflight, finalTerminalResult, terminalResultReason,
  childAttemptObserved, runCase, snapshotProjection, gradeSnapshots,
  verifyFrozenWorkspace, modelCallCount, run,
};

if (require.main === module) {
  (async () => {
    try {
      const args = parseArgs(process.argv.slice(2));
      const value = args.command === 'prepare'
        ? prepare(args.file, args.extra)
        : args.command === 'preflight'
          ? preflight(args.file, args.extra)
          : await run(args.file, args.extra);
      console.log(JSON.stringify(value, null, 2));
      if (args.command !== 'prepare' && !['passed', 'observed'].includes(value.status)) process.exitCode = 1;
    } catch (error) {
      console.error(error.message || String(error));
      process.exitCode = 1;
    }
  })();
}
