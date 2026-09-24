// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const { EngineConnection } = require('../dist/engine_connection.js');
const { WorkspaceMap } = require('../dist/workspace_map.js');
const { connectionRestriction } = require('../dist/trust.js');
const scope = { workspace: 'actual-workspace', session: 'actual-session' };
const root = '\\\\?\\C:\\work\\project';
const selection = { workspaceUri: 'file:///C:/work/project', workspacePath: 'C:\\work\\project', executable: 'C:\\trusted\\vcp.exe', executableSource: 'global', workspaceTrusted: true };
const workspace = { workspace: scope.workspace, host: 'actual-host', root, root_id: 'actual-root', binding_revision: '9007199254740993', trust: 'untrusted', revision: '13', authority_revision: '7' };
const task = { scope, task: 'actual-task', pending_inputs: [{ id: 'actual-question', kind: 'approval', revision: '5' }] };
function fake(overrides = {}) {
  const calls = [];
  return {
    calls, disposed: 0, scope,
    initialized: { execution_host: { id: 'actual-host', platform: 'windows' }, engine_build: 'actual-build', protocol_version: '1.0' },
    async call(method, params, options) {
      calls.push({ method, params, options });
      if (this.disposed) throw new Error('disposed');
      if (overrides.call) return overrides.call(method, params, options, calls);
      if (method === 'workspace/open') return { kind: 'workspace', value: workspace };
      if (method === 'session/snapshot') return { kind: 'snapshot', value: { session: { scope }, subscription: 'snapshot-subscription', sequence: '11', event_cursor: 'events-at-cut', watermark: '17', complete: true, tasks: [task] } };
      if (method === 'events/unsubscribe') return { kind: 'unsubscribed', value: { subscription: params.subscription } };
      throw new Error(`unexpected method ${method}`);
    },
    async dispose() { this.disposed++; },
  };
}
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
const tick = () => new Promise(resolve => setImmediate(resolve));

test('observer uses initialized identities, actual trust/pending data and cleans every refresh cursor', async () => {
  const client = fake(); const launches = [];
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async path => { assert.equal(path, selection.workspacePath); return root; }, launch: async options => { launches.push(options); return client; } });
  const state = await connection.connect(selection);
  assert.equal(state.phase, 'connected'); assert.equal(state.role, 'observer');
  assert.equal(state.engineTrust, 'untrusted'); assert.equal(state.editorTrusted, true);
  assert.equal(state.pendingInputs, 1); assert.equal(state.taskCount, 1); assert.equal(state.watermark, '17');
  assert.deepEqual(state.scope, scope); assert.equal(state.host.id, 'actual-host'); assert.equal(state.engineBuild, 'actual-build'); assert.equal(state.workspaceRoot, root);
  assert.equal(state.engineExecutable, selection.executable); assert.equal(state.bindingRevision, '9007199254740993'); assert.equal(state.rootId, 'actual-root');
  assert.ok(launches[0].initialize.required_capabilities.includes('workspace/binding/1'));
  assert.equal(launches[0].role, 'observer'); assert.equal(launches[0].transport, 'windows_pipe'); assert.equal(launches[0].execution, undefined);
  for (let i = 0; i < 12; i++) await connection.refresh();
  assert.equal(client.calls.filter(c => c.method === 'events/unsubscribe').length, 13);
  assert.ok(client.calls.every(c => ['workspace/open', 'session/snapshot', 'events/unsubscribe'].includes(c.method)));
  for (const call of client.calls) {
    if (call.method === 'workspace/open') { assert.equal(call.params.root, root); assert.equal(call.params.host, 'actual-host'); }
    else assert.deepEqual(call.params.scope, scope);
  }
  await connection.dispose(); assert.equal(client.disposed, 1);
});

test('restricted mode keeps observer-only access while project executable, remote host and mismatched URI are refused', async () => {
  assert.equal(connectionRestriction({ ...selection, workspaceTrusted: false }, 'win32'), undefined);
  for (const changed of [{ executableSource: 'workspace' }, { remoteName: 'ssh-remote' }, { workspaceUri: 'file:///C:/other' }, { workspaceUri: 'vscode-remote://ssh/path' }, { executable: 'relative.exe' }]) {
    let started = false;
    const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => { started = true; return root; }, launch: async () => { started = true; return fake(); } });
    assert.equal((await connection.connect({ ...selection, ...changed })).phase, 'unavailable'); assert.equal(started, false); await connection.dispose();
  }
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => fake() });
  assert.equal((await connection.connect({ ...selection, workspaceTrusted: false })).editorTrusted, false); await connection.dispose();
});

test('folder/trust invalidation prevents late launch from publishing and owns its cleanup', async () => {
  const launch = deferred(); const client = fake(); const states = [];
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: () => launch.promise, publish: state => states.push(state) });
  const pending = connection.connect(selection); await tick();
  await connection.invalidate('Workspace folders changed; reconnect explicitly.', false);
  launch.resolve(client); await pending;
  assert.equal(connection.state().phase, 'disconnected'); assert.equal(connection.state().editorTrusted, false);
  assert.equal(states.some(state => state.phase === 'connected'), false); assert.equal(client.disposed, 1); assert.equal(client.calls.length, 0);
});
test('extension disposal waits for late native launch cleanup rather than abandoning it', async () => {
  const launch = deferred(); const client = fake();
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: () => launch.promise });
  const pending = connection.connect(selection); await tick();
  let disposed = false; const disposal = connection.dispose().then(() => { disposed = true; });
  await tick(); assert.equal(disposed, false); launch.resolve(client); await disposal; await pending;
  assert.equal(client.disposed, 1); assert.equal(connection.state().phase, 'disconnected');
});

test('missing/incompatible host, canonical binding mismatch and snapshot gap never leave connected state', async () => {
  for (const mode of ['missing', 'incompatible', 'capability', 'host', 'root', 'root-id', 'binding-revision', 'null-root-id', 'null-binding-revision', 'gap']) {
    const client = fake({ call: async (method, params) => {
      if (method === 'workspace/open') return { kind: 'workspace', value: { ...workspace, root: mode === 'root' ? 'C:\\moved' : root, root_id: mode === 'root-id' ? undefined : mode === 'null-root-id' ? null : workspace.root_id, binding_revision: mode === 'binding-revision' ? undefined : mode === 'null-binding-revision' ? null : workspace.binding_revision } };
      if (method === 'session/snapshot') return { kind: 'gap', value: { subscription: 'gapped' } };
      return { kind: 'unsubscribed', value: { subscription: params.subscription } };
    } });
    if (mode === 'host') client.initialized.execution_host.platform = 'linux';
    const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => {
      if (mode === 'capability') throw Object.assign(new Error('unsupported projection'), { code: 'CAPABILITY_UNAVAILABLE' });
      if (mode === 'missing') throw new Error('SECRET raw native diagnostic');
      if (mode === 'incompatible') throw Object.assign(new Error('SECRET peer payload'), { code: 'UNSUPPORTED_VERSION' });
      return client;
    } });
    const state = await connection.connect(selection);
    assert.equal(state.phase, 'unavailable'); assert.equal(state.scope, undefined); assert.equal(state.pendingInputs, undefined); assert.equal(state.message.includes('SECRET'), false);
    if (mode === 'incompatible') assert.match(state.message, /incompatible/);
    if (mode === 'capability') assert.match(state.message, /required observer capabilities/);
    if (mode === 'gap') assert.equal(client.calls.at(-1).method, 'events/unsubscribe');
    if (['root', 'root-id', 'binding-revision', 'null-root-id', 'null-binding-revision'].includes(mode)) assert.equal(client.calls.some(call => call.method === 'session/snapshot'), false);
    await connection.dispose();
  }
});

test('failed refresh clears its promise and reconnect can publish a fresh capture', async () => {
  const client = fake(); let fail = false; const original = client.call;
  client.call = async function (...args) { if (fail) throw new Error('lost'); return original.apply(this, args); };
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => client });
  await connection.connect(selection); fail = true;
  const old = connection.refresh(); await old;
  assert.equal(connection.state().phase, 'unavailable'); assert.notEqual(connection.refresh(), old);
  assert.equal(connection.state().pendingInputs, undefined); await connection.dispose();
});

test('aggregate snapshot deadline bounds pagination and still closes its native subscription', async () => {
  let now = 0;
  const client = fake({ call: async (method, params) => {
    if (method === 'workspace/open') return { kind: 'workspace', value: workspace };
    if (method === 'events/unsubscribe') return { kind: 'unsubscribed', value: { subscription: params.subscription } };
    now += 10_001;
    return { kind: 'snapshot', value: { session: { scope }, subscription: 'same', sequence: '1', event_cursor: 'events', watermark: '2', complete: false, next_cursor: `page-${now}`, tasks: [] } };
  } });
  const connection = new EngineConnection({ platform: 'win32', now: () => now, canonicalize: async () => root, launch: async () => client });
  const state = await connection.connect(selection);
  assert.equal(state.phase, 'unavailable'); assert.match(state.message, /deadline/);
  assert.equal(client.calls.filter(call => call.method === 'session/snapshot').length, 3);
  assert.equal(client.calls.at(-1).method, 'events/unsubscribe'); await connection.dispose();
});

test('map distinguishes same-name and nested roots by URI plus engine host and rejects stale bindings', () => {
  const map = new WorkspaceMap(); const generation = map.invalidate();
  const a = 'file:///C:/one/project'; const b = 'file:///C:/two/project'; const nested = `${a}/project`;
  map.bind(generation, a, workspace); map.bind(generation, b, { ...workspace, workspace: 'second' }); map.bind(generation, nested, { ...workspace, workspace: 'nested' });
  map.bind(generation, a, { ...workspace, host: 'different-host', workspace: 'remote-identity' });
  assert.equal(map.get(a, 'actual-host').workspace, scope.workspace); assert.equal(map.get(b, 'actual-host').workspace, 'second'); assert.equal(map.get(nested, 'actual-host').workspace, 'nested');
  assert.equal(map.get(a, 'different-host').workspace, 'remote-identity');
  assert.equal(map.get(a, 'actual-host').rootId, 'actual-root');
  assert.equal(map.get(a, 'actual-host').bindingRevision, '9007199254740993');
  map.bind(generation, a, { ...workspace, root_id: 'rebound-root', binding_revision: '9007199254740994' });
  assert.equal(map.get(a, 'actual-host').rootId, 'rebound-root');
  assert.equal(map.get(a, 'actual-host').bindingRevision, '9007199254740994');
  assert.equal(map.get(a, 'actual-host').workspaceRevision, '13');
  map.invalidate();
  assert.equal(map.get(a, 'actual-host'), undefined); assert.equal(map.bind(generation, a, workspace), false);
});

test('reload restores only the saved observer and never launches on stale or mismatched recovery', async () => {
  const { profile } = require('../dist/recovery.js');
  const reference = { endpoint: 'native-verified', server: { pid: 7 }, scope };
  const saved = { folderUri: selection.workspaceUri, profile: profile(selection), reference };
  const observer = fake(); observer.observerReconnectReference = () => reference;
  const stored = []; let launches = 0; let reconnects = 0;
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => { launches++; return fake(); }, reconnect: async options => { reconnects++; assert.deepEqual(options.reference, reference); return observer; }, saveRecovery: async value => stored.push(value) });
  await connection.restore({ ...selection, executable: 'C:\\other\\vcp.exe' }, saved);
  assert.equal(reconnects, 0);
  assert.equal((await connection.restore(selection, saved)).role, 'observer');
  assert.equal(launches, 0); assert.equal(connection.state().pendingInputs, 1);
  assert.equal(observer.calls.some(call => call.method === 'controller/acquire'), false);
  await connection.dispose(); assert.deepEqual(stored.at(-1), saved);
  const stale = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => { launches++; return fake(); }, reconnect: async () => { throw Error('stale native process'); } });
  assert.equal((await stale.restore(selection, saved)).phase, 'unavailable');
  assert.equal(launches, 0); await stale.dispose();
});

function controlled(gate) {
  const client = fake(); const base = client.call;
  const reference = { endpoint: 'native-verified', server: { pid: 7 }, scope };
  client.observerReconnectReference = () => reference;
  client.call = async function (method, params, options) {
    if (['controller/read', 'controller/acquire', 'workspace/setTrust'].includes(method)) {
      this.calls.push({ method, params });
      if (method === 'controller/read') return { kind: 'controller', value: { revision: '9', ownership: 'this_connection' } };
      if (method === 'workspace/setTrust' && gate) await gate.promise;
      return { kind: 'acceptance', value: {} };
    }
    return base.call(this, method, params, options);
  };
  return client;
}

test('explicit trust command uses observed revisions once and releases stale control after commit', async () => {
  const gate = deferred(); const controller = controlled(gate); const observer = fake();
  const saved = [];
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async options => { assert.equal(options.role, 'controller'); return controller; }, reconnect: async () => observer, saveRecovery: async value => saved.push(value) });
  assert.equal((await connection.connect(selection, 'controller')).role, 'controller');
  assert.equal(controller.calls.find(call => call.method === 'controller/acquire').params.expected_revision, '9');
  const first = connection.setTrust(true); const duplicate = connection.setTrust(true); assert.equal(first, duplicate);
  const request = controller.calls.find(call => call.method === 'workspace/setTrust');
  assert.equal(request.params.expected_binding_revision, workspace.binding_revision);
  assert.equal(request.params.mutation.expected_revision, workspace.revision);
  assert.equal(request.params.mutation.steering_revision, '0'); assert.equal(request.params.trusted, true);
  gate.resolve(); assert.equal((await first).role, 'observer'); assert.equal(controller.disposed, 1);
  assert.equal(controller.calls.filter(call => call.method === 'workspace/setTrust').length, 1);
  await assert.rejects(connection.setTrust(false));
  await connection.disconnect(); assert.equal(saved.at(-1), undefined); await connection.dispose();
});

test('editor revocation fences an outstanding grant and never restores its late result', async () => {
  const gate = deferred(); const controller = controlled(gate); let reconnected = false;
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => controller, reconnect: async () => { reconnected = true; return fake(); } });
  await connection.connect(selection, 'controller'); const pending = connection.setTrust(true);
  await connection.editorTrustChanged(false); assert.equal(controller.disposed, 1);
  gate.resolve(); await pending;
  assert.equal(connection.state().phase, 'disconnected'); assert.equal(connection.state().editorTrusted, false); assert.equal(reconnected, false);
  await connection.dispose();
});

test('restricted controller can revoke but cannot grant; observer trust changes never mutate another owner', async () => {
  const controller = controlled(); const observer = fake();
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => controller, reconnect: async () => observer });
  await connection.connect({ ...selection, workspaceTrusted: false }, 'controller');
  await assert.rejects(connection.setTrust(true)); await connection.setTrust(false);
  assert.equal(controller.calls.find(call => call.method === 'workspace/setTrust').params.trusted, false);
  await connection.editorTrustChanged(true); await connection.editorTrustChanged(false);
  assert.equal(observer.calls.some(call => call.method === 'workspace/setTrust'), false);
  await connection.dispose();
});

test('moved-root reconciliation uses selected native identity and never connects after stale selection', async () => {
  const gate = deferred(); const calls = []; let launches = 0;
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, rebind: async options => { calls.push(options); await gate.promise; return { workspace: scope.workspace, root }; }, launch: async () => { launches++; return fake(); } });
  const pending = connection.reconcile(selection, scope.workspace); await tick();
  assert.equal(calls[0].workspaceId, scope.workspace); assert.equal(calls[0].workspace, root);
  await connection.invalidate('changed'); gate.resolve(); await pending;
  assert.equal(launches, 0); await connection.dispose();
});

test('opposite trust requests fence control instead of silently reusing an in-flight grant', async () => {
  const gate = deferred(); const controller = controlled(gate);
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => controller });
  await connection.connect(selection, 'controller');
  const grant = connection.setTrust(true); const revoked = await connection.setTrust(false);
  assert.equal(revoked.phase, 'disconnected'); assert.equal(controller.disposed, 1);
  gate.resolve(); await grant; assert.equal(connection.state().phase, 'disconnected'); await connection.dispose();
});

test('trust revoked during launch cannot be overwritten by captured trusted selection', async () => {
  const launch = deferred(); const controller = controlled();
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: () => launch.promise });
  const pending = connection.connect(selection, 'controller'); await tick();
  await connection.editorTrustChanged(false); launch.resolve(controller); await pending;
  assert.equal(connection.state().editorTrusted, false); assert.equal(connection.state().phase, 'disconnected');
  assert.equal(controller.calls.some(call => call.method === 'controller/acquire'), false);
  await assert.rejects(connection.setTrust(true)); await connection.dispose();
});

test('failed observer reconnect retains discovery and malformed recovery cannot supply a workspace default', async () => {
  const { profile, recovery } = require('../dist/recovery.js');
  const reference = { endpoint: 'native-verified', server: { pid: 7 }, scope };
  const saved = { folderUri: selection.workspaceUri, profile: profile(selection), reference };
  let stored = saved;
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => { throw Error('must not launch'); }, reconnect: async () => { throw Error('temporary unavailable'); }, saveRecovery: async value => { stored = value; } });
  await connection.restore(selection, saved); await connection.dispose(); assert.deepEqual(stored, saved);
  assert.deepEqual(recovery(saved), saved);
  for (const reference of [{}, { scope: {} }, { ...saved.reference, ticket: 'secret' }, { ...saved.reference, scope: { workspace: 'bad/id', session: 's' } }]) assert.equal(recovery({ ...saved, reference }), undefined);
});

test('lease loss and unknown trust outcome stop claiming controller state', async () => {
  for (const mode of ['lease', 'mutation']) {
    const controller = controlled(); const base = controller.call;
    const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => controller });
    await connection.connect(selection, 'controller');
    controller.call = async function (method, params, options) {
      if (mode === 'lease' && method === 'controller/read') return { kind: 'controller', value: { revision: '10', ownership: 'other_connection' } };
      if (mode === 'mutation' && method === 'workspace/setTrust') throw Error('lost acknowledgement');
      return base.call(this, method, params, options);
    };
    if (mode === 'lease') assert.equal((await connection.refresh()).phase, 'unavailable');
    else await assert.rejects(connection.setTrust(false));
    assert.equal(connection.state().role, undefined); assert.equal(controller.disposed, 1);
    await connection.dispose();
  }
});

test('refresh re-resolves the selected folder and invalidates a redirected mapping', async () => {
  let resolved = root; const client = fake();
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async path => { assert.equal(path, selection.workspacePath); return resolved; }, launch: async () => client });
  await connection.connect(selection); resolved = 'C:\\different-root';
  assert.equal((await connection.refresh()).phase, 'unavailable');
  assert.equal(connection.state().rootId, undefined); assert.equal(client.disposed, 1);
  await connection.dispose();
});

test('execution launch is explicit, carries only requested root/profile and never reuses inspection control', async () => {
  const inspection = controlled(), executionClient = controlled();
  inspection.attachment = () => ({ opaque: 'inspection-only' });
  const launches = [], published = [], saved = []; let attaches = 0;
  const execution = { profile: 'C:\\trusted\\execution.json', providerCredential: 'test-only-provider-secret', credentials: { helper: 'test-only-helper-secret' } };
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root,
    launch: async options => { launches.push(options); return launches.length === 1 ? inspection : executionClient; },
    attach: async () => { attaches++; throw Error('execution must not reuse inspection attachment'); },
    publish: state => published.push(state), saveRecovery: async value => saved.push(value) });
  await connection.connect(selection, 'controller');
  const state = await connection.connectExecution(selection, execution, 'explicit-new-root');
  assert.equal(state.phase, 'connected'); assert.equal(state.role, 'controller'); assert.equal(inspection.disposed, 1);
  assert.equal(attaches, 0); assert.equal(launches.length, 2); assert.equal(launches[0].execution, undefined);
  assert.equal(launches[1].rootTask, 'explicit-new-root'); assert.deepEqual(launches[1].execution, execution);
  assert.equal(launches[1].role, 'controller'); assert.equal(launches[1].transport, 'windows_pipe');
  assert.equal(executionClient.calls.filter(call => call.method === 'controller/acquire').length, 1);
  assert(!executionClient.calls.some(call => ['turn/start','turn/steer','session/resume'].includes(call.method)));
  for (const secret of [execution.providerCredential, execution.credentials.helper, execution.profile]) {
    assert(!JSON.stringify(published).includes(secret)); assert(!JSON.stringify(saved).includes(secret));
  }
  assert(saved.at(-1)?.reference); assert.equal(saved.at(-1).execution, undefined); assert.equal(saved.at(-1).rootTask, undefined);
  await connection.dispose();
});

test('execution refuses an untrusted editor before persistence, root lookup or process launch', async () => {
  let sideEffects = 0;
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => { sideEffects++; return root; },
    launch: async () => { sideEffects++; return controlled(); }, saveRecovery: async () => { sideEffects++; } });
  await assert.rejects(connection.connectExecution({ ...selection, workspaceTrusted: false }, { profile: 'p', providerCredential: 'secret' }, 'root'), /trusted editor/);
  assert.equal(sideEffects, 0); assert.equal(connection.currentClient(), undefined); await connection.dispose();
});

test('reload after execution reconnects only as observer without restoring credentials or acquiring control', async () => {
  const original = controlled(), saved = [];
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root, launch: async () => original, saveRecovery: async value => saved.push(value) });
  await connection.connectExecution(selection, { profile: 'execution-profile', providerCredential: 'secret-execution-token' }, 'selected-root');
  const recovery = saved.at(-1); await connection.dispose();
  const observer = fake(), reconnects = []; let starts = 0;
  const restored = new EngineConnection({ platform: 'win32', canonicalize: async () => root,
    launch: async () => { starts++; throw Error('reload cannot start execution'); },
    attach: async () => { starts++; throw Error('reload cannot restore controller attachment'); },
    reconnect: async options => { reconnects.push(options); return observer; } });
  const state = await restored.restore(selection, recovery);
  assert.equal(state.phase, 'connected'); assert.equal(state.role, 'observer'); assert.equal(starts, 0);
  assert.equal(reconnects.length, 1); assert.equal(reconnects[0].execution, undefined); assert.equal(reconnects[0].rootTask, undefined);
  assert(!JSON.stringify(reconnects).includes('secret-execution-token'));
  assert(!observer.calls.some(call => ['controller/acquire','turn/start','session/resume'].includes(call.method)));
  await restored.dispose();
});

test('revocation fences delayed execution launch without publishing its credentials or admitting work', async () => {
  const gate = deferred(), client = controlled(), states = [], saved = [];
  const connection = new EngineConnection({ platform: 'win32', canonicalize: async () => root,
    launch: async () => gate.promise, publish: value => states.push(value), saveRecovery: async value => saved.push(value) });
  const pending = connection.connectExecution(selection, { profile: 'private-profile', providerCredential: 'private-secret' }, 'root');
  await tick(); await connection.editorTrustChanged(false); gate.resolve(client); await pending;
  assert.equal(connection.state().phase, 'disconnected'); assert.equal(connection.state().editorTrusted, false);
  assert.equal(client.disposed, 1); assert.equal(client.calls.length, 0);
  assert(!JSON.stringify(states).includes('private-secret')); assert(!JSON.stringify(saved).includes('private-secret'));
  await connection.dispose();
});

test('publisher profile is explicit controller-only launch input and never persisted or reloaded', async () => {
  const launches=[], saved=[], states=[]; const client=controlled();
  const connection=new EngineConnection({platform:'win32',canonicalize:async()=>root,launch:async options=>{launches.push(options);return client;},saveRecovery:async value=>saved.push(value),publish:value=>states.push(value)});
  const path='C:\\trusted-private\\publisher.json';
  assert.equal((await connection.connectPublisher(selection,path)).role,'controller');
  assert.deepEqual(launches[0].publisher,{profile:path});assert.equal(launches[0].execution,undefined);assert.equal(launches[0].role,'controller');
  assert.ok(!JSON.stringify(saved).includes(path));assert.ok(!JSON.stringify(states).includes(path));
  const recovery=saved.at(-1);await connection.dispose();
  const reconnects=[];let starts=0;const observer=fake();
  const restored=new EngineConnection({platform:'win32',canonicalize:async()=>root,launch:async()=>{starts++;throw Error('no fallback');},reconnect:async options=>{reconnects.push(options);return observer;}});
  await restored.restore(selection,recovery);assert.equal(starts,0);assert.equal(restored.state().role,'observer');assert.equal(reconnects[0].publisher,undefined);
  assert.ok(!observer.calls.some(c=>c.method==='controller/acquire'));await restored.dispose();
});

test('publisher profile outside native local trust boundaries is rejected before side effects', async () => {
  let starts=0;const connection=new EngineConnection({platform:'win32',canonicalize:async()=>{starts++;return root;},launch:async()=>{starts++;return controlled();},saveRecovery:async()=>{starts++;}});
  for(const [selected,path] of [[{...selection,workspaceTrusted:false},'C:\\trusted\\profile.json'],[selection,'C:\\work\\project\\profile.json'],[selection,'c:\\WORK\\project\\nested\\..\\profile.json'],[selection,'relative.json'],[selection,'\\\\server\\share\\profile.json'],[{...selection,remoteName:'ssh-remote'},'C:\\trusted\\profile.json']]) await assert.rejects(connection.connectPublisher(selected,path));
  assert.equal(starts,0);await connection.dispose();
});

test('publisher profile may live in registry data root; native loader resolves canonical-store exclusions', () => {
  const {publisherProfileRestriction}=require('../dist/trust.js');
  assert.equal(publisherProfileRestriction({...selection,dataPath:'C:\\vcp-data'},'C:\\vcp-data\\publisher.json','win32'),undefined);
});

test('publisher profile launch never reuses old controller attachment or falls back on failure', async () => {
  let attaches=0;const launches=[];const first=controlled();first.attachment=()=>({opaque:'old-owner'});
  const connection=new EngineConnection({platform:'win32',canonicalize:async()=>root,launch:async options=>{launches.push(options);if(launches.length>1)throw Error('native profile unavailable');return first;},attach:async()=>{attaches++;return controlled();}});
  await connection.connect(selection,'controller');const state=await connection.connectPublisher(selection,'C:\\private\\profile.json');
  assert.equal(state.phase,'unavailable');assert.equal(attaches,0);assert.equal(launches.length,2);assert.equal(launches[0].publisher,undefined);assert.ok(!state.message.includes('profile.json'));await connection.dispose();
});
