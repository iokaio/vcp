// SPDX-License-Identifier: Apache-2.0
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const {EventEmitter} = require('node:events');
const {PassThrough, Writable} = require('node:stream');
const runner = require('../../../scripts/evals/delegation-terminal-runner.cjs');

function gitExecutable() {
  if (process.env.VCP_TEST_GIT && fs.existsSync(process.env.VCP_TEST_GIT)) {
    return process.env.VCP_TEST_GIT;
  }
  const found = spawnSync('where.exe', ['git'], {encoding: 'utf8'});
  return found.status === 0 ? found.stdout.split(/\r?\n/).find(Boolean) : null;
}

function fixedProfile(catalog) {
  const validUntil = String(Date.now() + 60 * 60 * 1000);
  return {
    version: 1,
    trust_workspace: true,
    maximum_autonomy: 'plan',
    automatic_effects: [],
    processes: [],
    checks: [],
    mcp: [],
    mcp_http: [],
    max_requests: 2,
    max_transport_retries: 0,
    deadline_seconds: 120,
    output_tokens: '128',
    catalog,
    provider: {
      valid_until: validUntil,
      max_output: '128',
      price: {currency: 'USD', valid_until: validUntil},
      compatibility: {
        valid_until: validUntil,
        responses_text_tools: true,
        provider_preferences_qualified: true,
      },
    },
  };
}

function caseSpec(id, files) {
  return {
    id,
    executable: files.executable,
    pty_driver: files.driver,
    state_exporter: files.exporter,
    profile: files.profile,
    catalog: files.catalog,
    task_file: files.prompt,
    git: files.git,
    cap_usd: '1.000000',
    child_cap_usd: '0.250000',
    startup_markers: ['U06-READY'],
    child_markers: ['U06-CHILD-STARTED'],
    paused_marker: id === 'pause-resume' ? 'U06-PAUSED-ACK' : null,
    resumed_marker: id === 'pause-resume' ? 'U06-RESUMED-ACK' : null,
  };
}

test('terminal spec requires the exact pair and bounds prior plus case exposure', () => {
  const empty = {
    schema: 'p7-delegation-terminal-spec/1', overall_cap_usd: '100.000000',
    prior_exposure_micros: 0, cases: [],
  };
  assert.throws(() => runner.validateSpec(empty), /exactly two/);
  assert.throws(() => runner.micros('100.0000001'), /exact decimal/);
  const files = Object.fromEntries(
    ['executable', 'driver', 'exporter', 'profile', 'catalog', 'prompt', 'git']
      .map(name => [name, `C:\\u06\\${name}`]),
  );
  const cases = [caseSpec('pause-resume', files), caseSpec('hard-close-reopen', files)];
  assert.throws(() => runner.validateSpec({
    ...empty, overall_cap_usd: '2.000000', prior_exposure_micros: 1, cases,
  }), /exceeds overall/);
});

test('profile gate is the exact fixed-provider gate plus read-only plan authority', () => {
  const profile = fixedProfile('C:\\u06\\catalog.json');
  assert.deepEqual(runner.profileReasons(profile), []);
  assert.match(runner.profileReasons({...profile, routing: {enabled: false}}).join('; '), /fixed qualified provider/i);
  assert.match(runner.profileReasons({...profile, maximum_autonomy: 'workspace'}).join('; '), /read-only plan/i);
  assert.match(runner.profileReasons({...profile, qualification_endpoint: 'http://127.0.0.1'}).join('; '), /endpoint overrides/i);
});

test('preparation creates fresh Git workspaces and freezes exact PTY controls before claim', {
  skip: !gitExecutable(),
}, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-u06-'));
  try {
    const files = {
      executable: path.join(root, 'vcp.exe'),
      driver: path.join(root, 'driver.exe'),
      exporter: path.join(root, 'exporter.exe'),
      profile: path.join(root, 'profile.json'),
      catalog: path.join(root, 'catalog.json'),
      prompt: path.join(root, 'prompt.txt'),
      git: gitExecutable(),
    };
    for (const name of ['executable', 'driver', 'exporter']) fs.writeFileSync(files[name], name);
    fs.writeFileSync(files.catalog, '{}\n');
    fs.writeFileSync(files.prompt, 'Inspect the synthetic workspace while a child review runs.\n');
    fs.writeFileSync(files.profile, JSON.stringify(fixedProfile(files.catalog)));
    const specFile = path.join(root, 'spec.json');
    fs.writeFileSync(specFile, JSON.stringify({
      schema: 'p7-delegation-terminal-spec/1',
      overall_cap_usd: '2.500000',
      prior_exposure_micros: 500000,
      cases: [caseSpec('pause-resume', files), caseSpec('hard-close-reopen', files)],
    }));
    const destination = path.join(root, 'trial');
    const prepared = runner.prepare(specFile, destination);
    assert.equal(prepared.overall_cap_micros, 2500000);
    assert.equal(prepared.allocated_cap_micros, 2000000);
    assert.equal(prepared.prior_exposure_micros, 500000);
    const plan = runner.validatePlan(prepared.plan, prepared.plan_sha256);
    assert.equal(plan.cases.length, 2);
    assert.throws(
      () => runner.requirePreflight(plan, prepared.plan_sha256),
      /preflight is required/,
    );
    let preflightCalls = 0;
    const preflight = runner.preflight(prepared.plan, prepared.plan_sha256, (executable, args, options) => {
      preflightCalls += 1;
      assert.equal(executable, files.executable);
      assert.equal(args[0], '--format');
      assert.equal(args[1], 'jsonl');
      assert.match(args[args.indexOf('--data-dir') + 1], /preflight-data$/);
      assert.equal(Object.keys(options.env).some(key => key.toUpperCase() === 'OPENROUTER_API_KEY'), false);
      return {
        status: 2, error: null,
        stdout: `${JSON.stringify({
          type: 'result', scope: null, exit_code: 2,
          conditions: {invalid_configuration: true},
        })}\n`,
        stderr: 'vcp: OPENROUTER_API_KEY is required\n',
      };
    });
    assert.equal(preflight.status, 'passed');
    assert.equal(preflightCalls, 2);
    assert.equal(runner.requirePreflight(plan, prepared.plan_sha256).status, 'passed');
    const base = path.join(destination, 'pause-resume');
    const driver = JSON.parse(fs.readFileSync(path.join(base, 'driver-spec.json')));
    assert.deepEqual(Object.keys(driver).sort(), ['arguments', 'executable', 'workspace']);
    assert.deepEqual(driver.arguments.slice(0, 2), ['--workspace', path.join(base, 'workspace')]);
    assert.equal(driver.arguments[driver.arguments.indexOf('--file') + 1], path.join(base, 'prompt.txt'));
    assert.equal(fs.readFileSync(path.join(base, 'prompt.txt'), 'utf8'), fs.readFileSync(files.prompt, 'utf8'));
    const derivedProfile = JSON.parse(fs.readFileSync(path.join(base, 'profile.json')));
    assert.deepEqual(derivedProfile.affected_paths, [
      'README.synthetic.txt', 'observations/source.txt',
    ]);
    const child = JSON.parse(fs.readFileSync(path.join(base, 'delegation.json')));
    for (const field of [
      'git', 'disposable_parent', 'objective', 'acceptance', 'mode', 'read_paths',
      'write_paths', 'untracked_inputs', 'allocation_usd', 'seconds',
    ]) assert.ok(Object.hasOwn(child, field), field);
    assert.deepEqual(child.read_paths, ['']);
    assert.equal(runner.verifyFrozenWorkspace(plan.cases[0], base).status, 'passed');
    const unexpected = path.join(base, 'workspace', 'unexpected.txt');
    fs.writeFileSync(unexpected, 'post-run mutation');
    assert.equal(runner.verifyFrozenWorkspace(plan.cases[0], base).status, 'failed');
    fs.rmSync(unexpected);
    assert.equal(spawnSync(files.git, ['status', '--porcelain'], {
      cwd: path.join(base, 'workspace'), encoding: 'utf8',
    }).stdout, '');
    const claim = path.join(destination, 'execution-claim.json');
    fs.writeFileSync(claim, '{}');
    assert.throws(() => runner.validatePlan(prepared.plan, prepared.plan_sha256), /already claimed/);
    fs.rmSync(claim);
    fs.writeFileSync(path.join(base, 'prompt.txt'), 'changed');
    assert.throws(() => runner.validatePlan(prepared.plan, prepared.plan_sha256), /input changed/);
  } finally {
    fs.rmSync(root, {recursive: true, force: true});
  }
});

test('workspace discovery requires one durable descriptor and exposes its root task ID', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-u06-discovery-'));
  try {
    const data = path.join(root, 'data');
    const directory = path.join(data, 'workspaces', 'one');
    fs.mkdirSync(directory, {recursive: true});
    const descriptor = path.join(directory, 'workspace.json');
    fs.writeFileSync(descriptor, JSON.stringify({config: {root_task: 'root-task'}}));
    assert.deepEqual(runner.discoverWorkspace(data), {
      path: descriptor, root_task_id: 'root-task',
    });
    const second = path.join(data, 'workspaces', 'two');
    fs.mkdirSync(second);
    fs.writeFileSync(path.join(second, 'workspace.json'), JSON.stringify({config: {root_task: 'other'}}));
    assert.equal(runner.discoverWorkspace(data), null);
  } finally {
    fs.rmSync(root, {recursive: true, force: true});
  }
});

function snapshot({phase = 'released', childState = 'paused', active = '0', settled = '5', unresolved = '10'} = {}) {
  const row = (collection, id, value, revision) => ({collection, id, revision, value});
  return {records: {
    root: row('task', 'root', {state: 'paused', root: 'root'}, 1),
    child: row('task', 'child', {state: childState, root: 'root'}, 2),
    mainAttempt: row('attempt', 'attempt-main', {
      scope: {task: 'root'}, phase, charged: '3', uncertain: null,
    }, 3),
    childAttempt: row('attempt', 'attempt-child', {
      scope: {task: 'child'}, phase, charged: '2', uncertain: null,
    }, 4),
    ledger: row('ledger', 'root', {
      cap: '1000000', settled, active, unresolved,
    }, 5),
  }};
}

test('snapshot grading ignores reopen revisions but requires stable tasks, attempts, and exact accounting', () => {
  const first = snapshot();
  const second = structuredClone(first);
  for (const record of Object.values(second.records)) record.revision += 100;
  const observed = runner.gradeSnapshots(first, second, {cap_micros: 1000000}, 'root');
  assert.deepEqual(observed.failures, []);
  assert.deepEqual(observed.accounting, {
    cap_micros: 1000000, settled_micros: 5, active_micros: 0,
    unresolved_micros: 10, charged_micros: 5,
  });
  const changed = runner.gradeSnapshots(first, snapshot({phase: 'reconciliation_pending'}), {cap_micros: 1000000}, 'root');
  assert.match(changed.failures.join('; '), /changed across recovery/);
  const active = runner.gradeSnapshots(first, snapshot({active: '10'}), {cap_micros: 1000000}, 'root');
  assert.match(active.failures.join('; '), /active liability/);
  const invented = runner.gradeSnapshots(first, snapshot({settled: '6'}), {cap_micros: 1000000}, 'root');
  assert.match(invented.failures.join('; '), /charges do not match/);
  const wrongLedger = snapshot();
  wrongLedger.records.ledger.id = 'not-root';
  assert.match(
    runner.gradeSnapshots(first, wrongLedger, {cap_micros: 1000000}, 'root').failures.join('; '),
    /exact root ledger missing/,
  );
  assert.equal(runner.modelCallCount([
    {actual_cost_micros: null, final_projection: {attempts: [{id: 'partial'}]}},
  ]), null);
  assert.equal(runner.modelCallCount([
    {actual_cost_micros: 5, final_projection: {attempts: [{id: 'main'}, {id: 'child'}]}},
  ]), 2);
});

test('runCase spawns the frozen PTY driver and completes the pause protocol with a mock transport', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-u06-mock-'));
  try {
    fs.writeFileSync(path.join(root, 'driver-spec.json'), JSON.stringify({
      executable: 'synthetic-cli.exe', workspace: root, arguments: ['synthetic-argument'],
    }));
    fs.writeFileSync(path.join(root, 'delegation.json'), '{}');
    let spawned = false;
    let parentDrained = false;
    let resumedBeforeDrain = false;
    const spawn = (executable, args, options) => {
      spawned = true;
      assert.equal(executable, 'frozen-driver.exe');
      assert.deepEqual(args, [path.join(root, 'driver-spec.json')]);
      assert.equal(options.env.RUST_MIN_STACK, '16777216');
      const child = new EventEmitter();
      child.stdout = new PassThrough();
      child.stderr = new PassThrough();
      child.kill = () => {};
      child.stdin = new Writable({
        write(bytes, _encoding, done) {
          const control = JSON.parse(bytes.toString('utf8'));
          if (control.action === 'write' && control.text.includes('/agents delegate')) {
            child.stdout.write(`${JSON.stringify({type: 'output', text: 'U06-CHILD-STARTED\n'})}\n`);
          } else if (control.action === 'write' && control.text.includes('/pause')) {
            child.stdout.write(`${JSON.stringify({type: 'output', text: 'U06-PAUSED-ACK\n'})}\n`);
            setImmediate(() => {
              parentDrained = true;
              child.stdout.write(`${JSON.stringify({
                type: 'output',
                text: 'Parent turn interrupted; inspect current state before explicit /resume.\n',
              })}\n`);
            });
          } else if (control.action === 'write' && control.text.includes('/resume')) {
            resumedBeforeDrain = !parentDrained;
            child.stdout.write(`${JSON.stringify({type: 'output', text: 'U06-RESUMED-ACK\n'})}\n`);
          } else if (control.action === 'write' && control.text.includes('/exit')) {
            child.stdout.write(`${JSON.stringify({type: 'exit', code: 0})}\n`);
            setImmediate(() => child.emit('close', 0));
          }
          done();
        },
      });
      process.nextTick(() => {
        child.stdout.write(`${JSON.stringify({type: 'started'})}\n`);
        child.stdout.write(`${JSON.stringify({type: 'output', text: 'U06-READY\n'})}\n`);
      });
      return child;
    };
    const item = {
      id: 'pause-resume', cap_micros: 1000000, child_cap_micros: 250000,
      data_dir: path.join(root, 'data'),
      source: {pty_driver: 'frozen-driver.exe', executable: 'synthetic-cli.exe'},
      markers: {
        startup: ['U06-READY'], child: ['U06-CHILD-STARTED'],
        paused: 'U06-PAUSED-ACK', resumed: 'U06-RESUMED-ACK',
      },
    };
    let canonicalCalls = 0;
    const result = await runner.runCase({}, item, root, {
      spawn,
      discoverWorkspace: () => ({path: 'workspace.json', root_task_id: 'root'}),
      invokeCanonical: () => {
        canonicalCalls += 1;
        return {
          status: 0, error: null, stderr: '',
          stdout: `${JSON.stringify({
            type: 'result', exit_code: 0,
            data: {items: [{
              state: 'paused', registration: {workspace: root},
              cost: {known: 0, reserved: canonicalCalls === 1 ? 0 : 1, uncertain: 0},
            }]},
          })}\n`,
        };
      },
      maxRunMs: 2000,
    });
    assert.equal(spawned, true);
    assert.ok(canonicalCalls >= 3);
    assert.equal(result.readiness.polls, 2);
    assert.equal(resumedBeforeDrain, false);
    assert.deepEqual(result.failures, []);
    assert.equal(result.status, 'transport_observed');
  } finally {
    fs.rmSync(root, {recursive: true, force: true});
  }
});

test('runCase sends terminate first and returns a bounded failure when a driver never exits', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-u06-timeout-'));
  try {
    fs.writeFileSync(path.join(root, 'driver-spec.json'), JSON.stringify({
      executable: 'synthetic-cli.exe', workspace: root, arguments: [],
    }));
    const controls = [];
    const spawn = () => {
      const child = new EventEmitter();
      child.stdout = new PassThrough();
      child.stderr = new PassThrough();
      child.stdin = new Writable({
        write(bytes, _encoding, done) {
          controls.push(JSON.parse(bytes.toString('utf8')));
          done();
        },
      });
      child.kill = () => {};
      process.nextTick(() => child.stdout.write(`${JSON.stringify({type: 'started'})}\n`));
      return child;
    };
    const started = Date.now();
    const result = await runner.runCase({}, {
      id: 'hard-close-reopen', cap_micros: 1000000, child_cap_micros: 250000,
      data_dir: path.join(root, 'data'),
      source: {pty_driver: 'frozen-driver.exe', executable: 'synthetic-cli.exe'},
      markers: {startup: ['never'], child: ['never'], paused: null, resumed: null},
    }, root, {spawn, maxRunMs: 100});
    assert.ok(Date.now() - started < 2000);
    assert.equal(controls[0].action, 'terminate');
    assert.equal(result.status, 'failed');
    assert.match(result.failures.join('; '), /deadline exceeded/);
    assert.match(result.failures.join('; '), /did not exit/);
  } finally {
    fs.rmSync(root, {recursive: true, force: true});
  }
});

const packagedDriver = process.env.VCP_DELEGATION_PTY_DRIVER
  || path.resolve(__dirname, '../../../artifacts/p7-owner-native-package-v2/delegation-pty-driver.exe');

test('runCase uses the actual PTY driver for pause/resume and hard-close transports', {
  skip: process.platform !== 'win32' || !fs.existsSync(packagedDriver),
  timeout: 30000,
}, async t => {
  for (const id of ['pause-resume', 'hard-close-reopen']) {
    await t.test(id, async () => {
      const root = fs.mkdtempSync(path.join(os.tmpdir(), `vcp-u06-pty-${id}-`));
      try {
        const script = path.join(root, 'synthetic-cli.cjs');
        fs.writeFileSync(script, [
          "'use strict';",
          "process.stdout.write('U06-READY\\n');",
          'let input = ""; let delegated = false; let paused = false; let resumed = false;',
          "process.stdin.on('data', bytes => {",
          " input += bytes.toString('utf8');",
          " if (!delegated && input.includes('/agents delegate')) { delegated = true; process.stdout.write('U06-CHILD-STARTED\\n'); }",
          " if (!paused && input.includes('/pause')) { paused = true; process.stdout.write('U06-PAUSED-ACK\\nParent turn interrupted; inspect current state before explicit /resume.\\n'); }",
          " if (!resumed && input.includes('/resume')) { resumed = true; process.stdout.write('U06-RESUMED-ACK\\n'); }",
          " if (input.includes('/exit')) process.exit(0);",
          '});',
          'setInterval(() => {}, 1000);',
        ].join('\n'));
        fs.writeFileSync(path.join(root, 'delegation.json'), '{}');
        fs.writeFileSync(path.join(root, 'driver-spec.json'), JSON.stringify({
          executable: process.execPath, workspace: root, arguments: [script],
        }));
        const item = {
          id, cap_micros: 1000000, child_cap_micros: 250000,
          data_dir: path.join(root, 'data'),
          source: {pty_driver: packagedDriver, executable: process.execPath},
          markers: {
            startup: ['U06-READY'], child: ['U06-CHILD-STARTED'],
            paused: id === 'pause-resume' ? 'U06-PAUSED-ACK' : null,
            resumed: id === 'pause-resume' ? 'U06-RESUMED-ACK' : null,
          },
        };
        const invokeCanonical = () => ({
          status: 0, error: null, stderr: '',
          stdout: JSON.stringify({
            type: 'result', exit_code: 0,
            data: {items: [{
              state: id === 'pause-resume' ? 'paused' : 'running',
              registration: {workspace: root},
              cost: {known: 0, reserved: 1, uncertain: 0},
            }]},
          }) + '\n',
        });
        const result = await runner.runCase({}, item, root, {
          invokeCanonical,
          discoverWorkspace: () => ({path: path.join(root, 'workspace.json'), root_task_id: 'root'}),
          maxRunMs: 10000,
        });
        assert.deepEqual(result.failures, [], JSON.stringify(result, null, 2));
        assert.equal(result.status, 'transport_observed');
        assert.ok(result.transcript.some(row => row.event?.type === 'started'));
        assert.ok(result.transcript.some(row => row.event?.type === 'exit'));
      } finally {
        fs.rmSync(root, {recursive: true, force: true});
      }
    });
  }
});
