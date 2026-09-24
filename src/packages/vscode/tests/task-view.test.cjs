// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Module = require('node:module');
const { parseTaskPanelMessage, taskPanelHtml } = require('../dist/task_view_model.js');
const opaque = '12345678-1234-1234-1234-123456789abc';

test('task webview accepts only closed opaque messages without caller-selected commands or scopes', () => {
  for (const action of ['ready', 'previous', 'next', 'refresh']) assert.deepEqual(parseTaskPanelMessage({ action }), { action });
  for (const action of ['select', 'task', 'evidence', 'history']) assert.deepEqual(parseTaskPanelMessage({ action, id: opaque }), { action, id: opaque });
  for (const value of [null, [], { action: 'task', id: opaque, command: 'run' }, { action: 'evidence', id: opaque, artifact: 'secret' }, { action: 'history', id: opaque, cursor: 'chosen' }, { action: 'task', id: opaque, scope: {} }, { action: 'task', id: 'root' }, { action: 'refresh', id: opaque }, { action: 'run', id: opaque }, Object.create({ action: 'refresh' }), Object.defineProperty({}, 'action', { enumerable: true, get() { throw Error('getter invoked'); } }), { action: 'ready', [Symbol('extra')]: 1 }]) assert.equal(parseTaskPanelMessage(value), undefined);
});

test('task panel markup restricts resources and escapes resource attributes', () => {
  const html = taskPanelHtml('vscode-resource:', 'style" onload="bad', 'script', 'nonce');
  assert.ok(html.includes("default-src 'none'")); assert.ok(html.includes("script-src 'nonce-nonce'"));
  assert.ok(html.includes('style&quot; onload=&quot;bad')); assert.ok(!html.includes('unsafe-inline')); assert.ok(!html.includes('onclick='));
});

test('hostile model/tool markup is bounded text and duplicate clicks emit one opaque action', () => {
  const nodes = new Map(); const posted = []; let received;
  const node = id => ({ id, textContent: '', children: [], listeners: {}, disabled: false,
    addEventListener(kind, listener) { this.listeners[kind] = listener; }, replaceChildren() { this.children = []; }, append(...children) { this.children.push(...children); },
    set innerHTML(_) { throw Error('HTML sink forbidden'); }, set href(_) { throw Error('external link forbidden'); }, set src(_) { throw Error('external resource forbidden'); },
  });
  const ids = ['phase', 'message', 'owner', 'count', 'rows', 'previous', 'next', 'refresh', 'detail', 'questions', 'actions', 'history', 'evidence', 'commands', 'artifact'];
  for (const id of ids) nodes.set(`task-${id}`, node(id));
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../media/tasks.js'), 'utf8'), {
    document: { getElementById: id => nodes.get(id), createElement: tag => { assert.ok(!['a', 'img', 'iframe', 'script'].includes(tag)); return node(tag); } },
    window: { addEventListener: (kind, listener) => { assert.equal(kind, 'message'); received = listener; } },
    acquireVsCodeApi: () => ({ postMessage: value => posted.push(JSON.parse(JSON.stringify(value))) }),
  });
  assert.deepEqual(posted, [{ action: 'ready' }]);
  const hostile = '<img src=x onerror="steal()"><a href="command:evil">run</a>';
  received({ data: { type: 'tasks', state: { phase: 'current', message: hostile.repeat(300), owner: 'another_connection', total: 200, offset: 0, hasMore: true,
    rows: Array.from({ length: 200 }, () => ({ task: hostile, state: 'running', effects: 'unknown', actionId: opaque, pendingInputs: [] })),
    detail: { task: { task: 'root', state: 'running', steering_revision: '7', effects: 'unknown' }, objective: { text: hostile, truncated: false }, model: {}, commentary: 'unavailable', questions: [], rows: [{ kind: 'commentary', task: 'child', text: { text: hostile, truncated: false } }] },
    actions: [{ id: opaque, label: hostile }], evidence: [{ id: opaque, label: hostile }], commands: [], artifact: hostile,
  } } });
  assert.equal(nodes.get('task-message').textContent.length, 4096);
  assert.equal(nodes.get('task-rows').children.length, 50);
  assert.ok(nodes.get('task-detail').children.some(child => child.textContent === hostile));
  assert.ok(nodes.get('task-history').children[0].textContent.includes(hostile));
  assert.equal(nodes.get('task-artifact').textContent, hostile);
  const action = nodes.get('task-actions').children[0]; action.listeners.click(); action.listeners.click();
  assert.deepEqual(posted.slice(1), [{ action: 'task', id: opaque }]);
  const evidence = nodes.get('task-evidence').children[0]; evidence.listeners.click();
  assert.deepEqual(posted.at(-1), { action: 'evidence', id: opaque });
  received({ data: { type: 'tasks', state: { phase: 'disconnected', message: 'unknown', owner: 'unknown', rows: [], actions: [{ id: opaque, label: 'run' }], commands: [] } } });
  assert.equal(nodes.get('task-actions').children[0].disabled, true);
  assert.equal(nodes.get('task-detail').children.length, 0);
});

test('TaskPanel rejects forged payloads and bounds messages before posting to the webview', async () => {
  const original = Module._load;
  Module._load = function (request, parent, isMain) {
    if (request === 'vscode') return { Uri: { joinPath: (_uri, ...parts) => ({ toString: () => parts.join('/') }) } };
    return original.call(this, request, parent, isMain);
  };
  let TaskPanel;
  try { ({ TaskPanel } = require('../dist/task_panel.js')); } finally { Module._load = original; }
  const posts = []; const actions = []; let receive;
  const disposable = () => ({ dispose() {} });
  const webview = { cspSource: 'local:', asWebviewUri: uri => uri, postMessage: value => { posts.push(value); return Promise.resolve(true); }, onDidReceiveMessage: callback => { receive = callback; return disposable(); } };
  const panel = new TaskPanel({}, async message => { actions.push(message); });
  panel.resolveWebviewView({ webview, visible: true, onDidChangeVisibility: disposable, onDidDispose: disposable });
  assert.equal(webview.options.enableScripts, true); assert.equal(webview.options.localResourceRoots.length, 1);
  receive({ action: 'evidence', id: opaque, artifact: 'caller-chosen' }); receive({ action: 'task', id: opaque }); await Promise.resolve();
  assert.deepEqual(actions, [{ action: 'task', id: opaque }]);
  panel.publish({ phase: 'current', message: 'x'.repeat(2 * 1024 * 1024), owner: 'this_connection', rows: [], actions: [], commands: [], total: 0, offset: 0, hasMore: false });
  assert.ok(posts.every(value => Buffer.byteLength(JSON.stringify(value)) <= 1024 * 1024), 'host must bound outgoing transport, not only rendered text');
  panel.dispose();
});
