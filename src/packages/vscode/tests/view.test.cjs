// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { parseViewMessage, connectionHtml } = require('../dist/view_model.js');
const { safeFailure } = require('../dist/diagnostics.js');

test('webview accepts only closed actions without executable or workspace arguments', () => {
  for (const action of ['connect', 'disconnect', 'refresh', 'ready']) assert.equal(parseViewMessage({ action }), action);
  for (const value of [null, [], { action: 'run' }, { action: 'connect', executable: 'evil.exe' }, { command: 'vcp.connect' }, Object.create({ action: 'connect' }), Object.defineProperty({}, 'action', { enumerable: true, get() { throw Error('getter invoked'); } }), { action: 'connect', [Symbol('extra')]: 1 }]) assert.equal(parseViewMessage(value), undefined);
});

test('HTML uses constrained local resources and nonce scripts without engine interpolation', () => {
  const html = connectionHtml('vscode-webview:', 'vscode-resource:style', 'vscode-resource:script', 'a'.repeat(48));
  assert.ok(html.includes("default-src 'none'"));
  assert.ok(html.includes("script-src 'nonce-"));
  assert.ok(!html.includes('unsafe-inline'));
  assert.ok(!html.includes('https://'));
  assert.ok(!html.includes('onclick='));
  assert.ok(connectionHtml('source', '" onload="bad', 'script', 'nonce').includes('&quot; onload=&quot;bad'));
});

test('engine markup remains bounded text and ready handshake never executes an engine action', () => {
  const nodes = new Map();
  function node(id) {
    return { id, textContent: '', children: [], listeners: {}, disabled: false,
      addEventListener(kind, handler) { this.listeners[kind] = handler; },
      replaceChildren() { this.children = []; }, append(...children) { this.children.push(...children); },
      set innerHTML(_) { throw Error('HTML sink forbidden'); },
    };
  }
  for (const id of ['connect', 'disconnect', 'refresh', 'phase', 'message', 'details', 'limitations']) nodes.set(id, node(id));
  const posted = [];
  let received;
  const context = {
    document: { getElementById: id => nodes.get(id), createElement: tag => node(tag) },
    window: { addEventListener: (kind, listener) => { assert.equal(kind, 'message'); received = listener; } },
    acquireVsCodeApi: () => ({ postMessage: value => posted.push(JSON.parse(JSON.stringify(value))) }),
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../media/connection.js'), 'utf8'), context);
  assert.deepEqual(posted, [{ action: 'ready' }]);
  const hostile = '<img src=x onerror="steal()">';
  received({ data: { type: 'connection', status: { phase: 'connected', message: hostile, engineBuild: hostile, engineExecutable: 'D:\\vcp.exe', editorTrusted: false, engineTrust: 'trusted', role: 'observer', limitations: Array.from({ length: 40 }, () => hostile) } } });
  assert.equal(nodes.get('message').textContent, hostile);
  assert.ok(nodes.get('details').children.some(child => child.textContent === hostile));
  assert.ok(nodes.get('details').children.some(child => child.textContent === 'restricted'));
  assert.ok(nodes.get('details').children.some(child => child.textContent === 'trusted'));
  assert.equal(nodes.get('limitations').children.length, 16);
  assert.equal(nodes.get('refresh').disabled, false);
  nodes.get('connect').listeners.click();
  assert.deepEqual(posted.at(-1), { action: 'connect' });
  received({ data: { type: 'connection', status: { phase: 'unavailable', message: 'x'.repeat(40000), limitations: [] } } });
  assert.equal(nodes.get('message').textContent.length, 32768);
  assert.equal(nodes.get('refresh').disabled, true);
});

test('diagnostics never display provider text or secret-bearing thrown errors', () => {
  const secret = 'secret-value';
  assert.ok(!safeFailure(Error(secret)).includes(secret));
  assert.ok(!safeFailure({ code: 'UNKNOWN', message: secret }).includes(secret));
  assert.match(safeFailure({ code: 'UNSUPPORTED_VERSION' }), /incompatible/);
});
