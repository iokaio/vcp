// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }

test('A-B-A profile navigation reuses the journal writer and late receipts preserve newer command identities', async () => {
  const sessions = []; let connection;
  class EngineConnection {
    constructor(deps) { this.deps = deps; this.status = { phase: 'disconnected', generation: 0 }; connection = this; }
    state() { return this.status; } currentClient() { return this.client; } profileKey() { return this.profile; }
    select(profile, client) { this.profile = profile; this.client = client; this.status = { phase: 'connected', generation: this.status.generation + 1, role: 'controller', engineTrust: 'trusted' }; this.deps.publish(this.status); }
    async dispose() {}
  }
  class TaskSession {
    constructor(deps) { this.actions = deps.actions; sessions.push(this); }
    connection() {} notifyActions() {} dispose() { this.actions.invalidate(); }
  }
  class View { publish() {} dispose() {} }
  const disposable = { dispose() {} };
  const vscode = { StatusBarAlignment: { Left: 1 },
    window: { createOutputChannel: () => disposable, createStatusBarItem: () => ({ ...disposable, show() {} }), registerWebviewViewProvider: () => disposable },
    workspace: { isTrusted: true, workspaceFolders: [] }, commands: { async executeCommand() {} },
  };
  const load = Module._load;
  Module._load = function (request, parent, isMain) {
    if (request === 'vscode') return vscode;
    if (parent?.filename.endsWith('extension.js')) {
      if (request === './engine_connection.js') return { EngineConnection };
      if (request === './task_session.js') return { TaskSession };
      if (request === './task_panel.js') return { TaskPanel: View };
      if (request === './connection_view.js') return { ConnectionView: View };
      if (request === './commands.js') return { registerCommands: () => [], selectionForFolder: () => { throw Error('unexpected automatic selection'); } };
    }
    return load.call(this, request, parent, isMain);
  };
  const extensionPath = require.resolve('../dist/extension.js'); delete require.cache[extensionPath];
  let extension;
  try { extension = require(extensionPath); } finally { Module._load = load; }
  const saved = new Map();
  const context = { extensionUri: {}, subscriptions: [], workspaceState: { get: key => structuredClone(saved.get(key)), async update(key, records) { saved.set(key, structuredClone(records)); } } };
  extension.activate(context);
  const scope = { workspace: 'workspace', session: 'session' };
  const task = id => ({ scope, task: id, root: id, state: 'running', revision: '1', steering_revision: '0', effects: 'pending', reason: 'canonical task', pending_inputs: [] });
  const first = deferred(); const second = deferred(); const receipts = new Map(); const sent = [];
  const client = { scope, initialized: { methods: ['task/cancel', 'task/read', 'command/read'], capabilities: [] }, async call(method, params) {
    if (method === 'task/read') return { kind: 'task', value: task(params.task) };
    if (method === 'command/read') return { kind: 'acceptance', value: receipts.get(params.command_id) };
    assert.equal(method, 'task/cancel');
    const receipt = { command_id: params.mutation.command_id, scope, task: params.task, revision: '2', watermark: '3', outcome: 'accepted' };
    receipts.set(receipt.command_id, receipt); sent.push(receipt.command_id);
    await (sent.length === 1 ? first.promise : second.promise);
    return { kind: 'acceptance', value: receipt };
  } };
  let pendingFirst; let pendingSecond;
  try {
    connection.select('profile-a', client);
    const original = sessions.at(-1).actions;
    const action1 = original.register(task('task-1')).find(action => action.operation === 'cancel');
    pendingFirst = original.dispatch({ action: 'task', id: action1.id }); await tick();
    assert.equal(sent.length, 1);
    connection.select('profile-b', client); connection.select('profile-a', client);
    const returned = sessions.at(-1).actions;
    // Reconcile the first server-applied command before its original response arrives.
    await returned.reconcile();
    const action2 = returned.register(task('task-2')).find(action => action.operation === 'cancel');
    pendingSecond = returned.dispatch({ action: 'task', id: action2.id }); await tick();
    assert.equal(sent.length, 2);
    first.resolve(); await pendingFirst;
    const journal = saved.get('vcp.taskCommands.v1.profile-a');
    assert.deepEqual(new Set(journal.map(record => record.commandId)), new Set(sent), 'late old receipt must not overwrite the newer persisted command');
    assert.equal(returned, original, 'one writer owns each profile throughout activation');
    assert.equal(saved.has('vcp.taskCommands.v1.profile-b'), false);
    second.resolve(); await pendingSecond;
  } finally {
    first.resolve(); second.resolve(); await Promise.allSettled([pendingFirst, pendingSecond]);
    await extension.deactivate(); delete require.cache[extensionPath];
  }
});
