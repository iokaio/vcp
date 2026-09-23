// SPDX-License-Identifier: Apache-2.0
import type { Client, Params } from '@vcp/sdk';

/** A gap is returned to the caller, which decides whether to obtain a fresh cut. */
export async function snapshotOrGap(client: Client, request: Params<'session/snapshot'>) {
  return client.snapshot(request);
}

/** Close the subscription even when only the first batch is wanted. */
export async function firstEventBatch(client: Client, request: Params<'events/subscribe'>) {
  const stream = client.events(request);
  try {
    return await stream.next();
  } finally {
    await stream.return();
  }
}
