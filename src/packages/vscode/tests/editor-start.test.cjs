// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
let active;
class EventEmitter { constructor() { this.event = () => ({ dispose() {} }); } fire() {} dispose() {} }
const folder = { name: 'project', uri: { scheme: 'file', fsPath: 'C:\\project', toString: () => 'file:///C:/project' } };
const vscode = {
  EventEmitter, env: {},
  workspace: {
    get isTrusted() { return active.trusted; }, workspaceFolders: [folder],
    getConfiguration() { return { inspect: key => ({ globalValue: key === 'engineExecutable' ? 'C:\\vcp.exe' : undefined }) }; },
  },
  window: {
    async showQuickPick(items) { return items[0]; },
    async showOpenDialog() { return [{ scheme: 'file', fsPath: 'C:\\profile.json' }]; },
    async showInputBox(options) {
      active.prompts.push(options.title);
      if (options.title === 'Task objective') return 'Repair the project';
      if (options.title === 'Configured maximum requests') {
        assert.match(options.prompt, /no spending or elapsed-time cap/);
        assert.ok(options.validateInput('0'));
        assert.ok(options.validateInput('1025'));
        assert.equal(options.validateInput('8'), undefined);
        return active.cancel ? undefined : '8';
      }
      if (options.title === 'Provider credential') return 'synthetic-editor-credential';
      throw Error(`Unexpected editor prompt: ${options.title}`);
    },
    showErrorMessage(message) { active.errors.push(message); },
    showInformationMessage(message) { active.messages.push(message); },
  },
};
const load = Module._load;
Module._load = function (request, parent, isMain) { return request === 'vscode' ? vscode : load.call(this, request, parent, isMain); };
let EditorWorkflow;
try { ({ EditorWorkflow } = require('../dist/editor_workflow.js')); } finally { Module._load = load; }

function fixture(options = {}) {
  const f = { trusted: true, cancel: false, prompts: [], calls: [], saved: [], errors: [], messages: [], connections: 0, ...options };
  active = f;
  const scope = { workspace: 'workspace', session: 'session' };
  const state = { phase: 'connected', generation: 1, role: 'controller', rootId: 'root', bindingRevision: '1', engineTrust: 'trusted', editorTrusted: true };
  const client = { scope, initialized: { methods: ['turn/start'] }, async call(method, params) {
    f.calls.push({ method, params: structuredClone(params) });
    return { kind: 'acceptance', value: { scope, task: params.task, command_id: params.mutation.command_id, outcome: 'accepted' } };
  } };
  const connection = {
    state: () => state, profileKey: () => 'profile', currentClient: () => client,
    async connectExecution(selection, execution) {
      f.connections++;
      assert.equal(selection.workspaceTrusted, true);
      assert.equal(execution.providerCredential, 'synthetic-editor-credential');
      return state;
    },
  };
  f.workflow = new EditorWorkflow(connection, { workspaceState: { get: () => undefined, async update(key, value) { f.saved.push(structuredClone(value)); } } }, () => undefined);
  return f;
}

test('editor start sends explicit unbounded financial/time limits and journals exact acceptance', async () => {
  const f = fixture();
  await f.workflow.start();
  assert.deepEqual(f.errors, []);
  assert.deepEqual(f.prompts, ['Task objective', 'Configured maximum requests', 'Provider credential']);
  assert.equal(f.calls.length, 1);
  const { method, params } = f.calls[0];
  assert.equal(method, 'turn/start');
  assert.deepEqual(params.budget, { currency: 'USD', cap_micros: { version: 1, kind: 'unbounded' }, max_requests: 8, deadline_seconds: { version: 1, kind: 'unbounded' } });
  assert.equal(f.saved.at(-1)[0].phase, 'accepted');
  assert.equal(f.saved.at(-1)[0].id, params.mutation.command_id);
  assert.equal(JSON.stringify(f.saved).includes('synthetic-editor-credential'), false);
  f.workflow.dispose();
});

test('cancelling editor request selection creates no command or connection', async () => {
  const f = fixture({ cancel: true });
  await f.workflow.start();
  assert.equal(f.connections, 0);
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.saved, []);
  f.workflow.dispose();
});

test('unbounded editor start still requires workspace trust', async () => {
  const f = fixture({ trusted: false });
  await f.workflow.start();
  assert.deepEqual(f.errors, ['Editor trust required']);
  assert.deepEqual(f.prompts, []);
  assert.equal(f.connections, 0);
  assert.deepEqual(f.calls, []);
  f.workflow.dispose();
});
