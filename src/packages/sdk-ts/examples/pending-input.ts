// SPDX-License-Identifier: Apache-2.0
import type { Client, Params } from '@vcp/sdk';

export async function inspectPendingInput(client: Client, request: Params<'task/read'>) {
  const task = await client.call('task/read', request);
  return task.value.pending_inputs;
}

/** Supply a deliberate answer with every source revision from the current input. */
export async function answerApproval(client: Client, answer: Params<'approval/respond'>) {
  return client.call('approval/respond', answer);
}
