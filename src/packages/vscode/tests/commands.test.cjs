// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const originalLoad = Module._load;
let globalPaths, registerCommands;
const vscode = { commands: undefined, window: undefined, workspace: undefined, env: undefined };
try {
  Module._load = function (name, ...args) { if (name === 'vscode') return vscode; return originalLoad.call(this, name, ...args); };
  ({ globalPaths, registerCommands } = require('../dist/commands.js'));
} finally { Module._load = originalLoad; }

test('only User settings choose executable and data; all project overrides are ignored', () => {
  const values = {
    engineExecutable: { globalValue: 'D:\\trusted\\vcp.exe', workspaceValue: 'D:\\project\\evil.exe', workspaceFolderValue: 'D:\\project\\evil2.exe' },
    dataDirectory: { globalValue: 'D:\\trusted-data', workspaceValue: 'D:\\project-data' },
  };
  assert.deepEqual(globalPaths({ inspect: key => values[key] }), { executable: 'D:\\trusted\\vcp.exe', dataPath: 'D:\\trusted-data' });
  assert.deepEqual(globalPaths({ inspect: () => ({ workspaceValue: 'D:\\evil.exe', workspaceFolderValue: 'D:\\evil2.exe' }) }), { executable: '' });
});

test('disconnect cancels a pending folder-picker intent before it can launch', async () => {
  const commands = new Map();
  const noop = () => ({ dispose() {} });
  const folder = { name: 'same', uri: { toString: () => 'file:///D:/project', fsPath: 'D:\\project' } };
  let finishPicker;
  const picker = new Promise(resolve => { finishPicker = resolve; });
  vscode.commands = { registerCommand: (name, handler) => { commands.set(name, handler); return noop(); } };
  vscode.window = { showQuickPick: () => picker, showErrorMessage: async () => {} };
  vscode.workspace = { workspaceFolders: [folder], isTrusted: true, getConfiguration: () => ({ inspect: () => ({ globalValue: 'D:\\vcp.exe' }) }), onDidChangeWorkspaceFolders: noop, onDidGrantWorkspaceTrust: noop, onDidChangeConfiguration: noop };
  vscode.env = {};
  let launched = 0;
  let disconnected = 0;
  const connection = { state: () => ({ phase: 'disconnected' }), connect: async () => { launched++; }, disconnect: async () => { disconnected++; } };
  const disposables = registerCommands(connection, { appendLine() {} });
  const pending = commands.get('vcp.connect')();
  await commands.get('vcp.disconnect')();
  finishPicker({ folder });
  await pending;
  assert.equal(disconnected, 1);
  assert.equal(launched, 0);
  for (const disposable of disposables) disposable.dispose();
});
