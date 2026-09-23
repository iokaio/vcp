// SPDX-License-Identifier: Apache-2.0
import type { Client, Params } from '@vcp/sdk';

/** Explicit retry demonstration: preserve every field, including command identity. */
export async function retryOriginalStart(client: Client, original: Params<'turn/start'>) {
  return client.call('turn/start', original);
}

/** Read retained acceptance after interruption before deciding to resubmit. */
export async function reconcileOriginal(client: Client, original: Params<'command/read'>) {
  return client.reconcile(original);
}
