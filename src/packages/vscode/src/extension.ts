// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { EngineConnection, type ConnectionStatus } from './engine_connection.js';
import { ConnectionView } from './connection_view.js';
import { registerCommands } from './commands.js';

let active: EngineConnection | undefined;
export interface ExtensionApi { getConnectionState(): ConnectionStatus }

export function activate(context: vscode.ExtensionContext): ExtensionApi {
  const output = vscode.window.createOutputChannel('VCP Connection');
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 30);
  statusBar.command = 'vcp.connection.focus';
  statusBar.name = 'VCP read-only connection';
  let view: ConnectionView | undefined;
  const connection = new EngineConnection({
    // Preserve ESM loading and JSON import attributes inside the actual SDK.
    launch: async options => (await import('@vcp/sdk')).launchLocal(options),
    publish: status => {
      statusBar.text = status.phase === 'connected' ? '$(eye) VCP: observer' : status.phase === 'connecting' ? '$(sync~spin) VCP: connecting' : '$(plug) VCP: disconnected';
      statusBar.tooltip = status.message;
      view?.publish(status);
    },
  });
  active = connection;
  const commands = { connect: 'vcp.connect', disconnect: 'vcp.disconnect', refresh: 'vcp.refreshConnection' } as const;
  view = new ConnectionView(context.extensionUri, connection.state(), action => Promise.resolve(vscode.commands.executeCommand(commands[action])));
  context.subscriptions.push(output, statusBar, view, vscode.window.registerWebviewViewProvider('vcp.connection', view), ...registerCommands(connection, output));
  statusBar.text = '$(plug) VCP: disconnected';
  statusBar.tooltip = 'Connect explicitly to inspect an initialized local VCP workspace.';
  statusBar.show();
  // Activation and webview restoration never launch the engine.
  return Object.freeze({ getConnectionState: () => connection.state() });
}

export async function deactivate(): Promise<void> {
  const connection = active;
  active = undefined;
  await connection?.dispose();
}
