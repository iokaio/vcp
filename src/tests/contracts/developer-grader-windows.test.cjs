// SPDX-License-Identifier: Apache-2.0
'use strict';
// Actual Windows AppContainer grading of trusted doubles: one passing reference and
// one regression for every wrapper kind. Run explicitly on native Windows.
const test = require('node:test'), assert = require('node:assert/strict');
const crypto = require('node:crypto'), fs = require('node:fs'), path = require('node:path');
const { grade, appContainerExecutor } = require('../../../scripts/evals/developer-grader.cjs');
const { workspace, reference } = require('../support/developer-doubles.cjs');
const wrap = (name, body) => `\n{ const original = exports.${name}; exports.${name} = ${body}; }`;
test('qualified AppContainer grading of every wrapper kind', { skip: process.platform !== 'win32', timeout: 900000 }, async () => {
  const nodeSha256 = crypto.createHash('sha256').update(fs.readFileSync(process.execPath)).digest('hex');
  const executor = appContainerExecutor({ node: process.execPath, nodeSha256 });
  const cases = [
    ['MCP-near-miss-rest-v3', wrap('validate', "b => { if (typeof b?.name === 'string') b.name = b.name.trim(); return original(b); }")],
    ['MCP-boundary-pages-v3', wrap('handle', "(m, s) => { if (m.method === 'notifications/cancelled') s.pending.clear(); return original(m, s); }")],
    ['LLM-normal-stream-v3', '\nexports.limits.bytes = Infinity;'],
    ['MCP-normal-tools-v3', wrap('handle', "async (...a) => { console.log('debug'); return original(...a); }")],
    ['LLM-normal-request-v3', wrap('summarize', 'async (input, t) => { try { return await original(input, t); } catch (e) { if (e instanceof TypeError && typeof input === "string" && input && Buffer.byteLength(input) <= 1000) return original(input, t); throw e; } }')],
    ['LLM-boundary-partial-v3', '\nexports.limits.midstream = false;'],
  ];
  for (const [caseId, mutation] of cases) {
    const good = await grade(caseId, workspace(caseId), executor);
    assert.equal(good.functional_pass, true, `${caseId}: ${JSON.stringify(good.errors)}`);
    assert.equal(good.qualified_executor, true); assert.equal(good.executor, 'windows-appcontainer-node-fixture');
    const bad = await grade(caseId, workspace(caseId, reference[caseId], mutation), executor);
    assert.equal(bad.functional_pass, false, `Regression passed in the AppContainer: ${caseId}`);
  }
  const leaked = fs.readdirSync(path.join(process.env.LOCALAPPDATA, 'Packages')).filter(name => name.startsWith('iokaio.vcp.memory.'));
  assert.deepEqual(leaked, [], 'AppContainer profiles leaked');
});
