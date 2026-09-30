// SPDX-License-Identifier: Apache-2.0
'use strict';
// A separate private test extension. Neither this driver nor package-host.cjs is
// installed into the shipping VCP extension or allowed to replace engine replies.
const fs = require('node:fs');
const vscode = require('vscode');
exports.activate = () => setImmediate(async () => {
  let input;
  try {
    input = JSON.parse(fs.readFileSync(process.env.VCP_EXTENSION_TEST_INPUT, 'utf8'));
    if (!['install', 'restart', 'failed-update', 'missing', 'reconnect'].includes(input.mode)) throw Error('Unknown lifecycle observation');
    await require('./package-host.cjs').run();
    const result = JSON.parse(fs.readFileSync(input.result, 'utf8'));
    if (result.ok !== true) throw Error('Lifecycle observation failed');
    // package-host asserts observer identity/scope before every successful
    // non-missing result and checks the restricted PATH before every mode.
    result.observer = input.mode !== 'missing';
    result.developmentPathAbsent = true;
    result.extensionHostPid = process.pid;
    if (input.mode === 'install') result.beforeReloadHostPid = JSON.parse(fs.readFileSync(input.reloadMarker, 'utf8')).pid;
    fs.writeFileSync(input.result, JSON.stringify(result));
  } catch (error) {
    if (input) fs.writeFileSync(input.result, JSON.stringify({ok: false, error: String(error.stack)}));
  } finally {
    void vscode.commands.executeCommand('workbench.action.quit');
  }
});
