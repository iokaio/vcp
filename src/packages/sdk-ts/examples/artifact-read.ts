// SPDX-License-Identifier: Apache-2.0
import type { Client, Params } from '@vcp/sdk';

export async function readArtifactRange(client: Client, request: Params<'artifact/read'>) {
  const result = await client.call('artifact/read', request);
  const range = result.value;
  const bytes = Buffer.from(range.content, range.encoding === 'base64' ? 'base64' : 'utf8');
  if (range.encoding === 'base64' && bytes.toString('base64') !== range.content) {
    throw new Error('Invalid retained artifact encoding');
  }
  // sha256 describes the full artifact. A partial range is not a full hash proof.
  return { range, bytes };
}
