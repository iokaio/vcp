// SPDX-License-Identifier: Apache-2.0
// Fixture-only installed-extension smoke. No native engine or provider is used.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const vscode = require('vscode');

const canonical = value => fs.realpathSync.native(value).toLowerCase();

exports.activate = () => setImmediate(async () => {
  const input = JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT, 'utf8'));
  let api;
  try {
    assert.equal(vscode.version, input.editorVersion);
    assert.equal(vscode.env.remoteName, undefined);
    assert(!process.argv.some(arg => /extensionDevelopmentPath|extensionTestsPath/i.test(arg)),
      'Ordinary installed-extension admission must not use development overrides');
    const extension = vscode.extensions.getExtension('iokaio.vcp');
    assert(extension, 'The editor must admit the staged VCP manifest');
    assert.equal(canonical(extension.extensionPath), canonical(input.extension));
    assert.equal(extension.packageJSON.version, input.version);
    assert.equal(extension.packageJSON.engines.vscode, input.engineRange);
    const configuration = vscode.workspace.getConfiguration('vcp');
    assert.equal(configuration.inspect('engineExecutable').globalValue, undefined);
    assert.equal(configuration.inspect('dataDirectory').globalValue, undefined);
    assert.equal(vscode.workspace.getConfiguration('security.workspace.trust').get('enabled'), true);

    const sdkPath = fs.realpathSync.native(path.join(extension.extensionPath, 'node_modules/@vcp/sdk/dist/index.js'));
    assert(canonical(sdkPath).startsWith(canonical(extension.extensionPath) + path.sep));
    const sdk = await import(pathToFileURL(sdkPath).href);
    assert.equal(typeof sdk.launchLocal, 'function');
    assert.equal(typeof sdk.newCommandId, 'function');
    assert.doesNotThrow(() => JSON.parse(fs.readFileSync(path.join(extension.extensionPath,
      'node_modules/@vcp/protocol/schema.json'), 'utf8')));

    api = await extension.activate();
    for (const method of ['getConnectionState', 'getTaskState', 'getInspectorState', 'dispatchTaskMessage']) {
      assert.equal(typeof api[method], 'function', method);
    }
    assert.equal(api.getConnectionState().phase, 'disconnected');
    const registered = new Set(await vscode.commands.getCommands(true));
    const commands = extension.packageJSON.contributes.commands.map(row => row.command);
    for (const command of commands) assert(registered.has(command), command);
    const views = extension.packageJSON.contributes.views.vcp.map(row => row.id);
    for (const view of views) {
      assert(registered.has(view + '.focus'), view + '.focus');
      await vscode.commands.executeCommand(view + '.focus');
    }
    await vscode.commands.executeCommand('vcp.openSetupGuide');
    await vscode.commands.executeCommand('vcp.refreshConnection');

    const workspaceUri = vscode.Uri.file(input.workspace).toString();
    const unconfigured = await vscode.commands.executeCommand('vcp.connect', workspaceUri);
    assert.equal(unconfigured.phase, 'unavailable');
    assert.match(unconfigured.message, /absolute trusted engine executable in User settings/);
    assert.equal(unconfigured.engineExecutable, '');

    const missing = path.join(input.workspace, 'deliberately-missing-engine.exe');
    assert.equal(fs.existsSync(missing), false);
    // ConfigurationTarget.Global belongs to this smoke's isolated user-data-dir.
    await configuration.update('engineExecutable', missing, vscode.ConfigurationTarget.Global);
    const unavailable = await vscode.commands.executeCommand('vcp.connect', workspaceUri);
    assert.equal(unavailable.phase, 'unavailable');
    assert.equal(unavailable.engineExecutable, missing);
    assert.match(unavailable.message, /Connection unavailable/);
    await configuration.update('engineExecutable', undefined, vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand('vcp.disconnect');
    assert.equal(api.getConnectionState().phase, 'disconnected');
    assert.equal(configuration.inspect('engineExecutable').globalValue, undefined);
    assert.equal(fs.existsSync(missing), false);
    assert.deepEqual(fs.readdirSync(input.workspace), []);
    fs.writeFileSync(input.result, JSON.stringify({ status: 'pass', editor_version: vscode.version,
      extension_version: extension.packageJSON.version, engine_range: extension.packageJSON.engines.vscode,
      installed_manifest_admitted: true, development_overrides: false, sdk_import: true,
      command_count: commands.length, focused_views: views, setup_guide_command: true,
      unconfigured_engine: unconfigured.phase, missing_engine: unavailable.phase,
      workspace_unchanged: true, native_engine_runs: 0, provider_calls: 0,
      limitations: ['Staged source activation and refusal smoke; not a strict VSIX or native candidate qualification.',
        'View focus commands were exercised; visual rendering was not inspected.'] }, null, 2) + '\n');
  } catch (error) {
    fs.writeFileSync(input.result, JSON.stringify({ status: 'fail', editor_version: vscode.version,
      error: String(error.stack), connection: api?.getConnectionState?.() }, null, 2) + '\n');
  } finally {
    void vscode.commands.executeCommand('workbench.action.quit');
  }
});
