// SPDX-License-Identifier: Apache-2.0
'use strict';
// Only called after exact retirement barriers exist. Never invokes a native CLI.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { createRequire } = require('node:module');
async function main() {
  const [oldRoot, manifest, destination, ...rows] = process.argv.slice(2);
  assert(path.isAbsolute(oldRoot) && path.isAbsolute(manifest) && path.isAbsolute(destination)); assert.equal(rows.length, 12);
  assert.equal(process.env.OPENROUTER_API_KEY, undefined); assert.equal(fs.existsSync(destination), false);
  const old = createRequire(path.join(oldRoot, 'scripts/evals/cs3-comparison.cjs')), core = old('./cs3-comparison.cjs'), doc = old('./cs3-document-remediation.cjs');
  const skills = ['frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing']; let calls = 0;
  for (let index = 0; index < 4; index++) {
    const [file, hash, skill] = rows.slice(index * 3, index * 3 + 3);
    assert.equal(skill, skills[index]); const plan = JSON.parse(fs.readFileSync(file));
    assert(fs.existsSync(path.join(plan.control_directory, 'halt.json')));
    await assert.rejects(core.run(file, hash, skill, () => { calls++; throw Error('Native transport is forbidden in retirement proof'); }), { message: 'Unknown skill or terminal halted envelope' });
  }
  assert(fs.existsSync(doc.claimFile()));
  assert.throws(() => doc.prepare(manifest, destination, true), { message: 'New private one-shot DOC campaign required' });
  assert.equal(calls, 0); assert.equal(fs.existsSync(destination), false);
  return { schema: 'cs3-friendli-retirement-denial/1', runtime_groups: skills, document_preparation_denied: true, transport_calls: 0, model_calls: 0 };
}
if (require.main === module) main().then(value => process.stdout.write(JSON.stringify(value) + '\n')).catch(error => { process.stderr.write(error.message + '\n'); process.exitCode = 1; });
