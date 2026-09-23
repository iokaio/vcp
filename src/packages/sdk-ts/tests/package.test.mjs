// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { Client, launchLocal, attachLocal, newCommandId, SdkError, RpcFailure } from '@vcp/sdk';

const require = createRequire(import.meta.url);
const root = fileURLToPath(new URL('../', import.meta.url));

test('ESM package exports resolve to emitted runtime and declarations', () => {
  for (const entry of [Client, launchLocal, attachLocal, newCommandId, SdkError, RpcFailure]) {
    assert.equal(typeof entry, 'function');
  }
  const manifest = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(manifest.exports['.'].import, './dist/index.js');
  assert.equal(manifest.exports['.'].types, './dist/index.d.ts');
  assert.ok(readFileSync(new URL('../dist/index.d.ts', import.meta.url), 'utf8').includes('newCommandId'));
  const first = newCommandId();
  assert.match(first, /^[0-9a-f-]{36}$/);
  assert.notEqual(newCommandId(), first);
});

test('a package consumer retains method-specific results, exact counters and mutation identity', () => {
  const result = spawnSync(process.execPath, [
    require.resolve('typescript/bin/tsc'), '--strict', '--noEmit', '--skipLibCheck', 'false',
    '--module', 'NodeNext', '--target', 'ES2022', 'tests/consumer.ts',
  ], { cwd: root, encoding: 'utf8', timeout: 30_000, windowsHide: true });
  assert.equal(result.status, 0, result.stdout + result.stderr);
});
