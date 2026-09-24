// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { EngineConnection, type ConnectionStatus } from './engine_connection.js';
import { ConnectionView } from './connection_view.js';
import { registerCommands, selectionForFolder } from './commands.js';
import { recovery } from './recovery.js';

let active: EngineConnection | undefined;
export interface ExtensionApi { getConnectionState(): ConnectionStatus }

export function activate(context: vscode.ExtensionContext): ExtensionApi {
  const output = vscode.window.createOutputChannel('VCP Connection');
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 30);
  statusBar.command = 'vcp.connection.focus';
  statusBar.name = 'VCP connection';
  const recoveryKey = 'vcp.observerRecovery.v1';
  const saved = recovery(context.workspaceState.get<unknown>(recoveryKey));
  let view: ConnectionView | undefined;
  const connection = new EngineConnection({
    // Preserve ESM loading and JSON import attributes inside the actual SDK.
    launch: async options => (await import('@vcp/sdk')).launchLocal(options),
    attach: async options => (await import('@vcp/sdk')).attachLocal(options),
    reconnect: async options => (await import('@vcp/sdk')).reconnectObserverLocal(options),
    rebind: async options => (await import('@vcp/sdk')).rebindLocal(options),
    saveRecovery: async value => { await context.workspaceState.update(recoveryKey, value); },
    publish: status => {
      statusBar.text = status.phase === 'connected' ? `$(eye) VCP: ${status.role}` : status.phase === 'connecting' ? '$(sync~spin) VCP: connecting' : '$(plug) VCP: disconnected';
      statusBar.tooltip = status.message;
      view?.publish(status);
    },
  });
  active = connection;
  const commands = { connect: 'vcp.connect', control: 'vcp.connectController', attach: 'vcp.attachObserver', reconcile: 'vcp.reconcileRoot', grant: 'vcp.grantTrust', revoke: 'vcp.revokeTrust', disconnect: 'vcp.disconnect', refresh: 'vcp.refreshConnection' } as const;
  view = new ConnectionView(context.extensionUri, connection.state(), action => Promise.resolve(vscode.commands.executeCommand(commands[action])));
  context.subscriptions.push(output, statusBar, view, vscode.window.registerWebviewViewProvider('vcp.connection', view), ...registerCommands(connection, output, () => recovery(context.workspaceState.get<unknown>(recoveryKey))));
  statusBar.text = '$(plug) VCP: disconnected';
  statusBar.tooltip = 'Connect explicitly to inspect an initialized local VCP workspace.';
  statusBar.show();
  // Reload can only attach to the authenticated live engine; it never launches
  // another writer, acquires control or replays a pending user command.
  const folder = saved && vscode.workspace.workspaceFolders?.find(candidate => candidate.uri.toString() === saved.folderUri);
  if (saved && folder) void connection.restore(selectionForFolder(folder), saved).catch(() => {});
  return Object.freeze({ getConnectionState: () => connection.state() });
}

export async function deactivate(): Promise<void> {
  const connection = active;
  active = undefined;
  await connection?.dispose();
}
