// SPDX-License-Identifier: Apache-2.0
import { spawn } from 'node:child_process';
import { isAbsolute, win32 } from 'node:path';
import type { Counter, Id, Trust } from '@vcp/protocol';
import { SdkError } from './errors.js';
import { validateWire } from './validation.js';

export type RebindOptions = { executable: string; workspace: string; workspaceId: Id; data?: string };
export type RebindResult = { workspace: Id; root: string; binding_revision: Counter; authority: Counter; workspace_revision: Counter; trust: Trust; rebound: boolean; history_preserved: true; tasks_resumed: false };
function absolute(value: string): void {
  if (typeof value !== 'string' || value.length > 32768 || value.includes('\0') || !(isAbsolute(value) || win32.isAbsolute(value))) throw new SdkError('INVALID_ARGUMENT', 'explicit absolute local path required');
}
/** Decode only the established CLI result projection; private descriptor paths never escape. */
export function decodeRebindResult(output: string, workspace: Id): RebindResult {
  let envelope: Record<string, unknown>;
  try { envelope = JSON.parse(output); } catch { throw new SdkError('MALFORMED_PEER', 'invalid rebind result'); }
  if (!envelope || envelope.schema_version !== 1 || envelope.type !== 'result' || envelope.exit_code !== 0 || envelope.scope !== null) throw new SdkError('MALFORMED_PEER', 'invalid rebind outcome');
  validateWire('Id', envelope.correlation);
  const data = envelope.data as Record<string, unknown> | undefined;
  if (!data || data.workspace !== workspace || typeof data.root !== 'string' || data.root.length > 32768 || data.root.includes('\0') || typeof data.rebound !== 'boolean' || data.history_preserved !== true || data.tasks_resumed !== false) throw new SdkError('MALFORMED_PEER', 'invalid rebind projection');
  absolute(data.root);
  validateWire('Trust', data.trust);
  for (const key of ['binding_revision', 'authority', 'workspace_revision']) validateWire('Counter', data[key]);
  return { workspace, root: data.root, binding_revision: data.binding_revision as Counter, authority: data.authority as Counter, workspace_revision: data.workspace_revision as Counter, trust: data.trust as Trust, rebound: data.rebound, history_preserved: true, tasks_resumed: false };
}
/** One deliberate offline reconciliation. Failure never triggers launch, takeover or retry. */
export function rebindLocal(options: RebindOptions): Promise<RebindResult> {
  absolute(options.executable); absolute(options.workspace); validateWire('Id', options.workspaceId);
  if (options.data !== undefined) absolute(options.data);
  const args = ['--format', 'jsonl', '--non-interactive', '--workspace', options.workspace, ...(options.data === undefined ? [] : ['--data-dir', options.data]), 'workspace', 'rebind', options.workspaceId];
  return new Promise((resolve, reject) => {
    const child = spawn(options.executable, args, { shell: false, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    const chunks: Buffer[] = []; let bytes = 0; let failed = false;
    const fail = (code: string) => { if (failed) return; failed = true; clearTimeout(timer); child.kill(); reject(new SdkError(code, 'rebind outcome unavailable; inspect the selected workspace before retrying')); };
    const timer = setTimeout(() => fail('OUTCOME_UNKNOWN'), 30_000);
    child.once('error', () => fail('TRANSPORT_INTERRUPTED'));
    child.stdout.on('error', () => fail('TRANSPORT_INTERRUPTED'));
    child.stderr.on('error', () => fail('TRANSPORT_INTERRUPTED'));
    child.stderr.on('data', (chunk: Buffer) => { bytes += chunk.length; if (bytes > 64 * 1024) fail('OUTCOME_UNKNOWN'); }); // Bound and discard native diagnostics.
    child.stdout.on('data', (chunk: Buffer) => { bytes += chunk.length; if (bytes > 64 * 1024) fail('MALFORMED_PEER'); else if (!failed) chunks.push(chunk); });
    child.once('close', code => {
      clearTimeout(timer); if (failed) return;
      if (code !== 0) { fail('REBIND_UNAVAILABLE'); return; }
      try { resolve(decodeRebindResult(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks)), options.workspaceId)); }
      catch { reject(new SdkError('MALFORMED_PEER', 'invalid rebind result; inspect the selected workspace')); }
    });
  });
}
