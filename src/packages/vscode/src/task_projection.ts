// SPDX-License-Identifier: Apache-2.0
import type { Counter, EventBatch, PendingInput, Scope, SessionSnapshot, TaskStatus, TaskView } from '@vcp/sdk' with { 'resolution-mode': 'import' };

const MAX_TASKS = 8192;
const MAX_EVENTS = 128;
const EVENTS_PER_TASK = 8;
const STATES: readonly TaskStatus[] = ['pending', 'running', 'waiting_for_input', 'blocked', 'paused', 'completed', 'failed', 'cancelled'];
export type ProjectionPhase = 'partial' | 'current' | 'stale' | 'gap' | 'disconnected';
export interface EventSummary { readonly id: string; readonly sequence: Counter; readonly kind: string; readonly redacted: boolean }
export interface TaskRow {
  readonly task: string;
  readonly root: string;
  readonly parent?: string;
  readonly state: TaskStatus;
  readonly effects: TaskView['effects'];
  readonly reason: string;
  readonly revision: Counter;
  readonly steeringRevision: Counter;
  readonly pendingInputs: readonly Pick<PendingInput, 'id' | 'kind' | 'revision'>[];
  readonly dirty: boolean;
  readonly events: readonly EventSummary[];
}
export interface TaskProjectionView {
  readonly scope: Readonly<Scope>;
  readonly phase: ProjectionPhase;
  readonly needsResync: boolean;
  readonly watermark?: Counter;
  readonly sequence?: Counter;
  readonly total: number;
  readonly offset: number;
  readonly hasMore: boolean;
  readonly stateCounts: Readonly<Partial<Record<TaskStatus, number>>>;
  readonly rows: readonly TaskRow[];
}
function id(value: unknown): asserts value is string {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]{1,96}$/.test(value)) throw new Error('invalid task projection identity');
}
function counter(value: unknown): bigint {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value)) throw new Error('invalid task projection counter');
  const result = BigInt(value);
  if (result > 18446744073709551615n) throw new Error('task projection counter overflow');
  return result;
}
function token(value: unknown): asserts value is string {
  if (typeof value !== 'string' || !value.length || value.length > 4096) throw new Error('invalid projection cursor');
}
function text(value: unknown, maximum: number): string {
  if (typeof value !== 'string' || value.length > 65536) throw new Error('invalid task projection text');
  return value.slice(0, maximum);
}

/** Canonical task state comes only from snapshots. Events invalidate it; their
 * outcome and end-of-stream flag never manufacture a terminal task result. */
export class TaskProjection {
  readonly #scope: Scope;
  #phase: ProjectionPhase = 'partial';
  #rows = new Map<string, TaskRow>();
  #cut: { subscription: string; watermark: Counter; sequence: Counter; cursor: string; sessionRevision: Counter; configurationRevision: Counter } | undefined;
  #complete = false;
  #cursors = new Set<string>();
  #sequence: Counter | undefined;
  #eventCut = 0n;
  #eventCursor: string | undefined;
  constructor(scope: Scope) {
    id(scope.workspace); id(scope.session);
    this.#scope = { workspace: scope.workspace, session: scope.session };
  }
  #own(scope: Scope): void {
    if (!scope || scope.workspace !== this.#scope.workspace || scope.session !== this.#scope.session) throw new Error('task projection scope mismatch');
  }
  #row(task: TaskView): TaskRow {
    this.#own(task.scope); id(task.task); id(task.root);
    if (task.parent != null) id(task.parent);
    counter(task.revision); counter(task.steering_revision);
    if (!STATES.includes(task.state) || !['pending', 'known', 'partial', 'unknown'].includes(task.effects)) throw new Error('invalid canonical task state');
    if (!Array.isArray(task.pending_inputs) || task.pending_inputs.length > 128) throw new Error('pending input bound');
    const inputs = new Set<string>();
    const pendingInputs = task.pending_inputs.map(input => {
      id(input.id); counter(input.revision);
      if (!['approval', 'question', 'reconciliation'].includes(input.kind) || inputs.has(`${input.kind}:${input.id}`)) throw new Error('invalid pending input identity');
      inputs.add(`${input.kind}:${input.id}`);
      return { id: input.id, kind: input.kind, revision: input.revision };
    });
    return { task: task.task, root: task.root, ...(task.parent == null ? {} : { parent: task.parent }), state: task.state, effects: task.effects, reason: text(task.reason, 512), revision: task.revision, steeringRevision: task.steering_revision, pendingInputs, dirty: false, events: [] };
  }
  appendSnapshot(page: SessionSnapshot): void {
    try {
      if (this.#complete || this.#phase !== 'partial' || this.#cursors.size >= 256) throw new Error('snapshot requires a new projection or exceeds page bound');
      this.#own(page.session.scope); id(page.subscription);
      counter(page.watermark); counter(page.sequence); counter(page.session.revision); counter(page.session.configuration_revision); token(page.event_cursor);
      if (typeof page.complete !== 'boolean' || !Array.isArray(page.tasks) || page.tasks.length > 128 || this.#rows.size + page.tasks.length > MAX_TASKS) throw new Error('snapshot bound');
      const cut = { subscription: page.subscription, watermark: page.watermark, sequence: page.sequence, cursor: page.event_cursor, sessionRevision: page.session.revision, configurationRevision: page.session.configuration_revision };
      if (this.#cut && JSON.stringify(cut) !== JSON.stringify(this.#cut)) throw new Error('snapshot cut changed');
      if (!page.complete) { token(page.next_cursor); if (this.#cursors.has(page.next_cursor)) throw new Error('snapshot cursor repeated'); }
      else if (page.next_cursor != null) throw new Error('completed snapshot has continuation');
      const staged = new Map(this.#rows);
      for (const task of page.tasks) {
        const row = this.#row(task);
        if (staged.has(row.task)) throw new Error('duplicate canonical task');
        staged.set(row.task, row);
      }
      let pendingCount = 0;
      for (const row of staged.values()) pendingCount += row.pendingInputs.length;
      if (pendingCount > 8192) throw new Error('snapshot pending input bound');
      if (page.complete) this.#validateTree(staged);
      this.#cut = cut; this.#rows = staged; this.#complete = page.complete; this.#sequence = page.sequence;
      this.#eventCut = counter(page.sequence); this.#eventCursor = page.event_cursor;
      if (!page.complete) this.#cursors.add(page.next_cursor!);
      this.#phase = page.complete ? 'current' : 'partial';
    } catch (error) { this.#phase = 'gap'; throw error; }
  }
  #validateTree(rows: Map<string, TaskRow>): void {
    const checked = new Set<string>();
    for (const row of rows.values()) {
      const root = rows.get(row.root);
      if (!root || root.root !== root.task || root.parent !== undefined) throw new Error('missing canonical root');
      const path = new Set<string>();
      let current: TaskRow | undefined = row;
      while (current && !checked.has(current.task)) {
        if (path.has(current.task) || current.root !== row.root) throw new Error('invalid task ancestry');
        path.add(current.task);
        if (!current.parent) { if (current.task !== row.root) throw new Error('disconnected task ancestry'); break; }
        current = rows.get(current.parent);
        if (!current) throw new Error('missing canonical parent');
      }
      for (const task of path) checked.add(task);
    }
  }
  applyEvents(batch: EventBatch): void {
    try {
      if (!this.#complete || this.#phase === 'gap' || this.#phase === 'disconnected' || !this.#cut || !this.#sequence) throw new Error('events require a complete live snapshot');
      id(batch.subscription); token(batch.cursor);
      const end = counter(batch.snapshot_sequence);
      if (batch.subscription !== this.#cut.subscription || end < this.#eventCut || typeof batch.at_end !== 'boolean' || !Array.isArray(batch.events) || batch.events.length > MAX_EVENTS) throw new Error('event batch mismatch');
      // Empty caught-up reads may repeat an opaque cursor (including an
      // idempotent retry). Only pages that claim progress must advance it.
      if (batch.cursor === this.#eventCursor && (batch.events.length > 0 || !batch.at_end)) throw new Error('event cursor did not advance');
      let sequence = counter(this.#sequence);
      const staged = new Map(this.#rows);
      const ids = new Set<string>();
      for (const event of batch.events) {
        this.#own(event.scope); id(event.id);
        const next = counter(event.sequence); counter(event.timestamp_ms);
        if (next <= sequence || next > end || ids.has(event.id) || typeof event.redacted !== 'boolean') throw new Error('event ordering mismatch');
        ids.add(event.id); sequence = next;
        const summary: EventSummary = { id: event.id, sequence: event.sequence, kind: text(event.kind, 128), redacted: event.redacted };
        if (event.task != null) {
          id(event.task);
          const row = staged.get(event.task);
          if (row) staged.set(row.task, { ...row, dirty: true, events: [...row.events, summary].slice(-EVENTS_PER_TASK) });
        } else {
          for (const row of staged.values()) staged.set(row.task, { ...row, dirty: true });
        }
      }
      this.#rows = staged; this.#sequence = sequence.toString(); this.#eventCut = end; this.#eventCursor = batch.cursor;
      if (batch.events.length) this.#phase = 'stale';
    } catch (error) { this.#phase = 'gap'; throw error; }
  }
  markGap(): void { this.#phase = 'gap'; }
  disconnect(): void { this.#phase = 'disconnected'; }
  view(offset = 0, limit = 50): TaskProjectionView {
    if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(limit) || limit < 1 || limit > 100) throw new Error('invalid task page selection');
    // Status rows have stable identity order independent of event volume.
    const compare = (a: string, b: string) => a < b ? -1 : a > b ? 1 : 0;
    const rows = [...this.#rows.values()].sort((a, b) => compare(a.root, b.root) || Number(a.parent !== undefined) - Number(b.parent !== undefined) || compare(a.task, b.task));
    const stateCounts: Partial<Record<TaskStatus, number>> = {};
    for (const row of rows) stateCounts[row.state] = (stateCounts[row.state] ?? 0) + 1;
    return structuredClone({ scope: this.#scope, phase: this.#phase, needsResync: this.#phase !== 'current', ...(this.#cut ? { watermark: this.#cut.watermark } : {}), ...(this.#sequence === undefined ? {} : { sequence: this.#sequence }), total: rows.length, offset, hasMore: offset + limit < rows.length, stateCounts, rows: rows.slice(offset, offset + limit) });
  }
}
