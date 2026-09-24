// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import type { ObserverReconnectReference } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import type { ConnectionSelection } from './trust.js';

/** Discovery only. Native peer authentication is required on every reuse. */
export interface Recovery {
  readonly folderUri: string;
  readonly profile: string;
  readonly reference: ObserverReconnectReference;
}
export function profile(selection: Pick<ConnectionSelection, 'executable' | 'dataPath'>): string {
  return createHash('sha256').update(JSON.stringify([selection.executable, selection.dataPath ?? null])).digest('hex');
}
export function recovery(value: unknown): Recovery | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const row = value as Record<string, unknown>;
  if (Object.keys(row).length !== 3 || typeof row.folderUri !== 'string' || row.folderUri.length > 32768 || typeof row.profile !== 'string' || !/^[a-f0-9]{64}$/.test(row.profile) || !row.reference || typeof row.reference !== 'object') return undefined;
  const reference = row.reference as Record<string, unknown>;
  if (Object.keys(reference).sort().join(',') !== 'endpoint,scope,server' || typeof reference.endpoint !== 'string' || reference.endpoint.length > 256 || !reference.server || typeof reference.server !== 'object' || !reference.scope || typeof reference.scope !== 'object') return undefined;
  const scope = reference.scope as Record<string, unknown>;
  if (Object.keys(scope).sort().join(',') !== 'session,workspace' || typeof scope.workspace !== 'string' || typeof scope.session !== 'string' || !/^[A-Za-z0-9_-]{1,96}$/.test(scope.workspace) || !/^[A-Za-z0-9_-]{1,96}$/.test(scope.session)) return undefined;
  // The SDK validates the complete reference before spawning its trusted helper.
  return value as Recovery;
}
