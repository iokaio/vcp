// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { EngineConnection, type ConnectionStatus, type Selection } from './engine_connection.js';
import { safeFailure } from './diagnostics.js';
import { profile, type Recovery } from './recovery.js';

/** Read inspected User values only; effective merged workspace values are unsafe. */
export function globalPaths(configuration: Pick<vscode.WorkspaceConfiguration, 'inspect'>): { executable: string; dataPath?: string } {
  const executable: unknown = configuration.inspect<string>('engineExecutable')?.globalValue;
  const data: unknown = configuration.inspect<string>('dataDirectory')?.globalValue;
  return { executable: typeof executable === 'string' ? executable : '', ...(typeof data === 'string' && data !== '' ? { dataPath: data } : {}) };
}
export function selectionForFolder(folder: vscode.WorkspaceFolder): Selection {
  const paths = globalPaths(vscode.workspace.getConfiguration('vcp'));
  return { workspaceUri: folder.uri.toString(), workspacePath: folder.uri.fsPath, executable: paths.executable, executableSource: 'global', ...(paths.dataPath === undefined ? {} : { dataPath: paths.dataPath }), workspaceTrusted: vscode.workspace.isTrusted, ...(vscode.env.remoteName === undefined ? {} : { remoteName: vscode.env.remoteName }) };
}
export function registerCommands(connection: EngineConnection, output: vscode.OutputChannel, saved: () => Recovery | undefined = () => undefined): vscode.Disposable[] {
  let selecting = false;
  let intent = 0;
  const report = async (operation: () => Promise<ConnectionStatus | void>): Promise<ConnectionStatus> => {
    try { await operation(); }
    catch (error) { const message = safeFailure(error); output.appendLine(message); void vscode.window.showErrorMessage(message); }
    return connection.state();
  };
  const select = async (folderUri: unknown, action: (selection: Selection) => Promise<unknown>): Promise<ConnectionStatus> => {
    if (selecting) return connection.state();
    selecting = true;
    const selectedIntent = ++intent;
    try {
      return await report(async () => {
        const folders = vscode.workspace.workspaceFolders ?? [];
        let folder: vscode.WorkspaceFolder | undefined;
        if (folderUri !== undefined) {
          if (typeof folderUri !== 'string') throw { code: 'INVALID_ARGUMENT' };
          folder = folders.find(candidate => candidate.uri.toString() === folderUri);
        } else {
          const choice = await vscode.window.showQuickPick(folders.map(candidate => ({ label: candidate.name, description: candidate.uri.toString(), folder: candidate })), { title: 'Select VCP workspace folder', placeHolder: 'Choose by full URI; names are not identities', ignoreFocusOut: true });
          if (!choice || selectedIntent !== intent) return;
          folder = vscode.workspace.workspaceFolders?.find(candidate => candidate.uri.toString() === choice.folder.uri.toString());
        }
        if (!folder) throw { code: 'INVALID_ARGUMENT' };
        if (selectedIntent !== intent) return;
        await action(selectionForFolder(folder));
      });
    } finally { selecting = false; }
  };
  const connect = vscode.commands.registerCommand('vcp.connect', (uri?: unknown) => select(uri, selection => connection.connect(selection)));
  const control = vscode.commands.registerCommand('vcp.connectController', (uri?: unknown) => select(uri, selection => connection.connect(selection, 'controller')));
  const attach = vscode.commands.registerCommand('vcp.attachObserver', (uri?: unknown, reference?: unknown) => select(uri, async selection => {
    const selectedIntent = intent;
    const input = reference ?? await vscode.window.showInputBox({ title: 'Observe an existing VCP engine', prompt: 'Paste its non-secret observer reconnection reference. Controller credentials are not accepted.', ignoreFocusOut: true });
    if (input === undefined || selectedIntent !== intent) return;
    const parsed: unknown = typeof input === 'string' ? JSON.parse(input) : input;
    // SDK/native code validates the full untrusted reference before attaching.
    await connection.restore(selection, { folderUri: selection.workspaceUri, profile: profile(selection), reference: parsed as Recovery['reference'] });
  }));
  const reconcile = vscode.commands.registerCommand('vcp.reconcileRoot', (uri?: unknown, workspace?: unknown) => select(uri, async selection => {
    const selectedIntent = intent;
    const value = workspace ?? await vscode.window.showInputBox({ title: 'Reconcile a moved VCP root', prompt: 'Existing workspace ID. Close its owner first; history is retained, trust resets, and tasks remain paused.', value: connection.state().scope?.workspace ?? saved()?.reference.scope.workspace ?? '', ignoreFocusOut: true });
    if (value === undefined || selectedIntent !== intent) return;
    if (typeof value !== 'string' || !/^[0-9a-fA-F-]{36}$/.test(value)) throw { code: 'INVALID_ARGUMENT' };
    await connection.reconcile(selection, value);
  }));
  const grant = vscode.commands.registerCommand('vcp.grantTrust', () => report(() => connection.setTrust(true)));
  const revoke = vscode.commands.registerCommand('vcp.revokeTrust', () => report(() => connection.setTrust(false)));
  const refresh = vscode.commands.registerCommand('vcp.refreshConnection', () => report(async () => {
    const status = connection.state();
    if (status.workspaceUri !== undefined && !vscode.workspace.workspaceFolders?.some(folder => folder.uri.toString() === status.workspaceUri)) {
      await connection.invalidate('Workspace folders changed; reconcile moved roots explicitly.', vscode.workspace.isTrusted, true);
      return;
    }
    if (status.editorTrusted !== vscode.workspace.isTrusted) await connection.editorTrustChanged(vscode.workspace.isTrusted);
    await connection.refresh();
  }));
  const disconnect = vscode.commands.registerCommand('vcp.disconnect', () => { intent++; return report(() => connection.disconnect()); });
  const folders = vscode.workspace.onDidChangeWorkspaceFolders(() => { intent++; void connection.invalidate('Workspace folders changed; reconcile moved roots explicitly.', vscode.workspace.isTrusted, true).catch(() => {}); });
  const trust = vscode.workspace.onDidGrantWorkspaceTrust(() => { intent++; void connection.editorTrustChanged(vscode.workspace.isTrusted).catch(() => {}); });
  const configuration = vscode.workspace.onDidChangeConfiguration(event => {
    if (event.affectsConfiguration('vcp.engineExecutable') || event.affectsConfiguration('vcp.dataDirectory')) { intent++; void connection.invalidate('Connection settings changed; reconnect explicitly.', vscode.workspace.isTrusted).catch(() => {}); }
  });
  return [connect, control, attach, reconcile, grant, revoke, refresh, disconnect, folders, trust, configuration, { dispose: () => { intent++; } }];
}
