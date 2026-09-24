// SPDX-License-Identifier: Apache-2.0
import type { Client, Id, WorkspaceView } from '@vcp/sdk';

/** Explicit user create under an already acquired controller; never auto-retries. */
export async function createEncryptedBackup(client: Client, workspace: WorkspaceView, commandId: Id) {
  const status = await client.call('backup/status', { scope: client.scope });
  if (status.value.capability.state !== 'loaded' || workspace.binding_revision == null) {
    throw new Error('Encrypted publisher unavailable; select a native host profile explicitly');
  }
  return client.call('backup/create', {
    scope: client.scope,
    mutation: { command_id: commandId, expected_revision: workspace.revision, steering_revision: '0' },
    expected_binding_revision: workspace.binding_revision,
    capability: status.value.capability.reference,
    expected_capability_generation: status.value.capability.generation,
  });
}
