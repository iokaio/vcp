// SPDX-License-Identifier: Apache-2.0
import type { Client, Params } from '@vcp/sdk';

export async function pauseAndInspect(client: Client, request: Params<'turn/pause'>) {
  const acceptance = await client.call('turn/pause', request);
  const task = await client.call('task/read', { scope: request.scope, task: request.task });
  return { acceptance, task };
}

/** Reconnection alone never invokes this explicit, revision-bound mutation. */
export async function resumeAndInspect(client: Client, request: Params<'session/resume'>) {
  const acceptance = await client.call('session/resume', request);
  const task = await client.call('task/read', { scope: request.scope, task: request.task });
  return { acceptance, task };
}
