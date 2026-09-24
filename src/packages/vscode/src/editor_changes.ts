// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { createHash, randomUUID } from 'node:crypto';
import type { ChangeView, ContextView, DocumentObservation, TaskView } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import { EngineConnection, type ConnectionClient } from './engine_connection.js';
import { EditorBuffers, type BufferBinding, type BufferCapture, type BufferEdit } from './editor_buffers.js';
import { EditorJournal } from './editor_journal.js';
import type { TaskPanelState } from './task_view_model.js';

export class EditorUserError extends Error {}

interface Draft { source: vscode.TextDocument; draft: vscode.TextDocument; expected: BufferCapture; task: string; client: ConnectionClient; used: boolean }
interface Prepared { view: ChangeView; drafts: Draft[]; edits: BufferEdit[][]; client: ConnectionClient; journal: EditorJournal }
interface Observed { source: vscode.TextDocument; task: string; client: ConnectionClient; id: string; closeCommand?: string }
const normalized = (text: string, eol: 'lf' | 'crlf') => text.replace(/\r\n|\r|\n/g, eol === 'crlf' ? '\r\n' : '\n');
function document(capture: BufferCapture, includeContent = true): DocumentObservation {
  return { ...capture.document, content: includeContent ? capture.document.content : null, selections: capture.document.selections.map(range => ({ start: { ...range.start }, end: { ...range.end } })) };
}

/** Native editable drafts and read-only previews; all authority stays in engine commands. */
export class EditorChanges implements vscode.TextDocumentContentProvider, vscode.Disposable {
  #buffers = new EditorBuffers();
  #drafts: Draft[] = [];
  #prepared: Prepared[] = [];
  #previews = new Map<string, string>();
  #previewChanges = new vscode.EventEmitter<vscode.Uri>();
  readonly onDidChange = this.#previewChanges.event;
  #disposed = false;
  #changed = new Set<vscode.TextDocument>();
  #refreshTimer: ReturnType<typeof setTimeout> | undefined;
  #refreshing = false;
  #refreshWork: Promise<void> | undefined;
  #working = false;
  #observed = new Map<string, Observed>();
  #epoch = 0;
  constructor(private readonly connection: EngineConnection, private readonly taskState: () => TaskPanelState | undefined,
    private readonly journal: () => EditorJournal) {}
  provideTextDocumentContent(uri: vscode.Uri): string { return this.#previews.get(uri.toString()) ?? 'Preview unavailable. Prepare a current observation.'; }
  #clearPreviews(): void { const keys = [...this.#previews.keys()]; this.#previews.clear(); for (const key of keys) this.#previewChanges.fire(vscode.Uri.parse(key)); }
  invalidate(document: vscode.TextDocument): void {
    this.#buffers.invalidate(document);
    if (this.#drafts.some(draft => draft.source === document) || [...this.#observed.values()].some(observed => observed.source === document)) { this.#clearPreviews(); this.#changed.add(document); this.#scheduleRefresh(); }
  }
  diagnosticsChanged(document: vscode.TextDocument): void {
    if (this.#drafts.some(draft => draft.source === document) || [...this.#observed.values()].some(observed => observed.source === document)) { this.#changed.add(document); this.#scheduleRefresh(); }
  }
  #require(epoch: number, client: ConnectionClient): void { if (this.#epoch !== epoch || !this.#binding(client)) throw new EditorUserError('Editor mapping changed; review current documents.'); }
  #remember(source: vscode.TextDocument, task: string, client: ConnectionClient, id: string, epoch: number): void {
    if (this.#epoch !== epoch || !this.#binding(client)) return;
    this.#observed.set(JSON.stringify([task, source.uri.toString()]), { source, task, client, id });
    if (source.isClosed) { this.#changed.add(source); this.#scheduleRefresh(); }
  }
  #rememberContext(sources: readonly vscode.TextDocument[], task: string, client: ConnectionClient, context: ContextView, epoch: number): void {
    for (const source of sources) {
      const observation = context.observations.find(observed => observed.document.uri === source.uri.toString());
      if (observation) this.#remember(source, task, client, observation.id, epoch);
    }
  }
  #scheduleRefresh(): void {
    if (this.#disposed || this.#working || this.#refreshing || this.#refreshTimer) return;
    this.#refreshTimer = setTimeout(() => { this.#refreshTimer = undefined; this.#refreshWork = this.#refreshObservations().finally(() => { this.#refreshWork = undefined; }); }, 200);
  }
  async suspendObservations(): Promise<void> { this.#working = true; clearTimeout(this.#refreshTimer); this.#refreshTimer = undefined; await this.#refreshWork; }
  resumeObservations(): void { this.#working = false; if (this.#changed.size) this.#scheduleRefresh(); }
  async refreshObservations(): Promise<void> {
    this.#client();
    for (const observed of this.#observed.values()) this.#changed.add(observed.source);
    for (const draft of this.#drafts) this.#changed.add(draft.source);
    await this.#refreshObservations();
  }
  async #refreshObservations(): Promise<void> {
    this.#refreshing = true;
    const epoch = this.#epoch;
    const documents = [...this.#changed]; this.#changed.clear();
    try {
      for (const source of documents) {
        const tracked = [...this.#observed.values()].filter(item => item.source === source);
        if (source.isClosed) {
          for (const observed of tracked) await this.#retire(observed);
          continue;
        }
        const draft = this.#drafts.find(item => item.source === source) ?? tracked[0];
        if (!draft || !this.#binding(draft.client)) continue;
        let id: string | undefined; let journal: EditorJournal | undefined;
        try {
          const task = await this.#task(draft.client, draft.task);
          journal = this.journal(); id = await journal.begin(draft.client.scope, draft.task, 'context');
          this.#require(epoch, draft.client);
          const binding = this.#binding(draft.client)!;
          const observed = this.#buffers.capture({ document: source, selections: [] }, binding, false);
          const reply = await draft.client.call('editor/context', { scope: draft.client.scope, task: draft.task, mutation: { command_id: id, expected_revision: task.revision, steering_revision: task.steering_revision }, documents: [document(observed, false)] });
          this.#rememberContext([source], draft.task, draft.client, reply.value, epoch);
          await journal.settle(id, 'accepted');
        } catch { if (id && journal) await journal.settle(id, 'unknown').catch(() => {}); }
      }
    } finally { this.#refreshing = false; if (this.#changed.size) this.#scheduleRefresh(); }
  }
  async #retire(observed: Observed): Promise<void> {
    const epoch = this.#epoch;
    if (!observed.source.isClosed || !this.#binding(observed.client)) return;
    const journal = this.journal();
    const remove = () => { const key = JSON.stringify([observed.task, observed.source.uri.toString()]); if (this.#observed.get(key) === observed) this.#observed.delete(key); };
    if (observed.closeCommand) {
      try {
        const receipt = (await observed.client.call('command/read', { scope: observed.client.scope, command_id: observed.closeCommand })).value;
        if (receipt.outcome === 'accepted' && receipt.command_id === observed.closeCommand && receipt.task === observed.task && receipt.scope.workspace === observed.client.scope.workspace && receipt.scope.session === observed.client.scope.session) {
          await journal.settle(observed.closeCommand, 'accepted'); remove();
        }
      } catch { /* Unknown close outcome gets only an original-ID read, never replay. */ }
      return;
    }
    for (let attempt = 0; attempt < 3; attempt++) {
      let id: string | undefined;
      try {
        const task = await this.#task(observed.client, observed.task);
        id = await journal.begin(observed.client.scope, observed.task, 'context'); observed.closeCommand = id;
        this.#require(epoch, observed.client);
        if (!observed.source.isClosed) throw new EditorUserError('Close attestation changed.');
        await observed.client.call('editor/context', { scope: observed.client.scope, task: observed.task, mutation: { command_id: id, expected_revision: task.revision, steering_revision: task.steering_revision }, documents: [], closed: [observed.id] });
        await journal.settle(id, 'accepted'); remove(); return;
      } catch (error) {
        const known = error as { code?: string; classification?: { applicationCode?: string; retry?: string } };
        const stale = known?.code === 'rpc' && known.classification?.applicationCode === 'VERSION_CONFLICT' && known.classification.retry !== 'reconcile_original';
        if (id) await journal.settle(id, stale ? 'resolved' : 'unknown').catch(() => {});
        if (!stale || this.#epoch !== epoch || !this.#binding(observed.client)) return;
        delete observed.closeCommand;
      }
    }
  }
  invalidateMapping(): void { this.#epoch++; clearTimeout(this.#refreshTimer); this.#refreshTimer = undefined; this.#changed.clear(); this.#observed.clear(); this.#buffers.dispose(); this.#buffers = new EditorBuffers(); this.#drafts = []; this.#prepared = []; this.#clearPreviews(); }
  #binding(client?: ConnectionClient): BufferBinding | undefined {
    const state = this.connection.state();
    if (this.#disposed || state.phase !== 'connected' || state.role !== 'controller' || !vscode.workspace.isTrusted || state.engineTrust !== 'trusted'
      || !state.host || !state.rootId || !state.workspaceRoot || state.bindingRevision === undefined
      || (client && this.connection.currentClient() !== client)) return undefined;
    return { host: state.host.id, rootId: state.rootId, rootPath: state.workspaceRoot, bindingRevision: state.bindingRevision, generation: state.generation, trusted: true };
  }
  #client(): ConnectionClient {
    const client = this.connection.currentClient();
    if (!client || !this.#binding(client) || !client.initialized.capabilities.includes('editor/prepared-edits/1')) throw new EditorUserError('An authorized execution-backed editor connection is required.');
    return client;
  }
  async #task(client: ConnectionClient, task: string): Promise<TaskView> {
    if (!this.#binding(client)) throw new EditorUserError('Editor authority changed.');
    const value = (await client.call('task/read', { scope: client.scope, task })).value;
    if (!this.#binding(client) || value.task !== task) throw new EditorUserError('Task changed.');
    return value;
  }
  #matches(editor: vscode.TextEditor, draft: Draft): BufferCapture {
    const binding = this.#binding(draft.client); if (!binding) throw new EditorUserError('Editor authority changed.');
    const capture = this.#buffers.capture(editor, binding);
    if (capture.binding.generation !== draft.expected.binding.generation || capture.binding.rootId !== draft.expected.binding.rootId
      || capture.binding.bindingRevision !== draft.expected.binding.bindingRevision || capture.epoch !== draft.expected.epoch
      || capture.document.open_id !== draft.expected.document.open_id || capture.document.version !== draft.expected.document.version
      || capture.document.content_sha256 !== draft.expected.document.content_sha256 || capture.document.dirty !== draft.expected.document.dirty) throw new EditorUserError('The source changed; create a fresh draft from current text.');
    return capture;
  }
  async createDraft(): Promise<void> {
    const epoch = this.#epoch;
    const client = this.#client(); const editor = vscode.window.activeTextEditor;
    const task = this.taskState()?.detail?.task;
    if (!editor || !task || task.scope.workspace !== client.scope.workspace || task.scope.session !== client.scope.session) throw new EditorUserError('Select a canonical task and a local source document.');
    this.#drafts = this.#drafts.filter(draft => draft.used || !draft.draft.isClosed);
    if (this.#drafts.length >= 16) throw new EditorUserError('Close or resolve existing edit drafts first.');
    const expected = this.#buffers.capture(editor, this.#binding(client)!);
    const draft = await vscode.workspace.openTextDocument({ content: expected.document.content!, language: editor.document.languageId });
    this.#require(epoch, client);
    this.#drafts.push({ source: editor.document, draft, expected, task: task.task, client, used: false });
    await vscode.window.showTextDocument(draft, { preview: false });
    void vscode.window.showInformationMessage('Edit this draft, then run VCP: Review Editor Drafts. The source file is unchanged.');
  }
  async review(): Promise<void> {
    const epoch = this.#epoch;
    const client = this.#client();
    const available = this.#drafts.filter(draft => !draft.used && !draft.draft.isClosed && draft.client === client);
    const selected = await vscode.window.showQuickPick(available.map(draft => ({ label: draft.expected.document.relative_path, description: draft.task, draft })),
      { title: 'Review editor drafts', canPickMany: true, ignoreFocusOut: true, placeHolder: 'Choose up to 16 files for one task. Changes remain unsaved buffers.' });
    if (!selected?.length) return;
    this.#require(epoch, client);
    const drafts = selected.map(item => item.draft); const taskId = drafts[0]!.task;
    if (drafts.some(draft => draft.task !== taskId) || new Set(drafts.map(draft => draft.expected.document.uri)).size !== drafts.length) throw new EditorUserError('Choose distinct files for one task.');
    const captures: BufferCapture[] = []; const edits: BufferEdit[][] = [];
    for (const draft of drafts) {
      const editor = await vscode.window.showTextDocument(draft.source, { preserveFocus: true, preview: false });
      this.#require(epoch, client);
      captures.push(this.#matches(editor, draft));
      const text = normalized(draft.draft.getText(), draft.expected.document.eol);
      if (Buffer.byteLength(text, 'utf8') > 65536 || text === draft.expected.document.content) throw new EditorUserError('Each draft needs a bounded content change.');
      const lines = draft.expected.document.content!.split(/\r\n|\r|\n/);
      edits.push([{ range: { start: { line: 0, character: 0 }, end: { line: lines.length - 1, character: lines.at(-1)!.length } }, text }]);
    }
    if (captures.reduce((sum, capture, index) => sum + Buffer.byteLength(capture.document.content!, 'utf8') + Buffer.byteLength(edits[index]![0]!.text, 'utf8'), 0) > 65536) throw new EditorUserError('Combined draft content exceeds the 64 KiB review bound.');
    const journal = this.journal(); const task = await this.#task(client, taskId);
    const observationId = await journal.begin(client.scope, taskId, 'context');
    let context;
    try {
      this.#require(epoch, client);
      context = (await client.call('editor/context', { scope: client.scope, task: taskId, mutation: { command_id: observationId, expected_revision: task.revision, steering_revision: task.steering_revision }, documents: captures.map(capture => document(capture)) })).value;
      this.#rememberContext(drafts.map(draft => draft.source), taskId, client, context, epoch);
      await journal.settle(observationId, 'accepted');
    } catch { await journal.settle(observationId, 'unknown'); throw new EditorUserError('Observation requires reconciliation.'); }
    this.#require(epoch, client);
    const current = await this.#task(client, taskId);
    const prepareId = await journal.begin(client.scope, taskId, 'prepare');
    for (const draft of drafts) draft.used = true;
    let view: ChangeView;
    try {
      this.#require(epoch, client);
      if (context.observations.length !== drafts.length) throw new EditorUserError('Observations changed.');
      const files = captures.map((capture, index) => {
        const observation = context.observations.find(item => item.document.open_id === capture.document.open_id && item.document.uri === capture.document.uri
          && item.document.content_sha256 === capture.document.content_sha256 && item.document.version === capture.document.version && item.root === capture.binding.rootId);
        if (!observation) throw new EditorUserError('Observation identity mismatch.');
        return { observation: observation.id, edits: edits[index]!.map(edit => ({ range: { start: { ...edit.range.start }, end: { ...edit.range.end } }, text: edit.text })) };
      });
      view = (await client.call('editor/prepare', { scope: client.scope, task: taskId, mutation: { command_id: prepareId, expected_revision: current.revision, steering_revision: current.steering_revision }, generation: context.generation, files })).value;
      if (view.change !== prepareId || view.files.length !== drafts.length) throw new EditorUserError('Prepared identity mismatch.');
      await journal.settle(prepareId, 'accepted', view.change);
    } catch { await journal.settle(prepareId, 'unknown'); throw new EditorUserError('Preparation requires reconciliation; do not recreate it.'); }
    this.#require(epoch, client);
    this.#prepared.push({ view, drafts, edits, client, journal });
    this.#clearPreviews();
    for (let index = 0; index < drafts.length; index++) {
      this.#require(epoch, client);
      const left = vscode.Uri.parse(`vcp-editor-preview:/${randomUUID()}/before`), right = vscode.Uri.parse(`vcp-editor-preview:/${randomUUID()}/after`);
      this.#previews.set(left.toString(), drafts[index]!.expected.document.content!); this.#previews.set(right.toString(), edits[index]![0]!.text);
      await vscode.commands.executeCommand('vscode.diff', left, right, `Review ${drafts[index]!.expected.document.relative_path}`, { preview: false });
    }
    this.#require(epoch, client);
    void vscode.window.showInformationMessage('Prepared for review. Resolve any policy question in Tasks, resume explicitly if required, then run VCP: Apply Reviewed Editor Changes.');
  }
  async apply(): Promise<void> {
    const epoch = this.#epoch;
    const client = this.#client();
    const selected = await vscode.window.showQuickPick(this.#prepared.filter(prepared => prepared.client === client).map(prepared => ({ label: prepared.view.change, description: `${prepared.view.files.length} files`, prepared })),
      { title: 'Apply reviewed editor changes', placeHolder: 'Each file is reauthorized separately. Partial application is possible.', ignoreFocusOut: true });
    if (!selected) return;
    this.#require(epoch, client);
    const prepared = selected.prepared;
    const approval = await vscode.window.showWarningMessage(`Apply ${prepared.drafts.length} reviewed file changes to editor buffers? Each file may succeed or conflict independently.`, { modal: true }, 'Apply');
    if (approval !== 'Apply') return;
    this.#require(epoch, client);
    let applied = 0;
    for (let index = 0; index < prepared.drafts.length; index++) {
      const draft = prepared.drafts[index]!;
      prepared.view = (await client.call('editor/changeRead', { scope: client.scope, task: prepared.view.task, change: prepared.view.change })).value;
      this.#require(epoch, client);
      const file = prepared.view.files[index];
      if (!file || file.file !== index) throw new EditorUserError('Change identity changed.');
      if (file.state === 'applied' || file.state === 'rejected') { if (file.state === 'applied') applied++; continue; }
      if (file.state !== 'prepared') throw new EditorUserError('Dispatched or unknown changes can only be reconciled; never replayed.');
      const editor = await vscode.window.showTextDocument(draft.source, { preserveFocus: true, preview: false });
      this.#require(epoch, client);
      this.#matches(editor, draft);
      if (file.open_id !== draft.expected.document.open_id || file.content_sha256 !== draft.expected.document.content_sha256 || file.version !== draft.expected.document.version
        || file.uri !== draft.expected.document.uri || file.after_sha256 !== createHash('sha256').update(prepared.edits[index]![0]!.text, 'utf8').digest('hex')
        || prepared.view.root !== draft.expected.binding.rootId || prepared.view.binding_revision !== draft.expected.binding.bindingRevision) throw new EditorUserError('Prepared source identity changed.');
      const task = await this.#task(client, draft.task);
      if (task.state !== 'running') throw new EditorUserError('Resolve pending policy questions and resume explicitly before applying.');
      const dispatchId = await prepared.journal.begin(client.scope, draft.task, 'dispatch', { change: prepared.view.change, file: index });
      let dispatched;
      try {
        this.#require(epoch, client);
        this.#matches(editor, draft);
        dispatched = (await client.call('editor/dispatch', { scope: client.scope, task: draft.task, change: prepared.view.change, generation: prepared.view.generation, file: index,
          mutation: { command_id: dispatchId, expected_revision: prepared.view.revision, steering_revision: task.steering_revision } })).value;
        await prepared.journal.settle(dispatchId, 'accepted');
      } catch { await prepared.journal.settle(dispatchId, 'unknown'); throw new EditorUserError('Dispatch requires reconciliation.'); }
      this.#require(epoch, client);
      if (!dispatched.apply || dispatched.change.change !== prepared.view.change || dispatched.file !== index) throw new EditorUserError('Historical dispatch cannot authorize a new edit.');
      prepared.view = dispatched.change;
      const result = await this.#buffers.apply(editor, draft.expected, prepared.edits[index]!, () => this.#epoch === epoch ? this.#binding(client) : undefined);
      // If current scope vanished, retain durable intent and report unknown later.
      if (!result.after || !this.#binding(client)) throw new EditorUserError('Current receipt unavailable; reconcile the original change.');
      const afterTask = await this.#task(client, draft.task);
      const resultId = await prepared.journal.begin(client.scope, draft.task, 'result', { change: prepared.view.change, file: index });
      try {
        this.#require(epoch, client);
        const binding = this.#binding(client); if (!binding) throw new EditorUserError('Receipt authority changed.');
        const observed = this.#buffers.capture(editor, binding);
        const outcome = observed.document.open_id === result.after.document.open_id && observed.epoch === result.after.epoch
          && observed.document.version === result.after.document.version && observed.document.content_sha256 === result.after.document.content_sha256
          && observed.document.dirty === result.after.document.dirty ? result.outcome : 'unknown';
        prepared.view = (await client.call('editor/changeResult', { scope: client.scope, task: draft.task, change: prepared.view.change, generation: prepared.view.generation,
          file: index, execution: dispatched.execution, outcome, document: document(observed),
          mutation: { command_id: resultId, expected_revision: prepared.view.revision, steering_revision: afterTask.steering_revision } })).value;
        const observedId = prepared.view.files[index]?.observed_observation;
        if (observedId) this.#remember(draft.source, draft.task, client, observedId, epoch);
        await prepared.journal.settle(resultId, 'accepted');
      } catch { await prepared.journal.settle(resultId, 'unknown'); throw new EditorUserError('Receipt outcome requires reconciliation; do not apply again.'); }
      this.#require(epoch, client);
      if (prepared.view.files[index]?.state !== 'applied') break;
    }
    if (prepared.view.files.every(file => file.state === 'applied' || file.state === 'rejected')) {
      await prepared.journal.resolveChange(client.scope, prepared.view.change);
      this.#prepared = this.#prepared.filter(item => item !== prepared);
      this.#drafts = this.#drafts.filter(draft => !prepared.drafts.includes(draft));
    }
    this.#clearPreviews();
    this.#require(epoch, client);
    applied = prepared.view.files.filter(file => file.state === 'applied').length;
    void vscode.window.showInformationMessage(`Observed ${applied} applied buffer changes. Inspect remaining file receipts before proceeding. Saving is a separate editor action.`);
  }
  dispose(): void { this.#disposed = true; this.invalidateMapping(); this.#buffers.dispose(); this.#previewChanges.dispose(); }
}
