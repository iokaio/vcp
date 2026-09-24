// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import type { Client, Scope } from '@vcp/sdk' with { 'resolution-mode': 'import' };

export type EditorCommandKind = 'start' | 'context' | 'prepare' | 'dispatch' | 'result';
export interface EditorCommandRecord {
  version: 1; id: string; scope: Scope; task: string; kind: EditorCommandKind;
  change?: string; file?: number; phase: 'pending' | 'accepted' | 'unknown' | 'resolved';
}
const ID = /^[A-Za-z0-9_-]{1,96}$/;
function plain(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value) && Object.getPrototypeOf(value) === Object.prototype
    && !Object.getOwnPropertySymbols(value).length
    && Object.values(Object.getOwnPropertyDescriptors(value)).every(field => 'value' in field && field.enumerable);
}
/** Only identities survive reload. Original edit/credential/draft bytes never do. */
export function editorCommandRecords(value: unknown): EditorCommandRecord[] {
  if (!Array.isArray(value) || value.length > 128) return [];
  const ids = new Set<string>();
  for (const record of value) {
    if (!plain(record)
      || Object.keys(record).some(key => !['version', 'id', 'scope', 'task', 'kind', 'change', 'file', 'phase'].includes(key))
      || record.version !== 1 || typeof record.id !== 'string' || !ID.test(record.id) || typeof record.task !== 'string' || !ID.test(record.task)
      || typeof record.kind !== 'string' || !['start', 'context', 'prepare', 'dispatch', 'result'].includes(record.kind)
      || typeof record.phase !== 'string' || !['pending', 'accepted', 'unknown', 'resolved'].includes(record.phase)
      || !plain(record.scope) || Object.keys(record.scope).sort().join(',') !== 'session,workspace'
      || typeof record.scope.workspace !== 'string' || !ID.test(record.scope.workspace)
      || typeof record.scope.session !== 'string' || !ID.test(record.scope.session)
      || (record.change !== undefined && (typeof record.change !== 'string' || !ID.test(record.change)))
      || (record.file !== undefined && (typeof record.file !== 'number' || !Number.isInteger(record.file) || record.file < 0 || record.file >= 16))
      || ids.has(record.id)) return [];
    ids.add(record.id);
  }
  return structuredClone(value).map(record => ({ ...record, phase: record.phase === 'pending' ? 'unknown' : record.phase }));
}

/** A profile owns one instance for the activation lifetime, including late writes. */
export class EditorJournal {
  #records: EditorCommandRecord[];
  #writes: Promise<void> = Promise.resolve();
  constructor(private readonly save: (records: readonly EditorCommandRecord[]) => Promise<void>, saved?: unknown) {
    this.#records = editorCommandRecords(saved);
  }
  records(): readonly EditorCommandRecord[] { return structuredClone(this.#records); }
  #persist(): Promise<void> {
    const snapshot = this.records();
    const write = this.#writes.catch(() => {}).then(() => this.save(snapshot));
    this.#writes = write;
    return write;
  }
  async begin(scope: Scope, task: string, kind: EditorCommandKind, target?: { change: string; file?: number }): Promise<string> {
    if (this.#records.length >= 128) {
      const index = this.#records.findIndex(record => record.phase === 'resolved' || (record.phase === 'accepted' && ['context', 'start'].includes(record.kind)));
      if (index < 0) throw new Error('Unresolved editor command journal is full; inspect existing outcomes.');
      this.#records.splice(index, 1);
    }
    const id = randomUUID();
    this.#records.push({ version: 1, id, scope: { ...scope }, task, kind, ...(kind === 'prepare' ? { change: id } : {}), ...target, phase: 'pending' });
    await this.#persist();
    return id;
  }
  async settle(id: string, phase: 'accepted' | 'unknown' | 'resolved', change?: string): Promise<void> {
    const record = this.#records.find(item => item.id === id);
    if (!record) throw new Error('Unknown editor command identity');
    record.phase = phase;
    if (change !== undefined) record.change = change;
    await this.#persist();
  }
  async reconcile(scope: Scope, call: Client['call']): Promise<void> {
    for (const record of this.records()) {
      if (record.phase === 'accepted' || record.phase === 'resolved' || record.scope.workspace !== scope.workspace || record.scope.session !== scope.session) continue;
      try {
        const reply = await call('command/read', { scope, command_id: record.id });
        const receipt = reply.value;
        if (reply.kind === 'acceptance' && receipt.outcome === 'accepted' && receipt.command_id === record.id && receipt.scope.workspace === scope.workspace
          && receipt.scope.session === scope.session && receipt.task === record.task) await this.settle(record.id, 'accepted');
      } catch { /* Absent, pruned or unavailable evidence is not permission to replay. */ }
    }
  }
  /** Call only after an authorized change read proves every file has a receipt. */
  async resolveChange(scope: Scope, change: string): Promise<void> {
    for (const record of this.#records) if (record.change === change && record.scope.workspace === scope.workspace && record.scope.session === scope.session) record.phase = 'resolved';
    await this.#persist();
  }
}
