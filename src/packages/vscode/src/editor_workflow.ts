// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { randomUUID } from 'node:crypto';
import { EngineConnection, type ConnectionStatus } from './engine_connection.js';
import { selectionForFolder } from './commands.js';
import { EditorJournal } from './editor_journal.js';
import { EditorChanges, EditorUserError } from './editor_changes.js';
import type { TaskPanelState } from './task_view_model.js';

export function dollarsToMicros(value: string): string | undefined {
  if (!/^(0|[1-9][0-9]{0,12})(\.[0-9]{1,6})?$/.test(value)) return undefined;
  const [whole, fraction = ''] = value.split('.');
  const micros = BigInt(whole!) * 1_000_000n + BigInt(fraction.padEnd(6, '0'));
  return micros > 0n && micros <= 18446744073709551615n ? micros.toString() : undefined;
}

/** Explicit native editor commands. Drafts and credentials stay in this activation. */
export class EditorWorkflow implements vscode.Disposable {
  #journals = new Map<string, EditorJournal>();
  #busy = false;
  #disposed = false;
  #changes: EditorChanges;
  #connectionKey: string;
  constructor(private readonly connection: EngineConnection, private readonly context: vscode.ExtensionContext,
    private readonly taskState: () => TaskPanelState | undefined) {
    this.#changes = new EditorChanges(connection, taskState, () => this.#journal());
    this.#connectionKey = this.#key(connection.state());
  }
  #key(state: ConnectionStatus): string { return JSON.stringify([state.phase, state.generation, state.role, state.rootId, state.bindingRevision, state.engineTrust, state.editorTrusted]); }
  connectionChanged(state: ConnectionStatus): void { const key = this.#key(state); if (key !== this.#connectionKey) { this.#connectionKey = key; this.#changes.invalidateMapping(); } }
  registrations(): vscode.Disposable[] {
    return [vscode.commands.registerCommand('vcp.startEditorTask', () => this.start()),
      vscode.commands.registerCommand('vcp.createEditDraft', () => this.#run(() => this.#changes.createDraft())),
      vscode.commands.registerCommand('vcp.reviewEditorDrafts', () => this.#run(() => this.#changes.review())),
      vscode.commands.registerCommand('vcp.applyEditorChanges', () => this.#run(() => this.#changes.apply())),
      vscode.commands.registerCommand('vcp.inspectEditorChanges', () => this.inspect()),
      vscode.commands.registerCommand('vcp.refreshEditorObservations', () => this.#run(() => this.#changes.refreshObservations())),
      vscode.workspace.registerTextDocumentContentProvider('vcp-editor-preview', this.#changes),
      vscode.workspace.onDidChangeTextDocument(event => this.#changes.invalidate(event.document)),
      vscode.workspace.onDidSaveTextDocument(document => this.#changes.invalidate(document)),
      vscode.workspace.onDidCloseTextDocument(document => this.#changes.invalidate(document)),
      vscode.languages.onDidChangeDiagnostics(event => { for (const document of vscode.workspace.textDocuments) if (event.uris.some(uri => uri.toString() === document.uri.toString())) this.#changes.diagnosticsChanged(document); }),
      vscode.workspace.onDidRenameFiles(() => this.#changes.invalidateMapping()),
      vscode.workspace.onDidDeleteFiles(() => this.#changes.invalidateMapping()),
      vscode.workspace.onDidChangeWorkspaceFolders(() => this.#changes.invalidateMapping())];
  }
  #journal(): EditorJournal {
    const profile = this.connection.profileKey();
    if (!profile) throw new EditorUserError('No editor connection');
    let journal = this.#journals.get(profile);
    if (!journal) {
      const key = `vcp.editorCommands.v1.${profile}`;
      journal = new EditorJournal(async records => { await this.context.workspaceState.update(key, records); }, this.context.workspaceState.get<unknown>(key));
      this.#journals.set(profile, journal);
    }
    return journal;
  }
  async #run(action: () => Promise<void>): Promise<void> {
    if (this.#busy || this.#disposed) return;
    this.#busy = true;
    try { await this.#changes.suspendObservations(); if (!this.#disposed) await action(); }
    catch (error) { void vscode.window.showErrorMessage(error instanceof EditorUserError ? error.message : 'Editor operation could not be completed. Inspect editor command outcomes before retrying; no edit is replayed automatically.'); }
    finally { this.#busy = false; this.#changes.resumeObservations(); }
  }
  start(): Promise<void> { return this.#run(async () => {
    if (!vscode.workspace.isTrusted) throw new EditorUserError('Editor trust required');
    const generation = this.connection.state().generation;
    const folders = vscode.workspace.workspaceFolders ?? [];
    const choice = await vscode.window.showQuickPick(folders.map(folder => ({ label: folder.name, description: folder.uri.toString(), folder })),
      { title: 'Start an execution-backed VCP task', placeHolder: 'Choose an initialized trusted workspace by its full URI', ignoreFocusOut: true });
    if (!choice) return;
    const profile = await vscode.window.showOpenDialog({ title: 'Choose the existing trusted VCP execution profile', canSelectMany: false, canSelectFiles: true, canSelectFolders: false });
    if (!profile?.[0] || profile[0].scheme !== 'file') return;
    const objective = await vscode.window.showInputBox({ title: 'Task objective', prompt: 'The initial provider prompt uses disk context. Later editor observations do not change that prompt.', ignoreFocusOut: true,
      validateInput: text => !text.trim() || Buffer.byteLength(text, 'utf8') > 65536 ? 'Enter a nonempty objective up to 64 KiB.' : undefined });
    if (objective === undefined) return;
    const cap = await vscode.window.showInputBox({ title: 'Configured task cost cap (USD)', prompt: 'Enter the cap already configured for this workspace; the engine verifies an exact match.', ignoreFocusOut: true, validateInput: text => dollarsToMicros(text) ? undefined : 'Enter a positive USD amount, with at most six decimal places.' });
    if (cap === undefined) return;
    const requests = await vscode.window.showInputBox({ title: 'Configured maximum requests', prompt: 'Use the limit from the selected execution profile.', ignoreFocusOut: true,
      validateInput: text => /^[1-9][0-9]{0,3}$/.test(text) && Number(text) <= 1024 ? undefined : 'Enter an integer from 1 through 1024.' });
    if (requests === undefined) return;
    const deadline = await vscode.window.showInputBox({ title: 'Configured task deadline (seconds)', prompt: 'Use the deadline from the selected execution profile.', ignoreFocusOut: true,
      validateInput: text => /^[1-9][0-9]{0,4}$/.test(text) && Number(text) <= 86400 ? undefined : 'Enter an integer from 1 through 86400.' });
    if (deadline === undefined) return;
    const credential = await vscode.window.showInputBox({ title: 'Provider credential', prompt: 'Used only by this explicitly launched engine; never stored in workspace state.', password: true, ignoreFocusOut: true });
    if (credential === undefined) return;
    if (this.#disposed || !vscode.workspace.isTrusted || this.connection.state().generation !== generation
      || !vscode.workspace.workspaceFolders?.some(folder => folder.uri.toString() === choice.folder.uri.toString())) return;
    const task = randomUUID(); const turn = randomUUID();
    const state = await this.connection.connectExecution(selectionForFolder(choice.folder), { profile: profile[0].fsPath, providerCredential: credential }, task);
    const client = this.connection.currentClient();
    if (!client || state.role !== 'controller' || state.engineTrust !== 'trusted' || !client.initialized.methods.includes('turn/start')) throw new EditorUserError('Execution host unavailable');
    const acceptedKey = this.#connectionKey;
    const journal = this.#journal(); const id = await journal.begin(client.scope, task, 'start');
    try {
      if (this.#disposed || !vscode.workspace.isTrusted || this.connection.currentClient() !== client || this.connection.state().generation !== state.generation) throw new EditorUserError('Connection changed');
      const receipt = await client.call('turn/start', { scope: client.scope, mutation: { command_id: id, expected_revision: '0', steering_revision: '0' }, task, turn,
        objective, constraints: [], acceptance: [], budget: { currency: 'USD', cap_micros: dollarsToMicros(cap)!, max_requests: Number(requests), deadline_seconds: Number(deadline) } });
      if (receipt.value.command_id !== id || receipt.value.task !== task || receipt.value.outcome !== 'accepted'
        || receipt.value.scope.workspace !== client.scope.workspace || receipt.value.scope.session !== client.scope.session) throw new EditorUserError('Acceptance identity changed');
      await journal.settle(id, 'accepted');
      if (this.#disposed || this.connection.currentClient() !== client || this.#connectionKey !== acceptedKey) return;
      void vscode.window.showInformationMessage('Task accepted. Inspect Tasks for its current state; editor changes require separate preparation and review.');
    } catch { await journal.settle(id, 'unknown'); throw new EditorUserError('Task outcome requires reconciliation'); }
  }); }
  inspect(): Promise<void> { return this.#run(async () => {
    const client = this.connection.currentClient(); if (!client) throw new EditorUserError('Connect to inspect outcomes');
    const key = this.#connectionKey;
    const current = () => !this.#disposed && this.connection.currentClient() === client && this.#connectionKey === key;
    const journal = this.#journal(); await journal.reconcile(client.scope, client.call.bind(client));
    if (!current()) return;
    const records = journal.records().filter(record => record.scope.workspace === client.scope.workspace && record.scope.session === client.scope.session);
    const selected = await vscode.window.showQuickPick(records.map(record => ({ label: `${record.kind}: ${record.phase}`, description: record.id, detail: `Task ${record.task}${record.change ? `; change ${record.change}` : ''}`, record })),
      { title: 'Editor command outcomes', placeHolder: 'Acceptance is not proof of applied edits or task completion.' });
    if (selected?.record.change && current()) {
      const view = (await client.call('editor/changeRead', { scope: client.scope, task: selected.record.task, change: selected.record.change })).value;
      if (!current()) return;
      if (view.files.every(file => file.state === 'applied' || file.state === 'rejected')) await journal.resolveChange(client.scope, view.change);
      if (!current()) return;
      await vscode.window.showQuickPick(view.files.map(file => ({ label: `${file.relative_path}: ${file.state}`, description: file.execution ?? 'No dispatch', detail: `Observed version ${file.observed_version ?? 'unavailable'}; ${file.dirty ? 'unsaved buffer' : 'inspect disk state separately'}` })),
        { title: 'Canonical per-file edit outcomes', placeHolder: view.buffers_unverified ? 'Disk-only checks do not verify these editor buffers. Unknown outcomes are never replayed.' : 'Receipts describe observed changes, not task completion.' });
    }
  }); }
  dispose(): void { this.#disposed = true; this.#changes.dispose(); }
}
