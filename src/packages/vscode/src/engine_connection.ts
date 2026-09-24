// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Client, Counter, InitializeParams, LaunchOptions, AttachOptions, LocalAttachment, ReconnectObserverOptions, RebindOptions, RebindResult, Scope, Trust, WorkspaceView } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import { connectionRestriction, type ConnectionSelection } from './trust.js';
import { canonicalWorkspaceRoot, WorkspaceMap } from './workspace_map.js';
import { profile, type Recovery } from './recovery.js';

export type Selection = ConnectionSelection;
export type ConnectionClient = Pick<Client, 'scope' | 'initialized' | 'call' | 'dispose'> & Partial<Pick<Client, 'observerReconnectReference' | 'attachment'>>;
export interface ConnectionStatus {
  readonly phase: 'disconnected' | 'connecting' | 'connected' | 'unavailable';
  readonly generation: number;
  readonly message: string;
  readonly editorTrusted: boolean;
  readonly role?: 'observer' | 'controller';
  readonly workspaceRevision?: Counter;
  readonly workspaceUri?: string;
  readonly workspaceRoot?: string;
  readonly rootId?: string;
  readonly bindingRevision?: Counter;
  readonly engineBuild?: string;
  readonly engineExecutable?: string;
  readonly protocolVersion?: string;
  readonly host?: Readonly<{ id: string; platform: string }>;
  readonly engineTrust?: Trust;
  readonly scope?: Readonly<Scope>;
  readonly pendingInputs?: number;
  readonly taskCount?: number;
  readonly watermark?: Counter;
  readonly limitations: readonly string[];
}
export interface ConnectionDependencies {
  launch(options: LaunchOptions): Promise<ConnectionClient>;
  reconnect?(options: ReconnectObserverOptions): Promise<ConnectionClient>;
  attach?(options: AttachOptions): Promise<ConnectionClient>;
  rebind?(options: RebindOptions): Promise<RebindResult>;
  saveRecovery?(value: Recovery | undefined): Promise<void>;
  publish?(status: ConnectionStatus): void;
  canonicalize?(path: string): Promise<string>;
  platform?: string;
  now?(): number;
}
const LIMITATIONS = Object.freeze([
  'Task views require a matching engine; task control requires explicit controller ownership.',
  'Connect to an existing initialized workspace.',
  'Moved roots require explicit reconciliation after the active owner closes.',
  'Reload restores observation only; control and task resume remain explicit.',
]);
const INITIALIZE: InitializeParams = {
  protocol_version: '1.0', client: { name: 'vcp-vscode', version: '0.1.0' },
  capabilities: ['approval/source-revisions/1', 'controller/read', 'controller/acquire', 'workspace/setTrust', 'task/presentation', 'task/read', 'usage/read', 'events/next', 'command/read', 'approval/respond', 'turn/start', 'turn/steer', 'turn/pause', 'task/cancel', 'session/resume', 'artifact/read', 'editor/prepared-edits/1', 'editor/context', 'editor/prepare', 'editor/changeRead', 'editor/dispatch', 'editor/changeResult'],
  required_capabilities: ['jsonrpc/2.0', 'workspace/open', 'workspace/binding/1', 'session/snapshot', 'events/unsubscribe'],
};
function failureMessage(error: unknown): string {
  const code = typeof error === 'object' && error !== null && 'code' in error ? error.code : undefined;
  if (code === 'UNSUPPORTED_VERSION') return 'The selected engine protocol version is incompatible.';
  if (code === 'CAPABILITY_UNAVAILABLE') return 'The selected engine does not expose the required observer capabilities.';
  if (code === 'TIMEOUT') return 'The engine did not respond before the connection deadline.';
  return 'Connection unavailable. Check the trusted executable, existing workspace initialization and data directory; reconnect explicitly.';
}
export class EngineConnection {
  #deps: ConnectionDependencies;
  #map = new WorkspaceMap();
  #client: ConnectionClient | undefined;
  #selection: Selection | undefined;
  #disposed = false;
  #role: 'observer' | 'controller' = 'observer';
  #mutation: Promise<ConnectionStatus> | undefined;
  #mutationTrusted: boolean | undefined;
  #persistence: Promise<void> = Promise.resolve();
  #controller: { folderUri: string; profile: string; attachment: LocalAttachment } | undefined;
  #refresh: Promise<ConnectionStatus> | undefined;
  #starts = new Set<Promise<ConnectionStatus>>();
  #disposal: Promise<void> | undefined;
  #status: ConnectionStatus = Object.freeze({ phase: 'disconnected', generation: 0, message: 'Select an initialized workspace and connect explicitly.', editorTrusted: false, limitations: LIMITATIONS });
  constructor(dependencies: ConnectionDependencies) { this.#deps = dependencies; }
  state(): ConnectionStatus { return this.#status; }
  currentClient(): ConnectionClient | undefined { return this.#status.phase === 'connected' ? this.#client : undefined; }
  profileKey(): string | undefined { return this.#selection ? profile(this.#selection) : undefined; }
  #publish(status: ConnectionStatus): ConnectionStatus {
    this.#status = Object.freeze(status);
    this.#deps.publish?.(this.#status);
    return this.#status;
  }
  #current(generation: number): boolean { return !this.#disposed && generation === this.#map.generation; }
  #remember(value: Recovery | undefined): Promise<void> {
    const write = this.#persistence.catch(() => {}).then(() => this.#deps.saveRecovery?.(value));
    this.#persistence = write;
    return write;
  }
  restore(selection: Selection, saved: Recovery): Promise<ConnectionStatus> {
    if (saved.folderUri !== selection.workspaceUri || saved.profile !== profile(selection)) return Promise.resolve(this.#status);
    return this.connect(selection, 'observer', saved);
  }
  connect(selection: Selection, role: 'observer' | 'controller' = 'observer', saved?: Recovery): Promise<ConnectionStatus> {
    if (!saved && role === 'observer' && this.#selection?.workspaceUri === selection.workspaceUri && profile(this.#selection) === profile(selection)) {
      const reference = this.#client?.observerReconnectReference?.();
      if (reference) saved = { folderUri: selection.workspaceUri, profile: profile(selection), reference };
    }
    const pending = this.#connect(selection, [...this.#starts], role, saved);
    this.#starts.add(pending);
    void pending.finally(() => this.#starts.delete(pending)).catch(() => {});
    return pending;
  }
  connectExecution(selection: Selection, execution: NonNullable<LaunchOptions['execution']>, rootTask: string): Promise<ConnectionStatus> {
    if (!selection.workspaceTrusted) return Promise.reject(new Error('trusted editor required'));
    const pending = this.#connect(selection, [...this.#starts], 'controller', undefined, { execution, rootTask });
    this.#starts.add(pending);
    void pending.finally(() => this.#starts.delete(pending)).catch(() => {});
    return pending;
  }
  async #connect(selection: Selection, previousStarts: Promise<ConnectionStatus>[], role: 'observer' | 'controller', saved?: Recovery, execution?: Pick<LaunchOptions, 'execution' | 'rootTask'>): Promise<ConnectionStatus> {
    if (this.#disposed) return this.#status;
    const selected = { ...selection };
    const generation = this.#map.invalidate();
    this.#role = role;
    const previous = this.#client;
    this.#client = undefined; this.#selection = undefined; this.#refresh = undefined;
    const restriction = connectionRestriction(selected, this.#deps.platform ?? process.platform);
    this.#publish({ phase: restriction ? 'unavailable' : 'connecting', generation, message: restriction ?? `Connecting to the selected workspace as ${role}.`, editorTrusted: selected.workspaceTrusted, workspaceUri: selected.workspaceUri, engineExecutable: selected.executable, limitations: LIMITATIONS });
    if (previous) await previous.dispose().catch(() => {});
    await Promise.allSettled(previousStarts);
    if (!this.#current(generation) || restriction) return this.#status;
    let client: ConnectionClient | undefined;
    try {
      if (!saved) await this.#remember(undefined);
      const root = await (this.#deps.canonicalize ?? canonicalWorkspaceRoot)(selected.workspacePath);
      if (!this.#current(generation)) return this.#status;
      if (saved) {
        if (!this.#deps.reconnect) throw new Error('observer reconnect unavailable');
        client = await this.#deps.reconnect({ executable: selected.executable, reference: saved.reference, initialize: structuredClone(INITIALIZE) });
      } else if (!execution && role === 'controller' && this.#controller?.folderUri === selected.workspaceUri && this.#controller.profile === profile(selected) && this.#deps.attach) {
        client = await this.#deps.attach({ executable: selected.executable, attachment: this.#controller.attachment, initialize: structuredClone(INITIALIZE) });
      } else {
        client = await this.#deps.launch({ executable: selected.executable, workspace: root, ...(selected.dataPath === undefined ? {} : { data: selected.dataPath }), role, transport: 'windows_pipe', initialize: structuredClone(INITIALIZE), ...execution });
      }
      if (!this.#current(generation)) { await client.dispose(); return this.#status; }
      this.#client = client; this.#selection = selected;
      if (role === 'controller') {
        const ownership = await client.call('controller/read', { scope: { ...client.scope } });
        await client.call('controller/acquire', { scope: { ...client.scope }, command_id: randomUUID(), expected_revision: ownership.value.revision ?? null });
        if (!this.#current(generation)) { await client.dispose(); return this.#status; }
        const attachment = client.attachment?.();
        if (attachment) this.#controller = { folderUri: selected.workspaceUri, profile: profile(selected), attachment };
      }
      const reading = this.#read(client, this.#selection, generation);
      this.#refresh = reading;
      try { return await reading; }
      finally { if (this.#refresh === reading) this.#refresh = undefined; }
    } catch (error) {
      if (client) await client.dispose().catch(() => {});
      if (this.#current(generation)) {
        this.#client = undefined; this.#selection = undefined;
        return this.#publish({ phase: 'unavailable', generation, message: failureMessage(error), editorTrusted: selected.workspaceTrusted, workspaceUri: selected.workspaceUri, engineExecutable: selected.executable, limitations: LIMITATIONS });
      }
      return this.#status;
    }
  }
  async #read(client: ConnectionClient, selection: Selection, generation: number): Promise<ConnectionStatus> {
    const root = await (this.#deps.canonicalize ?? canonicalWorkspaceRoot)(selection.workspacePath);
    if (!this.#current(generation) || client !== this.#client) return this.#status;
    const now = this.#deps.now ?? (() => performance.now());
    const deadline = now() + 30_000;
    const timeoutMs = () => {
      const remaining = Math.floor(deadline - now());
      if (remaining <= 0) throw Object.assign(new Error('observer deadline'), { code: 'TIMEOUT' });
      return Math.min(10_000, remaining);
    };
    const host = client.initialized.execution_host;
    if (host.platform !== 'windows') throw new Error('unsupported execution host');
    const workspaceReply = await client.call('workspace/open', { command_id: randomUUID(), host: host.id, root }, { timeoutMs: timeoutMs() });
    const workspace: WorkspaceView = workspaceReply.value;
    if (workspace.workspace !== client.scope.workspace || workspace.host !== host.id || workspace.root !== root) throw new Error('workspace binding mismatch');
    if (workspace.root_id == null || workspace.binding_revision == null) throw new Error('workspace binding projection missing');
    if (this.#role === 'controller') {
      const ownership = await client.call('controller/read', { scope: { ...client.scope } }, { timeoutMs: timeoutMs() });
      if (ownership.value.ownership !== 'this_connection') throw new Error('controller lease lost');
    }
    let subscription: string | undefined;
    let cursor: string | undefined;
    let watermark: Counter | undefined;
    let sequence: Counter | undefined;
    let eventCursor: string | undefined;
    const tasks = new Set<string>();
    const pending = new Set<string>();
    const cursors = new Set<string>();
    let complete = false;
    try {
      for (let pageIndex = 0; pageIndex < 256; pageIndex++) {
        if (!this.#current(generation)) return this.#status;
        const response = await client.call('session/snapshot', { scope: { ...client.scope }, limit: 128, ...(cursor === undefined ? {} : { cursor }) }, { timeoutMs: timeoutMs() });
        if (subscription !== undefined && subscription !== response.value.subscription) throw new Error('snapshot subscription changed');
        subscription = response.value.subscription;
        if (response.kind === 'gap') throw new Error('snapshot gap');
        const page = response.value;
        if (page.session.scope.workspace !== client.scope.workspace || page.session.scope.session !== client.scope.session || (watermark !== undefined && (watermark !== page.watermark || sequence !== page.sequence || eventCursor !== page.event_cursor))) throw new Error('snapshot cut changed');
        watermark = page.watermark; sequence = page.sequence; eventCursor = page.event_cursor;
        for (const task of page.tasks) {
          if (task.scope.workspace !== client.scope.workspace || task.scope.session !== client.scope.session || tasks.has(task.task)) throw new Error('snapshot task scope changed');
          tasks.add(task.task);
          for (const input of task.pending_inputs) pending.add(JSON.stringify([task.task, input.kind, input.id]));
        }
        if (tasks.size > 8192 || pending.size > 8192) throw new Error('observer snapshot limit');
        if (page.complete) { complete = true; break; }
        if (!page.next_cursor || cursors.has(page.next_cursor)) throw new Error('snapshot continuation missing');
        cursor = page.next_cursor; cursors.add(cursor);
      }
      if (!complete || watermark === undefined) throw new Error('snapshot incomplete');
    } finally {
      if (subscription !== undefined) {
        await client.call('events/unsubscribe', { scope: { ...client.scope }, subscription }, { timeoutMs: 2000 });
      }
    }
    if (!this.#current(generation) || this.#client !== client) return this.#status;
    if (!this.#map.bind(generation, selection.workspaceUri, workspace)) return this.#status;
    const reference = client.observerReconnectReference?.();
    if (reference) await this.#remember({ folderUri: selection.workspaceUri, profile: profile(selection), reference });
    if (!this.#current(generation) || this.#client !== client) return this.#status;
    return this.#publish({ phase: 'connected', generation, message: `Connected as ${this.#role}. Status reflects the captured engine snapshot.`, editorTrusted: selection.workspaceTrusted, role: this.#role, workspaceRevision: workspace.revision, workspaceUri: selection.workspaceUri, workspaceRoot: workspace.root, rootId: workspace.root_id, bindingRevision: workspace.binding_revision, engineExecutable: selection.executable, engineBuild: client.initialized.engine_build, protocolVersion: client.initialized.protocol_version, host: Object.freeze({ ...host }), engineTrust: workspace.trust, scope: Object.freeze({ ...client.scope }), taskCount: tasks.size, pendingInputs: pending.size, watermark, limitations: LIMITATIONS });
  }
  setTrust(trusted: boolean): Promise<ConnectionStatus> {
    if (this.#mutation) {
      if (trusted === this.#mutationTrusted) return this.#mutation;
      return this.invalidate('Conflicting trust change while outcome is pending; control released. Reconnect and inspect engine trust before retrying.', this.#status.editorTrusted).then(() => this.#status);
    }
    const client = this.#client; const status = this.#status; const selection = this.#selection;
    if (!client || !selection || status.phase !== 'connected' || this.#role !== 'controller' || status.workspaceRevision === undefined || status.bindingRevision === undefined || (trusted && !selection.workspaceTrusted)) return Promise.reject(new Error('controller and editor trust required'));
    const generation = this.#map.generation;
    this.#mutationTrusted = trusted;
    const pending = (async () => {
      await client.call('workspace/setTrust', {
        scope: { ...client.scope },
        mutation: { command_id: randomUUID(), expected_revision: status.workspaceRevision!, steering_revision: '0' },
        expected_binding_revision: status.bindingRevision!, trusted,
      });
      if (!this.#current(generation) || client !== this.#client) return this.#status;
      // Changing authority invalidates the lease token. Reconnect only as an
      // observer; a later control action must explicitly acquire a fresh lease.
      const reference = client.observerReconnectReference?.();
      if (!reference) { await this.invalidate('Trust changed. Reconnect to inspect current state.', selection.workspaceTrusted); return this.#status; }
      return await this.restore(selection, { folderUri: selection.workspaceUri, profile: profile(selection), reference });
    })().catch(async error => {
      if (this.#current(generation)) await this.invalidate('Trust change outcome requires inspection. Control released; reconnect to read current engine state before another change.', selection.workspaceTrusted, true);
      throw error;
    }).finally(() => { if (this.#mutation === pending) { this.#mutation = undefined; this.#mutationTrusted = undefined; } });
    this.#mutation = pending;
    return pending;
  }
  async editorTrustChanged(trusted: boolean): Promise<ConnectionStatus> {
    if (!trusted && this.#starts.size > 0) {
      await this.invalidate('Editor trust revoked while connecting; reconnect explicitly.', false);
      return this.#status;
    }
    if (this.#selection) this.#selection = { ...this.#selection, workspaceTrusted: trusted };
    this.#publish({ ...this.#status, editorTrusted: trusted });
    if (!trusted && this.#client && this.#role === 'controller') {
      // Loss during an outstanding authority operation fences this connection;
      // a late grant result must not overwrite the editor's revoked trust input.
      if (this.#mutation) {
        await this.invalidate('Editor trust revoked during a pending command; inspect its outcome after reconnecting.', false);
        return this.#status;
      }
      try { return await this.setTrust(false); }
      catch {
        await this.invalidate('Editor trust revoked; controller disconnected and work fenced.', false);
        return this.#status;
      }
    }
    return this.#status;
  }
  async reconcile(selection: Selection, workspaceId: string): Promise<ConnectionStatus> {
    const restriction = connectionRestriction(selection, this.#deps.platform ?? process.platform);
    if (restriction || !this.#deps.rebind) throw new Error(restriction ?? 'reconciliation unavailable');
    // This explicit operation can never close a different client's owner.
    await this.invalidate('Reconciling selected root. Any active owner must close first.', selection.workspaceTrusted);
    const generation = this.#map.generation;
    const root = await (this.#deps.canonicalize ?? canonicalWorkspaceRoot)(selection.workspacePath);
    if (!this.#current(generation)) return this.#status;
    const bound = await this.#deps.rebind({ executable: selection.executable, workspace: root, workspaceId, ...(selection.dataPath === undefined ? {} : { data: selection.dataPath }) });
    if (!this.#current(generation)) return this.#status;
    if (bound.workspace !== workspaceId || bound.root !== root) throw new Error('reconciliation scope changed');
    return this.connect(selection);
  }
  refresh(): Promise<ConnectionStatus> {
    if (this.#refresh) return this.#refresh;
    const client = this.#client; const selection = this.#selection; const generation = this.#map.generation;
    if (!client || !selection || this.#disposed) return Promise.resolve(this.#status);
    const reading = this.#read(client, selection, generation).catch(async error => {
      if (this.#current(generation)) {
        this.#client = undefined; this.#selection = undefined; this.#map.invalidate();
        this.#publish({ phase: 'unavailable', generation: this.#map.generation, message: failureMessage(error), editorTrusted: selection.workspaceTrusted, workspaceUri: selection.workspaceUri, engineExecutable: selection.executable, limitations: LIMITATIONS });
      }
      await client.dispose().catch(() => {}); return this.#status;
    }).finally(() => { if (this.#refresh === reading) this.#refresh = undefined; });
    this.#refresh = reading;
    return reading;
  }
  async invalidate(reason: string, editorTrusted = false, preserveRecovery = false): Promise<void> {
    const client = this.#client;
    this.#controller = undefined;
    this.#client = undefined; this.#selection = undefined; this.#refresh = undefined;
    const generation = this.#map.invalidate();
    this.#publish({ phase: 'disconnected', generation, message: reason, editorTrusted, limitations: LIMITATIONS });
    const closing = client?.dispose().catch(() => {});
    try { if (!preserveRecovery) await this.#remember(undefined); }
    finally { await closing; }
  }
  disconnect(): Promise<void> { this.#controller = undefined; return this.invalidate('Disconnected. Reconnect explicitly to an initialized workspace.', this.#status.editorTrusted); }
  dispose(): Promise<void> {
    if (this.#disposal) return this.#disposal;
    this.#disposed = true;
    this.#disposal = (async () => {
      await this.invalidate('Extension connection disposed.', false, true);
      await this.#persistence.catch(() => {});
      await Promise.allSettled([...this.#starts]);
    })();
    return this.#disposal;
  }
}
