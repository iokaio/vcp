// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

test('local native rebuild and deployment preserve version, installation and recovery boundaries', {
  skip: process.platform !== 'win32', timeout: 90000,
}, () => {
  const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File',
    path.resolve(__dirname, '../../../scripts/installer/local-deploy.test.ps1')], {
    encoding: 'utf8', timeout: 85000, maxBuffer: 2 * 1024 * 1024, windowsHide: true,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /PASS \d+ local native deployment assertions/);
});
