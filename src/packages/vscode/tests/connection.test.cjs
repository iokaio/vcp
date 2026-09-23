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
  assert.equal(launches[0].role, 'observer'); assert.equal(launches[0].transport, 'stdio'); assert.equal(launches[0].execution, undefined);
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
