// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Acceptance, Client, Scope } from '@vcp/sdk' with { 'resolution-mode': 'import' };

export const INSPECTOR_COMMAND_METHODS = ['routing/reportCapture', 'routing/apply', 'routing/rollback', 'memory/forget', 'backup/create', 'backup/retry', 'backup/cancel'] as const;
export type InspectorCommandMethod = typeof INSPECTOR_COMMAND_METHODS[number];
export type InspectorCommandPhase = 'submitting' | 'unknown' | 'accepted' | 'reconciled' | 'rejected';
export interface InspectorCommandRecord {
  readonly version: 1; readonly commandId: string; readonly scope: Scope;
  readonly task?: string; readonly target?: string; readonly method: InspectorCommandMethod;
  readonly phase: InspectorCommandPhase; readonly revision?: string; readonly watermark?: string;
}
const ID = /^[A-Za-z0-9_-]{1,96}$/;
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
const COUNTER = /^(0|[1-9][0-9]{0,19})$/;
const sameScope = (a: Scope, b: Scope): boolean => a.workspace === b.workspace && a.session === b.session;
const known = (phase: InspectorCommandPhase): boolean => phase === 'accepted' || phase === 'reconciled';
function plain(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value) && Object.getPrototypeOf(value) === Object.prototype && Object.getOwnPropertySymbols(value).length === 0
    && Object.values(Object.getOwnPropertyDescriptors(value)).every(field => field.enumerable && 'value' in field);
}
function scope(value: unknown): value is Scope {
  return plain(value) && Object.keys(value).sort().join(',') === 'session,workspace' && typeof value.workspace === 'string' && ID.test(value.workspace) && typeof value.session === 'string' && ID.test(value.session);
}
function counter(value: unknown): value is string { return typeof value === 'string' && COUNTER.test(value) && BigInt(value) <= 18446744073709551615n; }
/** Only strict nonsecret references survive reload; interrupted sends remain unknown. */
export function inspectorCommandRecords(value: unknown): InspectorCommandRecord[] {
  try {
    if (!Array.isArray(value) || Object.getPrototypeOf(value) !== Array.prototype || value.length > 256 || Object.getOwnPropertySymbols(value).length || Object.getOwnPropertyNames(value).length !== value.length + 1) return [];
    const ids = new Set<string>(); const result: InspectorCommandRecord[] = [];
    for (let index = 0; index < value.length; index++) {
      const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
      if (!descriptor?.enumerable || !('value' in descriptor)) return [];
      const item: unknown = descriptor.value;
      if (!plain(item) || Object.keys(item).some(key => !['version', 'commandId', 'scope', 'task', 'target', 'method', 'phase', 'revision', 'watermark'].includes(key))
        || item.version !== 1 || typeof item.commandId !== 'string' || !UUID.test(item.commandId) || ids.has(item.commandId.toLowerCase()) || !scope(item.scope)
        || typeof item.method !== 'string' || !INSPECTOR_COMMAND_METHODS.includes(item.method as InspectorCommandMethod)
        || typeof item.phase !== 'string' || !['submitting', 'unknown', 'accepted', 'reconciled', 'rejected'].includes(item.phase)
        || ['task', 'target'].some(key => Object.hasOwn(item, key) && (typeof item[key] !== 'string' || !ID.test(item[key] as string)))
        || ['revision', 'watermark'].some(key => Object.hasOwn(item, key) && !counter(item[key]))) return [];
      ids.add(item.commandId.toLowerCase());
      result.push(structuredClone({ ...item, phase: item.phase === 'submitting' ? 'unknown' : item.phase }) as unknown as InspectorCommandRecord);
    }
    return result;
  } catch { return []; }
}

/** One profile owns one writer for the activation lifetime, including late receipts. */
export class InspectorJournal {
  #records: InspectorCommandRecord[];
  #writes: Promise<unknown> = Promise.resolve();
  constructor(private readonly save: (records: readonly InspectorCommandRecord[]) => Promise<void>, saved?: unknown) { this.#records = inspectorCommandRecords(saved); }
  records(): readonly InspectorCommandRecord[] { return structuredClone(this.#records); }
  #serial<T>(operation: () => Promise<T>): Promise<T> {
    const next = this.#writes.catch(() => {}).then(operation); this.#writes = next; return next;
  }
  begin(selectedScope: Scope, method: InspectorCommandMethod, target?: { task?: string; target?: string }): Promise<string> {
    if (!scope(selectedScope) || (target !== undefined && (!plain(target) || Object.keys(target).some(key => !['task', 'target'].includes(key))))) return Promise.reject(new Error('Invalid inspector command metadata.'));
    const command: InspectorCommandRecord = { version: 1, commandId: randomUUID(), scope: structuredClone(selectedScope), method, ...target, phase: 'submitting' };
    if (inspectorCommandRecords([command]).length !== 1) return Promise.reject(new Error('Invalid inspector command metadata.'));
    return this.#serial(async () => {
      const next = this.records().slice();
      if (next.length >= 256) {
        const retired = next.findIndex(record => record.phase === 'reconciled' || record.phase === 'rejected');
        if (retired < 0) throw new Error('Inspector command journal is full; reconcile existing commands.');
        next.splice(retired, 1);
      }
      next.push(command);
      // No identity returns to the sender until this exact metadata is saved.
      await this.save(structuredClone(next)); this.#records = next; return command.commandId;
    });
  }
  settle(commandId: string, phase: Exclude<InspectorCommandPhase, 'submitting'>, receipt?: Acceptance, target?: string): Promise<void> {
    return this.#serial(async () => {
      const index = this.#records.findIndex(record => record.commandId === commandId);
      if (index < 0) throw new Error('Unknown inspector command identity.');
      const record = this.#records[index]!;
      if (!['unknown', 'accepted', 'reconciled', 'rejected'].includes(phase)) throw new Error('Invalid inspector command phase.');
      if (target !== undefined && (!known(phase) || typeof target !== 'string' || !ID.test(target) || (record.target !== undefined && record.target !== target))) throw new Error('Invalid inspector receipt target.');
      if (receipt && !known(phase)) throw new Error('Receipt requires an accepted inspector phase.');
      if (phase === 'accepted' || phase === 'reconciled') {
        if (!receipt || receipt.outcome !== 'accepted' || receipt.command_id !== commandId || !sameScope(receipt.scope, record.scope)
          || (receipt.task ?? undefined) !== record.task || !counter(receipt.revision) || !counter(receipt.watermark)) throw new Error('Uncorrelated inspector command receipt.');
      }
      // Negative/missing evidence cannot erase an authenticated receipt, including
      // one learned before a failed storage update or a concurrent reconciliation.
      if ((known(record.phase) && !known(phase)) || (record.phase === 'rejected' && phase === 'unknown')) return;
      const nextPhase = record.phase === 'reconciled' && phase === 'accepted' ? 'reconciled' : phase;
      this.#records[index] = { ...record, phase: nextPhase, ...(receipt ? { revision: receipt.revision, watermark: receipt.watermark } : {}), ...(target !== undefined ? { target } : {}) };
      await this.save(this.records());
    });
  }
  async reconcile(selectedScope: Scope, call: Client['call']): Promise<void> {
    if (!scope(selectedScope)) throw new Error('Invalid inspector reconciliation scope.');
    for (const record of this.records()) {
      if (!sameScope(record.scope, selectedScope) || record.phase === 'reconciled' || record.phase === 'rejected' || record.phase === 'submitting') continue;
      try {
        const reply = await call('command/read', { scope: record.scope, command_id: record.commandId });
        if (reply.kind !== 'acceptance') throw new Error('Command receipt unavailable.');
        await this.settle(record.commandId, 'reconciled', reply.value);
      } catch {
        // Missing/pruned/unavailable receipt is not authorization to resubmit.
        await this.settle(record.commandId, 'unknown');
      }
    }
  }
}
