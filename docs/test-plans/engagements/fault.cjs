// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { child, physical, sha, protectedFiles } = require('./prerequisites.cjs');
function inject(kind, workspace, declaration, evidence) {
  assert(['A', 'B'].includes(kind));
  const plan = JSON.parse(fs.readFileSync(physical(declaration), 'utf8'));
  assert.equal(plan.schema, 'vcp-engagement-declared-fault/1');
  assert.equal(plan.check, kind === 'A' ? 'unsupported-import-version' : 'stale-adjustment-version');
  assert(typeof plan.path === 'string' && !plan.path.includes('\\'));
  assert(kind === 'A' ? /^server\/.+\.ts$/.test(plan.path) : /^src\/Inventory\.Web\/.+\.cs$/.test(plan.path));
  assert(!protectedFiles[kind].includes(plan.path));
  for (const value of [plan.before, plan.after]) assert(typeof value === 'string' && value.length > 0 && Buffer.byteLength(value) <= 512);
  assert.notEqual(plan.before, plan.after);
  const file = child(workspace, plan.path), bytes = fs.readFileSync(file);
  assert(bytes.length <= 2 * 1024 * 1024);
  const text = bytes.toString('utf8'); assert(Buffer.from(text).equals(bytes), 'Fault source must be UTF-8');
  assert.equal(text.split(plan.before).length, 2, 'Declared guard must occur exactly once');
  physical(path.dirname(evidence)); fs.mkdirSync(evidence);
  fs.writeFileSync(path.join(evidence, 'original-source'), bytes, { flag: 'wx' });
  const changed = Buffer.from(text.replace(plan.before, plan.after));
  fs.writeFileSync(path.join(evidence, 'injected-source'), changed, { flag: 'wx' });
  const receipt = { schema: 'vcp-engagement-fault-receipt/1', ...plan, before_sha256: sha(bytes), after_sha256: sha(changed),
    provenance: 'Explicit independent harness injection; not attributed to VCP', acceptance: 'Requires passing negative oracle before injection, failing same oracle after, then passing after repair.' };
  fs.writeFileSync(path.join(evidence, 'receipt.json'), JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
  // Do not evaluate the declaration, or follow arbitrary executable/command data.
  fs.writeFileSync(file, changed);
  return receipt;
}
module.exports = { inject };
if (require.main === module) { try { inject(...process.argv.slice(2)); } catch (error) { console.error(error.message); process.exitCode = 1; } }
