// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Client, Counter, InitializeParams, LaunchOptions, Scope, Trust, WorkspaceView } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import { connectionRestriction, type ConnectionSelection } from './trust.js';
import { canonicalWorkspaceRoot, WorkspaceMap } from './workspace_map.js';

export type Selection = ConnectionSelection;
export type ConnectionClient = Pick<Client, 'scope' | 'initialized' | 'call' | 'dispose'>;
export interface ConnectionStatus {
  readonly phase: 'disconnected' | 'connecting' | 'connected' | 'unavailable';
  readonly generation: number;
  readonly message: string;
  readonly editorTrusted: boolean;
  readonly role?: 'observer';
  readonly workspaceUri?: string;
  readonly workspaceRoot?: string;
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
  publish?(status: ConnectionStatus): void;
  canonicalize?(path: string): Promise<string>;
  platform?: string;
  now?(): number;
}
const LIMITATIONS = Object.freeze([
  'Task controls are not available in this version.',
  'Connect to an existing initialized workspace.',
  'Moved workspaces must be reconciled with the CLI before connecting.',
  'Refresh to read current state; reconnect explicitly after disconnecting.',
]);
const INITIALIZE: InitializeParams = {
  protocol_version: '1.0', client: { name: 'vcp-vscode', version: '0.1.0' },
  capabilities: ['approval/source-revisions/1'],
  required_capabilities: ['jsonrpc/2.0', 'workspace/open', 'session/snapshot', 'events/unsubscribe'],
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
  #refresh: Promise<ConnectionStatus> | undefined;
  #starts = new Set<Promise<ConnectionStatus>>();
  #disposal: Promise<void> | undefined;
  #status: ConnectionStatus = Object.freeze({ phase: 'disconnected', generation: 0, message: 'Select an initialized workspace and connect explicitly.', editorTrusted: false, limitations: LIMITATIONS });
  constructor(dependencies: ConnectionDependencies) { this.#deps = dependencies; }
  state(): ConnectionStatus { return this.#status; }
  #publish(status: ConnectionStatus): ConnectionStatus {
    this.#status = Object.freeze(status);
    this.#deps.publish?.(this.#status);
    return this.#status;
  }
  #current(generation: number): boolean { return !this.#disposed && generation === this.#map.generation; }
  connect(selection: Selection): Promise<ConnectionStatus> {
    const pending = this.#connect(selection, [...this.#starts]);
    this.#starts.add(pending);
    void pending.finally(() => this.#starts.delete(pending)).catch(() => {});
    return pending;
  }
  async #connect(selection: Selection, previousStarts: Promise<ConnectionStatus>[]): Promise<ConnectionStatus> {
    if (this.#disposed) return this.#status;
    const selected = { ...selection };
    const generation = this.#map.invalidate();
    const previous = this.#client;
    this.#client = undefined; this.#selection = undefined; this.#refresh = undefined;
    const restriction = connectionRestriction(selected, this.#deps.platform ?? process.platform);
    this.#publish({ phase: restriction ? 'unavailable' : 'connecting', generation, message: restriction ?? 'Connecting to the selected workspace as an observer.', editorTrusted: selected.workspaceTrusted, workspaceUri: selected.workspaceUri, engineExecutable: selected.executable, limitations: LIMITATIONS });
    if (previous) await previous.dispose().catch(() => {});
    await Promise.allSettled(previousStarts);
    if (!this.#current(generation) || restriction) return this.#status;
    let client: ConnectionClient | undefined;
    try {
      const root = await (this.#deps.canonicalize ?? canonicalWorkspaceRoot)(selected.workspacePath);
      if (!this.#current(generation)) return this.#status;
      client = await this.#deps.launch({ executable: selected.executable, workspace: root, ...(selected.dataPath === undefined ? {} : { data: selected.dataPath }), role: 'observer', transport: 'stdio', initialize: structuredClone(INITIALIZE) });
      if (!this.#current(generation)) { await client.dispose(); return this.#status; }
      this.#client = client; this.#selection = { ...selected, workspacePath: root };
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
    const now = this.#deps.now ?? (() => performance.now());
    const deadline = now() + 30_000;
    const timeoutMs = () => {
      const remaining = Math.floor(deadline - now());
      if (remaining <= 0) throw Object.assign(new Error('observer deadline'), { code: 'TIMEOUT' });
      return Math.min(10_000, remaining);
    };
    const host = client.initialized.execution_host;
    if (host.platform !== 'windows') throw new Error('unsupported execution host');
    const workspaceReply = await client.call('workspace/open', { command_id: randomUUID(), host: host.id, root: selection.workspacePath }, { timeoutMs: timeoutMs() });
    const workspace: WorkspaceView = workspaceReply.value;
    if (workspace.workspace !== client.scope.workspace || workspace.host !== host.id || workspace.root !== selection.workspacePath) throw new Error('workspace binding mismatch');
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
    return this.#publish({ phase: 'connected', generation, message: 'Connected as observer. Status reflects the captured engine snapshot.', editorTrusted: selection.workspaceTrusted, role: 'observer', workspaceUri: selection.workspaceUri, workspaceRoot: workspace.root, engineExecutable: selection.executable, engineBuild: client.initialized.engine_build, protocolVersion: client.initialized.protocol_version, host: Object.freeze({ ...host }), engineTrust: workspace.trust, scope: Object.freeze({ ...client.scope }), taskCount: tasks.size, pendingInputs: pending.size, watermark, limitations: LIMITATIONS });
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
  async invalidate(reason: string, editorTrusted = false): Promise<void> {
    const client = this.#client;
    this.#client = undefined; this.#selection = undefined; this.#refresh = undefined;
    const generation = this.#map.invalidate();
    this.#publish({ phase: 'disconnected', generation, message: reason, editorTrusted, limitations: LIMITATIONS });
    if (client) await client.dispose().catch(() => {});
  }
  disconnect(): Promise<void> { return this.invalidate('Disconnected. Reconnect explicitly to an initialized workspace.', this.#status.editorTrusted); }
  dispose(): Promise<void> {
    if (this.#disposal) return this.#disposal;
    this.#disposed = true;
    this.#disposal = (async () => {
      await this.invalidate('Extension connection disposed.');
      await Promise.allSettled([...this.#starts]);
    })();
    return this.#disposal;
  }
}
