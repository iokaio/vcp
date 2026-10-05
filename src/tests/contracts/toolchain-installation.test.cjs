// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const path = require('node:path');

test('verified toolchain installer preserves inputs and rejects unsafe archives', { timeout: 180000 }, () => {
  const script = path.resolve(__dirname, '../skills/ToolchainInstallation.Tests.ps1');
  const result = spawnSync(process.env.VCP_TEST_PWSH || 'pwsh',
    ['-NoLogo', '-NoProfile', '-NonInteractive', '-File', script],
    { encoding: 'utf8', timeout: 170000, maxBuffer: 2 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.signal, null);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /passed/i, 'the real PowerShell tests must report success');
});
