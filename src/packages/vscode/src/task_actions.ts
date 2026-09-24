// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Acceptance, Client, PendingInput, Scope, TaskView } from '@vcp/sdk' with { 'resolution-mode': 'import' };

export type TaskOperation = 'allow' | 'deny' | 'steer' | 'pause' | 'resume' | 'cancel';
export interface TaskAction { readonly id: string; readonly label: string; readonly operation: TaskOperation; readonly input?: string; readonly disabledReason?: string }
export interface PendingTaskCommand {
  readonly version: 1; readonly commandId: string; readonly scope: Scope; readonly task: string;
  readonly operation: TaskOperation; readonly target?: string;
  readonly phase: 'submitting' | 'unknown' | 'accepted' | 'reconciled' | 'rejected';
  readonly revision?: string; readonly watermark?: string;
}
export interface ActionContext {
  readonly generation: number; readonly scope: Scope; readonly controller: boolean; readonly trusted: boolean;
  readonly capabilities: readonly string[]; readonly call: Client['call'];
}
export interface TaskActionDependencies {
  current(): ActionContext | undefined;
  save(records: readonly PendingTaskCommand[]): Promise<void>;
  publish?(): void;
  promptSteering?(task: TaskView): Promise<string | undefined>;
  now?(): number;
}
export interface SteeringObjective { readonly constraints: readonly string[]; readonly acceptance: readonly string[] }
export interface ActionQuestion { readonly input: PendingInput; readonly actionable: boolean; readonly expires_at: string }
type Registered = { action: TaskAction; task: TaskView; input?: PendingInput; expiresAt?: string; objective?: SteeringObjective; generation: number; epoch: number };
const METHODS = { allow: 'approval/respond', deny: 'approval/respond', steer: 'turn/steer', pause: 'turn/pause', resume: 'session/resume', cancel: 'task/cancel' } as const;
const ID = /^[A-Za-z0-9_-]{1,96}$/;
const COUNTER = /^(0|[1-9][0-9]{0,19})$/;
const sameScope = (a: Scope, b: Scope) => a.workspace === b.workspace && a.session === b.session;
const terminal = (record: PendingTaskCommand) => record.phase === 'reconciled' || record.phase === 'rejected';
function unexpired(expiry: string | undefined, now: number): boolean {
  return typeof expiry === 'string' && COUNTER.test(expiry) && Number.isSafeInteger(now) && now >= 0
    && BigInt(expiry) <= 18446744073709551615n && BigInt(expiry) > BigInt(now);
}
function plain(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value)
    && [Object.prototype, null].includes(Object.getPrototypeOf(value))
    && !Object.getOwnPropertySymbols(value).length
    && Object.values(Object.getOwnPropertyDescriptors(value)).every(field => field.enumerable && 'value' in field);
}
function scope(value: unknown): value is Scope {
  return plain(value) && Object.keys(value).sort().join(',') === 'session,workspace'
    && typeof value.workspace === 'string' && ID.test(value.workspace) && typeof value.session === 'string' && ID.test(value.session);
}
/** Only bounded nonsecret receipt references cross the persistence boundary. */
export function pendingTaskCommands(value: unknown): PendingTaskCommand[] {
  if (!Array.isArray(value) || value.length > 256) return [];
  const result: PendingTaskCommand[] = [];
  const identities = new Set<string>();
  for (const item of value) {
    if (!plain(item) || Object.keys(item).some(key => !['version', 'commandId', 'scope', 'task', 'operation', 'target', 'phase', 'revision', 'watermark'].includes(key))
      || item.version !== 1 || typeof item.commandId !== 'string' || !ID.test(item.commandId) || !scope(item.scope)
      || typeof item.task !== 'string' || !ID.test(item.task) || typeof item.operation !== 'string' || !Object.hasOwn(METHODS, item.operation)
      || typeof item.phase !== 'string' || !['submitting', 'unknown', 'accepted', 'reconciled', 'rejected'].includes(item.phase)
      || (item.target !== undefined && (typeof item.target !== 'string' || !ID.test(item.target)))
      || (['allow', 'deny', 'steer', 'pause'].includes(item.operation) && typeof item.target !== 'string')
      || ['revision', 'watermark'].some(key => item[key] !== undefined && (typeof item[key] !== 'string' || !COUNTER.test(item[key] as string) || BigInt(item[key] as string) > 18446744073709551615n))) return [];
    const key = JSON.stringify([item.scope, item.commandId]);
    if (identities.has(key)) return [];
    identities.add(key);
    result.push(structuredClone({ ...item, phase: item.phase === 'submitting' ? 'unknown' : item.phase }) as unknown as PendingTaskCommand);
  }
  return result;
}

/** Registry IDs are presentation handles, never wire methods or engine authority. */
export class TaskActions {
  #registry = new Map<string, Registered>();
  #flights = new Map<string, Promise<void>>();
  #journal: PendingTaskCommand[];
  #writes: Promise<void> = Promise.resolve();
  #epoch = 0;
  constructor(private readonly deps: TaskActionDependencies, saved: unknown = []) { this.#journal = pendingTaskCommands(saved); }
  records(): readonly PendingTaskCommand[] { return structuredClone(this.#journal); }
  invalidate(): void { this.#epoch++; this.#registry.clear(); }
  register(task: TaskView, objective?: SteeringObjective, questions?: readonly ActionQuestion[]): readonly TaskAction[] {
    const context = this.deps.current();
    if (!context || !sameScope(context.scope, task.scope)) return [];
    const criteria = objective && [objective.constraints, objective.acceptance].every(values => Array.isArray(values) && values.length <= 64
      && values.every(value => typeof value === 'string' && value.length > 0 && Buffer.byteLength(value, 'utf8') <= 4096 && !value.includes('\0'))) ? structuredClone(objective) : undefined;
    for (const [id, old] of this.#registry) if (old.task.task === task.task) this.#registry.delete(id);
    const actions: TaskAction[] = [];
    const add = (operation: TaskOperation, label: string, input?: PendingInput) => {
      if (!context.capabilities.includes(METHODS[operation])) return;
      const question = input && questions?.find(row => row.input.id === input.id && row.input.kind === input.kind && row.input.revision === input.revision
        && row.input.operation_digest === input.operation_digest && row.input.effect_revision === input.effect_revision && row.input.policy_revision === input.policy_revision);
      const reason = !context.controller ? 'Connect explicitly as controller.' : !context.trusted ? 'Editor workspace trust is required.'
        : !context.capabilities.includes('command/read') ? 'Durable command reconciliation is unavailable.'
        : operation === 'steer' && !criteria ? 'Current objective criteria are unavailable; refresh task details.'
        : this.#blocked(task, operation, input?.id) ? 'Reconcile the pending command before submitting again.'
        : input && (!input.operation_digest || input.effect_revision == null || input.policy_revision == null || !context.capabilities.includes('approval/source-revisions/1')) ? 'Approval source revisions are unavailable.'
        : input && (!question?.actionable || !unexpired(question.expires_at, this.deps.now?.() ?? Date.now())) ? 'Question is expired or its current validity is unavailable.' : undefined;
      const action: TaskAction = { id: randomUUID(), label, operation, ...(input ? { input: input.id } : {}), ...(reason ? { disabledReason: reason } : {}) };
      this.#registry.set(action.id, { action, task: structuredClone(task), ...(input ? { input: structuredClone(input) } : {}), ...(question ? { expiresAt: question.expires_at } : {}), ...(criteria ? { objective: criteria } : {}), generation: context.generation, epoch: this.#epoch });
      actions.push(action);
    };
    for (const input of task.pending_inputs) if (input.kind === 'approval') { add('allow', 'Allow', input); add('deny', 'Deny', input); }
    if (!['completed', 'failed', 'cancelled'].includes(task.state)) {
      if (task.turn) {
        add('steer', 'Send guidance');
        if (task.state !== 'paused') add('pause', 'Pause');
      }
      if (task.state === 'paused') add('resume', 'Resume');
      add('cancel', 'Cancel task');
    }
    return actions;
  }
  dispatch(message: unknown): Promise<void> {
    if (!plain(message) || Object.keys(message).sort().join(',') !== 'action,id' || message.action !== 'task' || typeof message.id !== 'string' || !ID.test(message.id)) return Promise.reject(new Error('Invalid task action message.'));
    const inflight = this.#flights.get(message.id);
    if (inflight) return inflight;
    const entry = this.#registry.get(message.id);
    if (!entry) return Promise.reject(new Error('Task action expired; refresh the view.'));
    const operation = this.#dispatch(entry);
    this.#flights.set(message.id, operation);
    void operation.finally(() => { this.#flights.delete(message.id as string); this.#registry.delete(message.id as string); }).catch(() => {});
    return operation;
  }
  #context(entry: Registered): ActionContext {
    const current = this.deps.current();
    if (!current || current.generation !== entry.generation || entry.epoch !== this.#epoch || !sameScope(current.scope, entry.task.scope)
      || !current.controller || !current.trusted || !current.capabilities.includes('command/read') || !current.capabilities.includes(METHODS[entry.action.operation])) throw new Error('Task action authority changed; refresh the view.');
    if (entry.input && entry.expiresAt !== undefined && !unexpired(entry.expiresAt, this.deps.now?.() ?? Date.now())) throw new Error('Question expired; refresh before answering.');
    return current;
  }
  #blocked(task: TaskView, operation: TaskOperation, target?: string): boolean {
    return this.#journal.some(record => !terminal(record) && sameScope(record.scope, task.scope) && record.task === task.task
      && (operation === 'pause' || operation === 'cancel' ? record.operation === operation
        : operation === 'allow' || operation === 'deny' ? ((record.operation === 'allow' || record.operation === 'deny') && record.target === target) || record.operation === 'steer'
          : true));
  }
  async #dispatch(entry: Registered): Promise<void> {
    let context = this.#context(entry);
    if (entry.action.disabledReason || this.#blocked(entry.task, entry.action.operation, entry.input?.id)) throw new Error('Task action is disabled; refresh or reconcile first.');
    let text: string | undefined;
    if (entry.action.operation === 'steer') {
      text = await this.deps.promptSteering?.(structuredClone(entry.task));
      if (text === undefined) return;
      if (!text.trim() || Buffer.byteLength(text, 'utf8') > 65536 || text.includes('\0')) throw new Error('Guidance must contain 1–65536 UTF-8 bytes.');
    }
    context = this.#context(entry);
    const read = await context.call('task/read', { scope: entry.task.scope, task: entry.task.task });
    context = this.#context(entry);
    const task = read.value;
    if (read.kind !== 'task' || !sameScope(task.scope, entry.task.scope) || task.task !== entry.task.task || task.revision !== entry.task.revision
      || task.steering_revision !== entry.task.steering_revision || task.turn !== entry.task.turn) throw new Error('Task revision changed; refresh before submitting.');
    if (entry.input) {
      const input = task.pending_inputs.find(value => value.id === entry.input!.id);
      if (!input || input.kind !== 'approval' || input.revision !== entry.input.revision || input.operation_digest !== entry.input.operation_digest
        || input.effect_revision !== entry.input.effect_revision || input.policy_revision !== entry.input.policy_revision) throw new Error('Question changed; refresh before answering.');
    }
    if (this.#blocked(task, entry.action.operation, entry.input?.id)) throw new Error('A command for this action is already pending.');
    this.#journal = this.#journal.filter(record => !terminal(record)).concat(this.#journal.filter(terminal).slice(-127));
    if (this.#journal.length >= 256) throw new Error('Pending command limit; reconcile existing commands first.');
    const command: PendingTaskCommand = { version: 1, commandId: randomUUID(), scope: structuredClone(task.scope), task: task.task, operation: entry.action.operation,
      ...(entry.input ? { target: entry.input.id } : task.turn ? { target: task.turn } : {}), phase: 'submitting' };
    this.#journal.push(command);
    try { await this.#save(); } catch { this.#journal = this.#journal.filter(record => record !== command); throw new Error('Could not save command identity; nothing was submitted.'); }
    // Persistence can outlive a root/lease switch. Never submit to its replacement.
    try { context = this.#context(entry); } catch (error) { await this.#update(command.commandId, { phase: 'rejected' }); throw error; }
    this.deps.publish?.();
    const mutation = { command_id: command.commandId, expected_revision: entry.input?.revision ?? task.revision, steering_revision: task.steering_revision };
    try {
      let receipt: Acceptance;
      switch (entry.action.operation) {
        case 'allow': case 'deny': receipt = (await context.call('approval/respond', { scope: task.scope, mutation, task: task.task, approval: entry.input!.id,
          operation_digest: entry.input!.operation_digest!, effect_revision: entry.input!.effect_revision!, policy_revision: entry.input!.policy_revision!, decision: entry.action.operation })).value; break;
        case 'steer': receipt = (await context.call('turn/steer', { scope: task.scope, mutation, task: task.task, turn: task.turn!, objective: text!, constraints: [...entry.objective!.constraints], acceptance: [...entry.objective!.acceptance] })).value; break;
        case 'pause': receipt = (await context.call('turn/pause', { scope: task.scope, mutation, task: task.task, turn: task.turn!, reason: 'Explicit editor pause' })).value; break;
        case 'resume': receipt = (await context.call('session/resume', { scope: task.scope, mutation, task: task.task })).value; break;
        case 'cancel': receipt = (await context.call('task/cancel', { scope: task.scope, mutation, task: task.task, reason: 'Explicit editor cancellation' })).value; break;
      }
      this.#receipt(command, receipt);
      await this.#update(command.commandId, { phase: 'accepted', revision: receipt.revision, watermark: receipt.watermark });
      if (this.#isCurrent(entry)) { this.deps.publish?.(); await this.#reconcileOne(command, context); }
    } catch (error) {
      const classified = error as { code?: string; classification?: { applicationCode?: string; retry?: string } };
      const rejected = classified?.code === 'rpc' && classified.classification?.retry !== 'reconcile_original'
        && ['POLICY_DENIED', 'APPROVAL_STALE', 'VERSION_CONFLICT', 'CAPABILITY_UNAVAILABLE', 'AUTHORITY_STALE', 'COMMAND_CONFLICT'].includes(classified.classification?.applicationCode ?? '');
      await this.#update(command.commandId, { phase: rejected ? 'rejected' : 'unknown' });
      if (this.#isCurrent(entry)) this.deps.publish?.();
      throw new Error(rejected ? 'Engine rejected this action; refresh before a new submission.' : `Outcome not yet known. Reconcile command ${command.commandId}.`);
    }
  }
  #isCurrent(entry: Registered): boolean { const current = this.deps.current(); return !!current && current.generation === entry.generation && sameScope(current.scope, entry.task.scope); }
  #receipt(command: PendingTaskCommand, receipt: Acceptance): void {
    if (receipt.command_id !== command.commandId || !sameScope(receipt.scope, command.scope) || receipt.task !== command.task || receipt.outcome !== 'accepted') throw new Error('Uncorrelated command receipt.');
  }
  async reconcile(): Promise<void> {
    const current = this.deps.current();
    if (!current || !current.capabilities.includes('command/read')) return;
    for (const command of this.#journal.filter(record => !terminal(record) && sameScope(record.scope, current.scope))) {
      if (this.deps.current()?.generation !== current.generation) return;
      await this.#reconcileOne(command, current);
    }
  }
  async #reconcileOne(command: PendingTaskCommand, context: ActionContext): Promise<void> {
    try {
      const result = await context.call('command/read', { scope: command.scope, command_id: command.commandId });
      this.#receipt(command, result.value);
      await this.#update(command.commandId, { phase: 'reconciled', revision: result.value.revision, watermark: result.value.watermark });
    } catch {
      // A missing/pruned query cannot erase a previously authenticated acceptance.
      const known = this.#journal.find(record => record.commandId === command.commandId);
      await this.#update(command.commandId, { phase: known?.phase === 'accepted' ? 'accepted' : 'unknown' });
    }
    const current = this.deps.current();
    if (current?.generation === context.generation && sameScope(current.scope, command.scope)) this.deps.publish?.();
  }
  async #update(commandId: string, update: Partial<PendingTaskCommand>): Promise<void> {
    this.#journal = this.#journal.map(record => record.commandId === commandId ? { ...record, ...update } : record);
    await this.#save();
  }
  #save(): Promise<void> {
    const records = this.records();
    const next = this.#writes.catch(() => {}).then(() => this.deps.save(records));
    this.#writes = next;
    return next;
  }
}
