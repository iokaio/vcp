// SPDX-License-Identifier: Apache-2.0
import { newCommandId, type Client, type Reply, type Params } from '@vcp/sdk';

declare const client: Client;
const scope = { workspace: 'workspace', session: 'session' };
const request: Params<'task/read'> = { scope, task: 'task' };
const task: Promise<Reply<'task/read'>> = client.call('task/read', request);
void task;

async function snapshot() {
  const reply = await client.snapshot({ scope, limit: 16 });
  if (reply.kind === 'gap') return reply.value;
  return reply.value.tasks;
}
void snapshot;

client.call('turn/pause', {
  scope, task: 'task', turn: 'turn', reason: 'deliberate connected pause',
  mutation: { command_id: newCommandId(), expected_revision: '9007199254740993', steering_revision: '0' },
});

// @ts-expect-error A mutation must carry the caller's durable identity and revisions.
client.call('turn/pause', { scope, task: 'task' });
// @ts-expect-error Exact counters are decimal strings, never JavaScript numbers.
client.call('artifact/read', { scope, task: 'task', artifact: 'artifact', offset: 9007199254740993, length: 128 });
// @ts-expect-error A task read cannot be misrepresented as durable acceptance.
const wrong: Promise<Reply<'command/read'>> = client.call('task/read', request);
void wrong;
