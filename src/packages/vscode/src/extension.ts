// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { EngineConnection, type ConnectionStatus } from './engine_connection.js';
import { ConnectionView } from './connection_view.js';
import { registerCommands, selectionForFolder } from './commands.js';
import { recovery } from './recovery.js';
import { TaskActions } from './task_actions.js';
import { TaskSession } from './task_session.js';
import { TaskPanel } from './task_panel.js';
import { parseTaskPanelMessage, type TaskPanelState } from './task_view_model.js';
import { EditorWorkflow } from './editor_workflow.js';

let active: EngineConnection | undefined;
let activeTasks: TaskSession | undefined;
let activeEditor: EditorWorkflow | undefined;
export interface ExtensionApi { getConnectionState(): ConnectionStatus; getTaskState(): TaskPanelState | undefined; dispatchTaskMessage(message: unknown): Promise<void> }

export function activate(context: vscode.ExtensionContext): ExtensionApi {
  const output = vscode.window.createOutputChannel('VCP Connection');
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 30);
  statusBar.command = 'vcp.connection.focus';
  statusBar.name = 'VCP connection';
  const recoveryKey = 'vcp.observerRecovery.v1';
  const saved = recovery(context.workspaceState.get<unknown>(recoveryKey));
  let view: ConnectionView | undefined;
  let taskPanel: TaskPanel | undefined;
  let tasks: TaskSession | undefined;
  let taskProfile: string | undefined;
  let editorWorkflow: EditorWorkflow | undefined;
  // A late command receipt still owns its profile's journal after navigation.
  // Reuse that writer on A→B→A rather than creating competing saved snapshots.
  const actionProfiles = new Map<string, TaskActions>();
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
      editorWorkflow?.connectionChanged(status);
      const client = connection.currentClient(); const selectedProfile = connection.profileKey();
      if (client && selectedProfile && selectedProfile !== taskProfile) {
        tasks?.dispose(); taskProfile = selectedProfile;
        const journalKey = `vcp.taskCommands.v1.${selectedProfile}`;
        let actions = actionProfiles.get(selectedProfile);
        if (!actions) {
          actions = new TaskActions({
            current: () => {
              const current = connection.currentClient(); const state = connection.state();
              if (!current || connection.profileKey() !== selectedProfile) return undefined;
              return { generation: state.generation, scope: current.scope, controller: state.role === 'controller', trusted: vscode.workspace.isTrusted && state.engineTrust === 'trusted', capabilities: [...current.initialized.methods, ...current.initialized.capabilities], call: current.call.bind(current) };
            },
            save: async records => { await context.workspaceState.update(journalKey, records); },
            publish: () => { if (connection.profileKey() === selectedProfile) tasks?.notifyActions(); },
            promptSteering: async () => vscode.window.showInputBox({ title: 'Update task guidance', prompt: 'Enter the new objective. Existing constraints and acceptance criteria are preserved. Sending guidance does not resume paused work.', ignoreFocusOut: true }),
          }, context.workspaceState.get<unknown>(journalKey));
          actionProfiles.set(selectedProfile, actions);
        }
        tasks = new TaskSession({ actions, publish: state => taskPanel?.publish(state) });
        activeTasks = tasks;
      }
      tasks?.connection(status, client);
    },
  });
  active = connection;
  const commands = { connect: 'vcp.connect', control: 'vcp.connectController', attach: 'vcp.attachObserver', reconcile: 'vcp.reconcileRoot', grant: 'vcp.grantTrust', revoke: 'vcp.revokeTrust', disconnect: 'vcp.disconnect', refresh: 'vcp.refreshConnection' } as const;
  view = new ConnectionView(context.extensionUri, connection.state(), action => Promise.resolve(vscode.commands.executeCommand(commands[action])));
  taskPanel = new TaskPanel(context.extensionUri, async message => { await tasks?.dispatch(message); });
  editorWorkflow = new EditorWorkflow(connection, context, () => tasks?.state());
  activeEditor = editorWorkflow;
  context.subscriptions.push(editorWorkflow, ...editorWorkflow.registrations());
  context.subscriptions.push(output, statusBar, view, taskPanel, vscode.window.registerWebviewViewProvider('vcp.connection', view), vscode.window.registerWebviewViewProvider('vcp.tasks', taskPanel), ...registerCommands(connection, output, () => recovery(context.workspaceState.get<unknown>(recoveryKey))));
  statusBar.text = '$(plug) VCP: disconnected';
  statusBar.tooltip = 'Connect explicitly to inspect an initialized local VCP workspace.';
  statusBar.show();
  // Reload can only attach to the authenticated live engine; it never launches
  // another writer, acquires control or replays a pending user command.
  const folder = saved && vscode.workspace.workspaceFolders?.find(candidate => candidate.uri.toString() === saved.folderUri);
  if (saved && folder) void connection.restore(selectionForFolder(folder), saved).catch(() => {});
  return Object.freeze({ getConnectionState: () => connection.state(), getTaskState: () => tasks?.state(), dispatchTaskMessage: async (raw: unknown) => { const message = parseTaskPanelMessage(raw); if (message) await tasks?.dispatch(message); } });
}

export async function deactivate(): Promise<void> {
  activeEditor?.dispose(); activeEditor = undefined;
  activeTasks?.dispose(); activeTasks = undefined;
  const connection = active;
  active = undefined;
  await connection?.dispose();
}
