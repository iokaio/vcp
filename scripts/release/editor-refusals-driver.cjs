// SPDX-License-Identifier: Apache-2.0
'use strict';
// A separate installed test extension. Only prompt answers and call observation
// are instrumented; production authority, RPC results and buffer APIs stay real.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {createRequire} = require('node:module'), {createHash} = require('node:crypto');
const vscode = require('vscode');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
exports.activate = () => setImmediate(async () => {
  let input, api, phase = 'activate';
  const restores = [], calls = [], errors = [];
  const replace = (object, key, value) => { const old = object[key]; object[key] = value; assert.equal(object[key], value); restores.push(() => { object[key] = old; }); };
  const wait = async (read, label) => { for (let i = 0; i < 400; i++) { const result = read(); if (result) return result; await delay(50); } throw Error(`Deadline: ${label}`); };
  try {
    input = JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT, 'utf8'));
    assert(['restricted', 'trusted'].includes(input.mode));
    const normalize = file => path.resolve(file).toLowerCase();
    const allowed = new Set([input.editorRuntime, process.env.SystemRoot, path.join(process.env.SystemRoot, 'System32')].map(normalize));
    assert(process.env.PATH.split(path.delimiter).every(entry => entry && allowed.has(normalize(entry))));
    const extension = vscode.extensions.getExtension('vcp.vcp-local'); assert(extension);
    assert.equal(extension.packageJSON.version, input.version);
    assert(!normalize(extension.extensionPath).startsWith(normalize(input.checkout) + path.sep));
    assert.equal(vscode.workspace.getConfiguration('security.workspace.trust').get('enabled'), true);
    assert.equal(vscode.workspace.isTrusted, input.mode === 'trusted', 'Actual editor trust must match the private profile');
    const target = createRequire(path.join(extension.extensionPath, 'dist/extension.js'))('vscode');
    replace(target.window, 'showQuickPick', async (items, options) => options?.title === 'Review editor drafts' ? await items : undefined);
    replace(target.window, 'showErrorMessage', async message => { errors.push(String(message)); });
    replace(target.window, 'showInformationMessage', async () => undefined);
    api = await extension.activate();
    const expected = fs.realpathSync(path.join(extension.extensionPath, 'dist/engine_connection.js')).toLowerCase();
    const module = Object.values(require.cache).find(value => value.filename && path.basename(value.filename) === 'engine_connection.js' && fs.realpathSync(value.filename).toLowerCase() === expected);
    assert(module, 'Actual installed connection module must be loaded');
    const prototype = module.exports.EngineConnection.prototype, original = prototype.currentClient, wrapped = new Set();
    replace(prototype, 'currentClient', function () {
      const client = original.call(this);
      if (client && !wrapped.has(client)) {
        wrapped.add(client);
        assert(!client.initialized.methods.includes('turn/start') && !client.initialized.methods.includes('session/resume'), 'Fixture must remain an ordinary attachment');
        const call = client.call;
        replace(client, 'call', async function (method, ...args) {
          const observed = {method, outcome: 'pending'}; calls.push(observed);
          try { const result = await call.call(this, method, ...args); observed.outcome = 'accepted'; return result; }
          catch (error) {
            observed.outcome = error?.code === 'rpc' && error.classification?.applicationCode === 'VERSION_CONFLICT'
              && error.classification?.retry === 'after_revalidation' ? 'refused' : 'failed';
            throw error;
          }
        });
      }
      return client;
    });
    const config = target.workspace.getConfiguration('vcp');
    const settle = () => wait(() => api.getConnectionState().phase !== 'connecting', 'connection settled');
    const paths = async data => {
      await config.update('engineExecutable', input.executable, vscode.ConfigurationTarget.Global);
      await config.update('dataDirectory', data, vscode.ConfigurationTarget.Global);
      await settle();
    };
    const connect = async (role, workspace = input.workspace) => {
      const state = await vscode.commands.executeCommand(role === 'controller' ? 'vcp.connectController' : 'vcp.connect', vscode.Uri.file(workspace).toString());
      assert.equal(state.phase, 'connected', JSON.stringify(state)); assert.equal(state.role, role); assert.deepEqual(state.scope, input.scope);
      const view = await wait(() => api.getTaskState()?.phase === 'current' && api.getTaskState().rows.some(row => row.task === input.task) ? api.getTaskState() : undefined, 'paused history');
      const row = view.rows.find(row => row.task === input.task); assert.equal(row.state, 'paused');
      await api.dispatchTaskMessage({action: 'select', id: row.actionId});
      await wait(() => api.getTaskState()?.detail?.task.task === input.task, 'selected paused task');
      return state;
    };
    const open = async file => {
      const document = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
      await vscode.window.showTextDocument(document, {preview: false});
      return document;
    };
    // Draft invalidation may legitimately refresh context in the background.
    // This paused fixture has no tool policy; those real requests must refuse.
    // Never suppress refresh timers or fabricate an RPC result to pass a case.
    const forbiddenCalls = () => calls.filter(({method}) => ['editor/prepare', 'editor/dispatch', 'editor/changeResult', 'turn/start', 'session/resume'].includes(method));
    await paths(input.data);
    const observer = await connect('observer'); assert.equal(observer.engineTrust, 'untrusted');
    const observations = {};
    if (input.mode === 'restricted') {
      phase = 'restricted trust and edits';
      const controlled = await connect('controller'); assert.equal(controlled.editorTrusted, false);
      const priorErrors = errors.length;
      const grant = await vscode.commands.executeCommand('vcp.grantTrust');
      assert.equal(grant.engineTrust, 'untrusted'); assert.equal(grant.workspaceRevision, controlled.workspaceRevision);
      assert(errors.length > priorErrors, 'Restricted trust grant must report refusal');
      const document = await open(path.join(input.workspace, 'typing.txt'));
      for (const command of ['vcp.createEditDraft', 'vcp.reviewEditorDrafts', 'vcp.applyEditorChanges']) {
        const before = errors.length; await vscode.commands.executeCommand(command);
        assert(errors.length > before, `${command} must report restricted refusal`);
      }
      assert.equal(document.getText(), input.sourceText); assert.equal(document.isDirty, false);
      observations.restricted = {actualTrusted: false, grantRefused: true, editsRefused: 3, workspaceRevision: grant.workspaceRevision};
      await vscode.commands.executeCommand('vcp.disconnect');
      phase = 'uninitialized selected folder';
      const uninitialized = await vscode.commands.executeCommand('vcp.connect', vscode.Uri.file(input.uninitialized).toString());
      assert.equal(uninitialized.phase, 'unavailable'); assert.equal(uninitialized.scope, undefined);
      observations.uninitialized = {phase: uninitialized.phase, connected: false};
      await vscode.commands.executeCommand('vcp.disconnect');
      phase = 'wrong explicit data directory'; await paths(input.wrongData);
      const wrongData = await vscode.commands.executeCommand('vcp.connect', vscode.Uri.file(input.workspace).toString());
      assert.equal(wrongData.phase, 'unavailable'); assert.equal(wrongData.scope, undefined);
      observations.wrongData = {phase: wrongData.phase, connected: false};
      await vscode.commands.executeCommand('vcp.disconnect');
      phase = 'folder not selected in editor';
      const generation = api.getConnectionState().generation;
      await vscode.commands.executeCommand('vcp.connect', vscode.Uri.file(input.outside).toString());
      assert.equal(api.getConnectionState().phase, 'disconnected'); assert.equal(api.getConnectionState().generation, generation);
      observations.unselected = {phase: 'disconnected', generationUnchanged: true};
    } else {
      phase = 'explicit native trust grant';
      const controlled = await connect('controller'); assert.equal(controlled.editorTrusted, true);
      const granted = await vscode.commands.executeCommand('vcp.grantTrust');
      assert.equal(granted.engineTrust, 'trusted'); assert.equal(granted.role, 'observer');
      assert.equal(BigInt(granted.workspaceRevision), BigInt(controlled.workspaceRevision) + 1n);
      await connect('controller');
      observations.trust = {granted: true, returnedToObserver: true, explicitControllerReacquired: true};
      phase = 'outside selected source root';
      const outside = await open(path.join(input.outside, 'outside.txt'));
      const before = errors.length; await vscode.commands.executeCommand('vcp.createEditDraft');
      assert(errors.length > before); assert.equal(outside.isDirty, false);
      observations.outsideSource = {refused: true, diskSha256: hash(fs.readFileSync(outside.uri.fsPath))};
      observations.drafts = [];
      for (const action of ['typing', 'undo', 'reopen']) {
        phase = `actual draft ${action}`;
        const source = await open(path.join(input.workspace, `${action}.txt`));
        const disk = hash(fs.readFileSync(source.uri.fsPath));
        if (action !== 'reopen') {
          const editor = await vscode.window.showTextDocument(source, {preview: false}); editor.selection = new vscode.Selection(0, 0, 0, 0);
          // Establish an explicit real undo boundary around the pre-existing
          // dirty text, so undo below targets only the later typing operation.
          assert.equal(await editor.edit(builder => builder.insert(new vscode.Position(0, 0), 'human '), {undoStopBefore: true, undoStopAfter: true}), true);
          await wait(() => source.getText() === 'human ' + input.sourceText && source.isDirty, 'real unsaved source edit');
        }
        const capturedText = source.getText(), capturedVersion = source.version, capturedDirty = source.isDirty;
        await vscode.commands.executeCommand('vcp.createEditDraft');
        const draftEditor = vscode.window.activeTextEditor; assert(draftEditor.document.isUntitled);
        const draft = draftEditor.document; assert.equal(draft.getText(), capturedText);
        assert.equal(await draftEditor.edit(builder => builder.replace(new vscode.Range(draft.positionAt(0), draft.positionAt(draft.getText().length)), 'candidate change\n')), true);
        let current = source;
        const editor = await vscode.window.showTextDocument(source, {preview: false});
        if (action === 'reopen') {
          await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
          await wait(() => source.isClosed, 'actual source close'); current = await open(source.uri.fsPath); assert.notEqual(current, source);
        } else {
          editor.selection = new vscode.Selection(0, 0, 0, 0);
          await vscode.commands.executeCommand('type', {text: 'later '});
          await wait(() => source.version > capturedVersion && source.getText() === 'later ' + capturedText, 'later real typing');
          if (action === 'undo') {
            await vscode.commands.executeCommand('undo'); await wait(() => source.getText() === capturedText, 'actual undo restored dirty text');
            assert(source.version > capturedVersion); assert(source.isDirty);
          }
        }
        const preservedText = current.getText(), preservedVersion = current.version, errorCount = errors.length;
        await vscode.commands.executeCommand('vcp.reviewEditorDrafts');
        assert(errors.length > errorCount, 'Changed source draft must be refused');
        assert.equal(current.getText(), preservedText); assert.equal(current.version, preservedVersion);
        assert.equal(hash(fs.readFileSync(source.uri.fsPath)), disk); assert.equal(forbiddenCalls().length, 0);
        observations.drafts.push({action, capturedDirty, capturedVersion, afterVersion: current.version, reopened: current !== source,
          capturedSha256: hash(capturedText), afterSha256: hash(current.getText()), diskBeforeSha256: disk, diskAfterSha256: disk,
          reviewRefused: true, bufferPreserved: true, dirtyAfter: current.isDirty});
        await vscode.window.showTextDocument(draft, {preview: false}); await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
        assert(draft.isClosed);
      }
      // Explicit fixture cleanup happens only after observations were recorded.
      for (const document of vscode.workspace.textDocuments.filter(document => document.uri.scheme === 'file' && document.isDirty)) {
        await vscode.window.showTextDocument(document, {preview: false}); await vscode.commands.executeCommand('workbench.action.files.revert');
      }
      // Let ordinary refresh work report its real RPC outcome before explicitly
      // disconnecting. Transport loss is never counted as a native refusal.
      await delay(300); await wait(() => calls.every(call => call.outcome !== 'pending'), 'background context completion');
      await vscode.commands.executeCommand('vcp.disconnect');
    }
    assert.equal(forbiddenCalls().length, 0, 'No editor mutation or execution-start RPC may be attempted');
    phase = 'final explicit observer'; await paths(input.data); const final = await connect('observer');
    assert.equal(final.engineTrust, input.mode === 'trusted' ? 'trusted' : 'untrusted');
    await vscode.commands.executeCommand('vcp.disconnect');
    await wait(() => calls.every(call => call.outcome !== 'pending'), 'actual RPC completion');
    const contexts = calls.filter(call => call.method === 'editor/context');
    assert(contexts.every(call => call.outcome === 'refused'), 'Offline fixture must not accept a buffer observation');
    fs.writeFileSync(input.result, JSON.stringify({ok: true, mode: input.mode, version: extension.packageJSON.version,
      scope: input.scope, task: input.task, extensionHostPid: process.pid, actualTrusted: vscode.workspace.isTrusted,
      developmentPathAbsent: true, finalObserver: true, ordinaryAttachment: wrapped.size > 0,
      forbiddenRpcCount: forbiddenCalls().length, contextAttempts: contexts.length, contextRefused: contexts.length,
      contextAccepted: 0, observations, driverSha256: hash(fs.readFileSync(__filename))}));
  } catch (error) {
    if (input) fs.writeFileSync(input.result, JSON.stringify({ok: false, phase, error: String(error.stack), errors, calls, state: api?.getConnectionState()}));
  } finally {
    for (const restore of restores.reverse()) restore();
    if (api) await vscode.commands.executeCommand('vcp.disconnect').catch(() => {});
    void vscode.commands.executeCommand('workbench.action.quit');
  }
});
