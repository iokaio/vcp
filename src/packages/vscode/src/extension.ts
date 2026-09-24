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
import { InspectorPanel } from './inspector_panel.js';
import { InspectorSession } from './inspector_session.js';
import type { InspectorState } from './inspector_view_model.js';
import { InspectorActions } from './inspector_actions.js';
import { InspectorJournal } from './inspector_journal.js';

let active: EngineConnection | undefined;
let activeTasks: TaskSession | undefined;
let activeEditor: EditorWorkflow | undefined;
let activeInspectors: InspectorSession | undefined;
export interface ExtensionApi { getConnectionState(): ConnectionStatus; getTaskState(): TaskPanelState | undefined; getInspectorState(): InspectorState | undefined; dispatchTaskMessage(message: unknown): Promise<void> }

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
  let inspectorPanel: InspectorPanel | undefined;
  let inspectorActions: InspectorActions | undefined;
  const inspectorProfiles = new Map<string, InspectorActions>();
  const reconciledInspectorClients = new WeakSet<object>();
  const inspectors = new InspectorSession({
    publish: state => inspectorPanel?.publish(state),
    prompt: async input => vscode.window.showInputBox({ title: input.title, ignoreFocusOut: true,
      validateInput: value => Buffer.byteLength(value, 'utf8') > input.maxLength || value.includes('\0') ? 'Query exceeds the allowed bound.' : undefined }),
    actions: {
      register: value => inspectorActions?.register(value) ?? [],
      dispatch: id => inspectorActions?.dispatch(id) ?? Promise.resolve(),
      records: () => inspectorActions?.records() ?? [],
      invalidate: () => inspectorActions?.invalidate(),
    },
  });
  activeInspectors = inspectors;
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
        inspectorActions?.invalidate();
        inspectorActions = inspectorProfiles.get(selectedProfile);
        if (!inspectorActions) {
          const key = `vcp.inspectorCommands.v1.${selectedProfile}`;
          inspectorActions = new InspectorActions({
            current: () => connection.profileKey() === selectedProfile ? inspectors.current() : undefined,
            session: () => inspectors,
            prompt: (title, value) => Promise.resolve(vscode.window.showInputBox({title,...(value === undefined ? {} : {value}),ignoreFocusOut:true,validateInput: text => text.length > 256 || text.includes('\0') ? 'Input exceeds its allowed bound.' : undefined})),
            choose: (title, choices) => Promise.resolve(vscode.window.showQuickPick([...choices],{title,ignoreFocusOut:true})),
            confirm: async message => (await vscode.window.showWarningMessage(message,{modal:true},'Confirm')) === 'Confirm',
          },new InspectorJournal(async records => { await context.workspaceState.update(key,records); },context.workspaceState.get<unknown>(key)));
          inspectorProfiles.set(selectedProfile,inspectorActions);
        }
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
        tasks = new TaskSession({ actions, publish: state => taskPanel?.publish(state), selection: value => inspectors.selection(value) });
        activeTasks = tasks;
      }
      tasks?.connection(status, client);
      inspectors.connection(status, client);
      if (client && inspectorActions && !reconciledInspectorClients.has(client)) {
        reconciledInspectorClients.add(client);
        const inspected = inspectors.current();
        // Recovery reads only the original identities; it never replays commands.
        void inspectorActions.journal.reconcile(client.scope, client.call.bind(client)).then(() => {
          // A delayed receipt read must not replace a newer query or exact review.
          if (connection.currentClient() === client && inspectors.current() === inspected) return inspectors.refresh();
        }).catch(() => {});
      }
    },
  });
  active = connection;
  const commands = { connect: 'vcp.connect', control: 'vcp.connectController', attach: 'vcp.attachObserver', reconcile: 'vcp.reconcileRoot', grant: 'vcp.grantTrust', revoke: 'vcp.revokeTrust', disconnect: 'vcp.disconnect', refresh: 'vcp.refreshConnection' } as const;
  view = new ConnectionView(context.extensionUri, connection.state(), action => Promise.resolve(vscode.commands.executeCommand(commands[action])));
  taskPanel = new TaskPanel(context.extensionUri, async message => { await tasks?.dispatch(message); });
  inspectorPanel = new InspectorPanel(context.extensionUri, message => inspectors.dispatch(message), visible => inspectors.visible(visible));
  context.subscriptions.push(inspectorPanel, inspectors, vscode.window.registerWebviewViewProvider('vcp.inspectors', inspectorPanel));
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
  return Object.freeze({ getConnectionState: () => connection.state(), getTaskState: () => tasks?.state(), getInspectorState: () => inspectors.state(), dispatchTaskMessage: async (raw: unknown) => { const message = parseTaskPanelMessage(raw); if (message) await tasks?.dispatch(message); } });
}

export async function deactivate(): Promise<void> {
  activeInspectors?.dispose(); activeInspectors = undefined;
  activeEditor?.dispose(); activeEditor = undefined;
  activeTasks?.dispose(); activeTasks = undefined;
  const connection = active;
  active = undefined;
  await connection?.dispose();
}
