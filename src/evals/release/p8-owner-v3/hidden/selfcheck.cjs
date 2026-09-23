'use strict'; // SPDX-License-Identifier: Apache-2.0
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const review = path.join(root, 'u02/workspace');
const capacity = require(path.join(review, 'src/capacity.cjs'));
const expiry = require(path.join(review, 'src/expiry.cjs'));
const receipt = require(path.join(review, 'src/receipt.cjs'));
assert.equal(capacity.canReserve(3, 5, 2), false);
assert.equal(expiry.isActive(100, 100), true);
assert.equal(require(path.join(review, 'base/src/capacity.cjs')).canReserve(3, 5, 2), true);
assert.equal(require(path.join(review, 'base/src/expiry.cjs')).isActive(100, 100), false);
for (const id of [0, 1, 42, 9007199254740991]) assert.equal(receipt.receipt(id), require(path.join(review, 'base/src/receipt.cjs')).receipt(id));
const architecture = path.join(root, 'u01/workspace');
assert.equal(require(path.join(architecture, 'src/transport/handler.cjs')).getSummary().body.total, 18);
const domain = fs.readFileSync(path.join(architecture, 'src/domain/report.cjs'), 'utf8').split('\n');
assert.match(domain[1], /require\('\.\.\/transport\/status\.cjs'\)/);
const { evaluate } = require(path.join(root, 'u03/hidden/oracle.cjs'));
const reference = evaluate(path.join(root, 'u03/hidden/reference'));
const baseline = evaluate(path.join(root, 'u03/workspace'));
assert.equal(reference.pass, true);
assert.equal(baseline.pass, false);
assert.ok(baseline.checks.filter(c => !c.pass).length >= 10);
const visible = spawnSync(process.execPath, ['--test', 'test/page.test.cjs'], { cwd: path.join(root, 'u03/workspace'), encoding: 'utf8', windowsHide: true });
assert.equal(visible.status, 0, visible.stdout + visible.stderr);
console.log(JSON.stringify({ schema: 'p805-fixture-selfcheck/1', model_calls: 0, review_seeded_defects_reproduced: 2,
  benign_review_change_verified: true, architecture_violation_verified: true, visible_baseline_tests: '3/3',
  reference, incomplete_generation_baseline: baseline, caveat: 'Fixture validation only; no VCP task execution or owner acceptance.' }, null, 2));
