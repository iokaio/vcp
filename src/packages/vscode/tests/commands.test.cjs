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

test('publisher command uses an explicit native file picker and drops a cancelled selection intent', async () => {
  const platform=Object.getOwnPropertyDescriptor(process,'platform');Object.defineProperty(process,'platform',{value:'win32',configurable:true});
  try {
    const commands=new Map(), calls=[]; const noop=()=>({dispose(){}});
    const folder={name:'project',uri:{toString:()=> 'file:///C:/project',fsPath:'C:\\project'}};
    let finish;const picker=new Promise(resolve=>{finish=resolve;});
    vscode.commands={registerCommand:(id,handler)=>{commands.set(id,handler);return noop();}};
    vscode.env={};vscode.workspace={workspaceFolders:[folder],isTrusted:true,getConfiguration:()=>({inspect:key=>key==='engineExecutable'?{globalValue:'C:\\trusted\\vcp.exe',workspaceValue:'C:\\project\\evil.exe'}:{}}),onDidChangeWorkspaceFolders:noop,onDidGrantWorkspaceTrust:noop,onDidChangeConfiguration:noop};
    vscode.window={showOpenDialog:options=>{assert.equal(options.canSelectMany,false);return picker;},showErrorMessage:async()=>{}};
    const connection={state:()=>({phase:'disconnected'}),connectPublisher:async(...args)=>calls.push(args),disconnect:async()=>{}};
    const disposables=registerCommands(connection,{appendLine(){}});
    const pending=commands.get('vcp.connectPublisherController')(folder.uri.toString());await commands.get('vcp.disconnect')();finish([{scheme:'file',authority:'',fsPath:'C:\\private\\publisher.json'}]);await pending;assert.equal(calls.length,0);
    vscode.window.showOpenDialog=async()=>[{scheme:'file',authority:'',fsPath:'C:\\private\\publisher.json'}];
    await commands.get('vcp.connectPublisherController')(folder.uri.toString());assert.equal(calls.length,1);assert.equal(calls[0][1],'C:\\private\\publisher.json');assert.equal(calls[0][0].executable,'C:\\trusted\\vcp.exe');assert.equal(calls[0][0].publisher,undefined);
    let prompted=0;vscode.workspace.isTrusted=false;vscode.window.showOpenDialog=async()=>{prompted++;return[];};await commands.get('vcp.connectPublisherController')(folder.uri.toString());assert.equal(prompted,0);assert.equal(calls.length,1);
    for(const disposable of disposables)disposable.dispose();
  } finally { Object.defineProperty(process,'platform',platform); }
});
