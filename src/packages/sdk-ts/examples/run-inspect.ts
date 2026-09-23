// SPDX-License-Identifier: Apache-2.0
import type { Client, Params } from '@vcp/sdk';

/** The caller acquires the controller and supplies the durable identity/budget. */
export async function startAndInspect(client: Client, request: Params<'turn/start'>) {
  const acceptance = await client.call('turn/start', request);
  const task = await client.call('task/read', { scope: request.scope, task: request.task });
  // Acceptance admits work; this task snapshot may still be running or waiting.
  return { acceptance, task };
}
