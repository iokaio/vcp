// SPDX-License-Identifier: Apache-2.0
import type { Client, Id, Scope } from '@vcp/sdk';

/** Parent/root identity stays attached to each child's independently read state. */
export async function inspectChildProgress(client: Client, scope: Scope, child: Id) {
  return client.call('task/read', { scope, task: child });
}
