// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { EvidenceReference, TaskPresentation, UsageView } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import type { ConnectionClient, ConnectionStatus } from './engine_connection.js';
import { TaskProjection } from './task_projection.js';
import { TaskActions } from './task_actions.js';
import type { TaskPanelMessage, TaskPanelState } from './task_view_model.js';

export interface TaskSessionDependencies {
  actions: TaskActions;
  publish(state: TaskPanelState): void;
}
/** Owns a bounded live view, not an execution engine. Every invalidation returns
 * to canonical reads; no event outcome is used to manufacture task completion. */
export class TaskSession {
  #deps: TaskSessionDependencies;
  #client: ConnectionClient | undefined;
  #status: ConnectionStatus | undefined;
  #epoch = 0;
  #projection: TaskProjection | undefined;
  #subscription: string | undefined;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #work: Promise<void> | undefined;
  #offset = 0;
  #selected: string | undefined;
  #detail: TaskPresentation | undefined;
  #usage: UsageView | undefined;
  #owner = 'unknown';
  #notice = 'Connect to inspect tasks.';
  #select = new Map<string, string>();
  #evidence = new Map<string, EvidenceReference>();
  #history: { id: string; cursor: string } | undefined;
  #detailGeneration = 0;
  #artifact: string | undefined;
  #state: TaskPanelState = { phase: 'disconnected', message: 'Connect to inspect tasks.', owner: 'unknown', total: 0, offset: 0, hasMore: false, rows: [], actions: [], commands: [] };
  constructor(dependencies: TaskSessionDependencies) { this.#deps = dependencies; }
  state(): TaskPanelState { return structuredClone(this.#state); }
  connection(status: ConnectionStatus, client: ConnectionClient | undefined): void {
    const changed = client !== this.#client || status.generation !== this.#status?.generation;
    this.#status = status;
    if (!changed && status.phase === 'connected') return;
    this.#stop(); this.#client = status.phase === 'connected' ? client : undefined;
    this.#offset = 0; this.#selected = undefined; this.#projection = undefined;
    this.#clearContent();
    if (!this.#client) { this.#notice = 'Disconnected. No current task result is available.'; this.#publish(); return; }
    const methods = this.#client.initialized.methods;
    if (!['task/presentation', 'events/next', 'controller/read', 'usage/read'].every(method => methods.includes(method))) {
      this.#notice = 'Task views require a matching engine with task presentation support.'; this.#publish(); return;
    }
    this.#notice = 'Reading canonical task state.';
    void this.refresh();
  }
  #valid(epoch: number, client: ConnectionClient): boolean { return this.#epoch === epoch && this.#client === client && this.#status?.phase === 'connected'; }
  #clearContent(fence = true, keepSelection = false): void { this.#detailGeneration++; this.#detail = undefined; this.#artifact = undefined; this.#usage = undefined; if (!keepSelection) this.#select.clear(); this.#evidence.clear(); this.#history = undefined; if (fence) this.#deps.actions.invalidate(); }
  #stop(): void {
    this.#owner = 'unknown';
    this.#epoch++; clearTimeout(this.#timer); this.#timer = undefined;
    const client = this.#client; const subscription = this.#subscription;
    this.#subscription = undefined; this.#work = undefined;
    if (client && subscription) void client.call('events/unsubscribe', { scope: { ...client.scope }, subscription }, { timeoutMs: 2000 }).catch(() => {});
  }
  #publish(): void {
    const view = this.#projection?.view(this.#offset, 20);
    this.#select.clear();
    const rows = (view?.rows ?? []).map(row => { const actionId = randomUUID(); this.#select.set(actionId, row.task); return { ...row, actionId }; });
    const state: TaskPanelState = {
      phase: this.#client ? view?.phase ?? 'partial' : 'disconnected', message: this.#notice, owner: this.#owner,
      total: view?.total ?? 0, offset: this.#offset, hasMore: view?.hasMore ?? false, rows,
      ...(view ? { stateCounts: view.stateCounts } : {}),
      ...(this.#detail ? { detail: this.#detail } : {}), ...(this.#usage ? { usage: this.#usage } : {}),
      actions: this.#detail && view?.phase === 'current' ? this.#deps.actions.register(this.#detail.task,
        this.#detail.objective_constraints && this.#detail.objective_acceptance
          ? { constraints: this.#detail.objective_constraints, acceptance: this.#detail.objective_acceptance } : undefined, this.#detail.questions) : [],
      evidence: [...this.#evidence].map(([id, reference]) => ({ id, label: `Read artifact ${reference.artifact} at ${reference.offset}` })),
      ...(this.#history ? { historyId: this.#history.id } : {}),
      ...(this.#artifact ? { artifact: this.#artifact } : {}),
      commands: this.#deps.actions.records().filter(row => row.scope.workspace === this.#client?.scope.workspace && row.scope.session === this.#client?.scope.session).map(row => ({ commandId: row.commandId, operation: row.operation, phase: row.phase })),
    };
    this.#state = state; this.#deps.publish(state);
  }
  notifyActions(): void { this.#publish(); }
  refresh(): Promise<void> {
    if (this.#work) return this.#work;
    const client = this.#client; if (!client) return Promise.resolve();
    clearTimeout(this.#timer);
    const epoch = this.#epoch;
    const work = this.#snapshot(client, epoch).catch(() => {
      if (!this.#valid(epoch, client)) return;
      this.#release(client); this.#owner = 'unknown';
      this.#projection?.disconnect(); this.#clearContent(); this.#notice = 'Task stream unavailable. Refresh to query current engine state; completion is unknown.'; this.#publish();
    }).finally(() => { if (this.#work === work) this.#work = undefined; });
    this.#work = work; return work;
  }
  #release(client: ConnectionClient): void {
    const subscription = this.#subscription; this.#subscription = undefined;
    if (subscription) void client.call('events/unsubscribe', { scope: { ...client.scope }, subscription }, { timeoutMs: 2000 }).catch(() => {});
  }
  async #snapshot(client: ConnectionClient, epoch: number): Promise<void> {
    const previous = this.#subscription; this.#subscription = undefined;
    if (previous) await client.call('events/unsubscribe', { scope: { ...client.scope }, subscription: previous }, { timeoutMs: 2000 });
    if (!this.#valid(epoch, client)) return;
    this.#clearContent(false);
    const projection = new TaskProjection(client.scope); this.#projection = projection;
    let cursor: string | undefined; let eventCursor: string | undefined;
    const deadline = Date.now() + 30_000;
    for (let pages = 0; pages < 65; pages++) {
      const timeoutMs = Math.min(10_000, deadline - Date.now());
      if (timeoutMs <= 0) throw new Error('task snapshot deadline');
      const reply = await client.call('session/snapshot', { scope: { ...client.scope }, limit: 128, ...(cursor ? { cursor } : {}) }, { timeoutMs });
      if (!this.#valid(epoch, client)) { await client.call('events/unsubscribe', { scope: { ...client.scope }, subscription: reply.value.subscription }).catch(() => {}); return; }
      this.#subscription = reply.value.subscription;
      if (reply.kind === 'gap') { projection.markGap(); this.#notice = 'Snapshot changed. Refresh to resynchronize.'; this.#publish(); return; }
      projection.appendSnapshot(reply.value); this.#publish();
      if (reply.value.complete) { eventCursor = reply.value.event_cursor; break; }
      cursor = reply.value.next_cursor ?? undefined;
    }
    if (!eventCursor) throw new Error('task snapshot bound');
    const ownership = await client.call('controller/read', { scope: { ...client.scope } });
    if (!this.#valid(epoch, client)) return;
    this.#owner = ownership.value.ownership;
    const view = projection.view(this.#offset, 20);
    if (!view.rows.some(row => row.task === this.#selected)) { this.#deps.actions.invalidate(); this.#selected = view.rows[0]?.task; }
    await this.#details(client, epoch);
    if (!this.#valid(epoch, client)) return;
    await this.#deps.actions.reconcile();
    if (!this.#valid(epoch, client)) return;
    this.#notice = 'Canonical task state; new events trigger refresh. Paused work requires deliberate resume.'; this.#publish();
    const subscription = this.#subscription!;
    this.#timer = setTimeout(() => { void this.#poll(client, epoch, subscription, eventCursor!); }, 500);
  }
  async #poll(client: ConnectionClient, epoch: number, subscription: string, cursor: string): Promise<void> {
    if (!this.#valid(epoch, client) || subscription !== this.#subscription) return;
    try {
      const reply = await client.call('events/next', { scope: { ...client.scope }, subscription, cursor });
      if (!this.#valid(epoch, client) || subscription !== this.#subscription) return;
      if (reply.kind === 'gap') {
        this.#projection?.markGap(); this.#clearContent(); this.#notice = 'Event gap: resynchronizing from current engine state.'; this.#publish(); await this.refresh(); return;
      }
      this.#projection?.applyEvents(reply.value);
      if (reply.value.events.length) { this.#clearContent(false); this.#notice = 'Engine state changed; refreshing.'; this.#publish(); await this.refresh(); return; }
      this.#timer = setTimeout(() => { void this.#poll(client, epoch, subscription, reply.value.cursor); }, 500);
    } catch {
      if (this.#valid(epoch, client)) { this.#release(client); this.#owner = 'unknown'; this.#projection?.disconnect(); this.#clearContent(); this.#notice = 'Task stream disconnected. Refresh to inspect the engine outcome.'; this.#publish(); }
    }
  }
  async #details(client: ConnectionClient, epoch: number, cursor?: string): Promise<void> {
    const task = this.#selected; if (!task) return;
    this.#clearContent(false, true); const detailGeneration = this.#detailGeneration;
    const reply = await client.call('task/presentation', { scope: { ...client.scope }, task, target: null, cursor: cursor ?? null, limit: 32 });
    if (!this.#valid(epoch, client) || task !== this.#selected || detailGeneration !== this.#detailGeneration) return;
    const detail = reply.value;
    if (detail.task.task !== task || detail.task.scope.workspace !== client.scope.workspace || detail.task.scope.session !== client.scope.session) throw new Error('task presentation scope mismatch');
    this.#detail = detail;
    for (const row of detail.rows) if ('evidence' in row) {
      const references = Array.isArray(row.evidence) ? row.evidence : row.evidence ? [row.evidence] : [];
      for (const reference of references) {
        if (this.#evidence.size >= 128) break;
        this.#evidence.set(randomUUID(), reference);
      }
    }
    if (detail.next_cursor) this.#history = { id: randomUUID(), cursor: detail.next_cursor };
    const usage = await client.call('usage/read', { scope: { ...client.scope }, task, limit: 1 }).catch(() => undefined);
    if (!this.#valid(epoch, client) || task !== this.#selected || detailGeneration !== this.#detailGeneration) return;
    if (usage && usage.value.scope.workspace === client.scope.workspace && usage.value.scope.session === client.scope.session && usage.value.task === task) this.#usage = usage.value;
  }
  async dispatch(message: TaskPanelMessage): Promise<void> {
    if (message.action === 'ready') { this.#publish(); return; }
    if (message.action === 'refresh') { await this.refresh(); return; }
    const client = this.#client; const epoch = this.#epoch;
    if (!client || this.#projection?.view().phase !== 'current') return;
    let expectedDetailGeneration = this.#detailGeneration;
    try {
      if (message.action === 'previous' || message.action === 'next') {
        const view = this.#projection.view(this.#offset, 20);
        if (message.action === 'next' && !view.hasMore) return;
        this.#deps.actions.invalidate();
        this.#offset = message.action === 'next' ? this.#offset + 20 : Math.max(0, this.#offset - 20);
        this.#selected = this.#projection.view(this.#offset, 20).rows[0]?.task;
        expectedDetailGeneration = this.#detailGeneration + 1;
        await this.#details(client, epoch);
      } else if (message.action === 'select') {
        const task = this.#select.get(message.id); if (!task) return;
        if (task !== this.#selected) this.#deps.actions.invalidate();
        this.#selected = task; expectedDetailGeneration = this.#detailGeneration + 1; await this.#details(client, epoch);
      } else if (message.action === 'history') {
        if (message.id !== this.#history?.id) return;
        expectedDetailGeneration = this.#detailGeneration + 1;
        await this.#details(client, epoch, this.#history.cursor);
      } else if (message.action === 'task') {
        await this.#deps.actions.dispatch(message); await this.refresh();
      } else if (message.action === 'evidence') {
        const reference = this.#evidence.get(message.id); const task = this.#selected;
        if (!reference || !task) return;
        const detailGeneration = this.#detailGeneration;
        const length = Number(BigInt(reference.length) > 65536n ? 65536n : BigInt(reference.length));
        const result = await client.call('artifact/read', { scope: { ...client.scope }, task, artifact: reference.artifact, offset: reference.offset, length: Math.max(1, length) });
        if (!this.#valid(epoch, client) || this.#selected !== task || detailGeneration !== this.#detailGeneration) return;
        if (result.value.artifact !== reference.artifact || result.value.offset !== reference.offset || result.value.sha256 !== reference.sha256) throw new Error('artifact presentation unavailable');
        const bytes = Buffer.from(result.value.content, result.value.encoding === 'base64' ? 'base64' : 'utf8');
        if (bytes.length > length || (result.value.encoding === 'base64' && bytes.toString('base64') !== result.value.content)) throw new Error('artifact range invalid');
        const next = BigInt(reference.offset) + BigInt(bytes.length);
        const end = BigInt(reference.offset) + BigInt(reference.length);
        if (next > BigInt(result.value.total_bytes) || next > end) throw new Error('artifact range exceeds reference');
        this.#artifact = `Artifact ${reference.artifact}, bytes ${reference.offset}–${next}. UTF-8 preview.\n${new TextDecoder().decode(bytes).replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, '\uFFFD')}`;
        if (bytes.length && next < end && next < BigInt(result.value.total_bytes)) {
          this.#evidence.delete(message.id);
          this.#evidence.set(randomUUID(), { ...reference, offset: next.toString(), length: (end - next).toString() });
        }
      }
      if (this.#valid(epoch, client)) this.#publish();
    } catch {
      if (this.#valid(epoch, client) && expectedDetailGeneration === this.#detailGeneration) { this.#clearContent(); this.#notice = 'Action unavailable or stale. Refresh to inspect current engine state; no action was replayed.'; this.#publish(); }
    }
  }
  dispose(): void { this.#stop(); this.#client = undefined; this.#projection = undefined; this.#clearContent(); }
}
