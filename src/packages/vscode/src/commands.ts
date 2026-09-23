// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { EngineConnection, type ConnectionStatus } from './engine_connection.js';
import { safeFailure } from './diagnostics.js';

/** Read inspected User values only; effective merged workspace values are unsafe. */
export function globalPaths(configuration: Pick<vscode.WorkspaceConfiguration, 'inspect'>): { executable: string; dataPath?: string } {
  const executable: unknown = configuration.inspect<string>('engineExecutable')?.globalValue;
  const data: unknown = configuration.inspect<string>('dataDirectory')?.globalValue;
  return {
    executable: typeof executable === 'string' ? executable : '',
    ...(typeof data === 'string' && data !== '' ? { dataPath: data } : {}),
  };
}

export function registerCommands(connection: EngineConnection, output: vscode.OutputChannel): vscode.Disposable[] {
  let selecting = false;
  let intent = 0;
  const report = async (operation: () => Promise<ConnectionStatus | void>): Promise<ConnectionStatus> => {
    try { await operation(); }
    catch (error) { const message = safeFailure(error); output.appendLine(message); void vscode.window.showErrorMessage(message); }
    return connection.state();
  };
  const connect = vscode.commands.registerCommand('vcp.connect', async (folderUri?: unknown) => {
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
          if (!folder) throw { code: 'INVALID_ARGUMENT' };
        } else {
          const choice = await vscode.window.showQuickPick(folders.map(candidate => ({ label: candidate.name, description: candidate.uri.toString(), folder: candidate })), { title: 'Connect to an initialized VCP workspace', placeHolder: 'Choose a folder by its full URI; names are not identities', ignoreFocusOut: true });
          if (!choice) return;
          if (selectedIntent !== intent) return;
          // The workspace may have changed while the picker was open.
          folder = vscode.workspace.workspaceFolders?.find(candidate => candidate.uri.toString() === choice.folder.uri.toString());
          if (!folder) throw { code: 'INVALID_ARGUMENT' };
        }
        const paths = globalPaths(vscode.workspace.getConfiguration('vcp'));
        if (selectedIntent !== intent) return;
        await connection.connect({
          workspaceUri: folder.uri.toString(), workspacePath: folder.uri.fsPath,
          executable: paths.executable, executableSource: 'global',
          ...(paths.dataPath === undefined ? {} : { dataPath: paths.dataPath }),
          workspaceTrusted: vscode.workspace.isTrusted,
          ...(vscode.env.remoteName === undefined ? {} : { remoteName: vscode.env.remoteName }),
        });
      });
    } finally { selecting = false; }
  });
  const refresh = vscode.commands.registerCommand('vcp.refreshConnection', () => report(async () => {
    const status = connection.state();
    if (status.editorTrusted !== vscode.workspace.isTrusted || (status.workspaceUri !== undefined && !vscode.workspace.workspaceFolders?.some(folder => folder.uri.toString() === status.workspaceUri))) {
      await connection.invalidate('Workspace or editor trust changed; reconnect explicitly.', vscode.workspace.isTrusted);
      return;
    }
    await connection.refresh();
  }));
  const disconnect = vscode.commands.registerCommand('vcp.disconnect', () => { intent++; return report(() => connection.disconnect()); });
  const folders = vscode.workspace.onDidChangeWorkspaceFolders(() => { intent++; void connection.invalidate('Workspace folders changed; reconnect explicitly.', vscode.workspace.isTrusted).catch(() => {}); });
  const trust = vscode.workspace.onDidGrantWorkspaceTrust(() => { intent++; void connection.invalidate('Editor trust changed; reconnect explicitly. Engine trust is unchanged.', vscode.workspace.isTrusted).catch(() => {}); });
  const configuration = vscode.workspace.onDidChangeConfiguration(event => {
    if (event.affectsConfiguration('vcp.engineExecutable') || event.affectsConfiguration('vcp.dataDirectory')) { intent++; void connection.invalidate('Connection settings changed; reconnect explicitly.', vscode.workspace.isTrusted).catch(() => {}); }
  });
  return [connect, refresh, disconnect, folders, trust, configuration, { dispose: () => { intent++; } }];
}
