// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const root = path.resolve(__dirname, '../..');
const original = path.join(root, 'src/evals/skills/cs3-comparison/candidates/document-authoring');
const prospective = path.join(root, 'src/evals/skills/cs3-document-remediation/candidates/document-authoring');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const oldBodyHash = 'c720315975e2a83f475177a9c4ef6e31e2583978fc020c58e22af8de93aaea50';
const oldDescriptorHash = '1346be306da353bdbb2c5676b9999301c05f836f15668c3073b7248b3c86c722';
const bodyHash = '4e1359504193ce160c60a767da63b031052b119a7eaad676f169fd3116cd72fb';
const oldEnding = 'When returning file content, reread the final artifact and reproduce its exact\n'
  + 'stored bytes, including its final newline state. Compare the returned byte length\n'
  + 'or hash with the reread file when the task or harness reports file content. Do not\n'
  + 'reconstruct the response from an earlier draft or silently normalize line endings.\n';
const newEnding = 'When returning content for a file that already exists or that you actually wrote,\n'
  + 'reread the final artifact and reproduce its exact stored bytes, including its final\n'
  + 'newline state. Compare the returned byte length or hash with the reread file when\n'
  + 'the task or harness reports file content. Do not reconstruct the response from an\n'
  + 'earlier draft or silently normalize line endings.\n\n'
  + 'When the task instead requests newly authored content as a files map under no-write\n'
  + 'authority, return the requested exact content directly in that map. A stored output\n'
  + 'file or directory is not a prerequisite for this response. Preserve the requested\n'
  + 'line endings and final newline state; do not invent a file write, claim a stored\n'
  + 'artifact reread, or request write authority just to satisfy the return format.\n';
test('frozen DOC 1.0.4 bytes remain unchanged beside the separate prospective candidate', () => {
  assert.equal(sha(fs.readFileSync(path.join(original, 'SKILL.md'))), oldBodyHash);
  assert.equal(sha(fs.readFileSync(path.join(original, 'skill.json'))), oldDescriptorHash);
  assert.notEqual(original, prospective);
});
test('DOC 1.0.5 descriptor changes only version and exact body digest', () => {
  const old = JSON.parse(fs.readFileSync(path.join(original, 'skill.json'))), bytes = fs.readFileSync(path.join(prospective, 'skill.json'));
  const expected = { ...old, version: '1.0.5', body: { ...old.body, sha256: bodyHash } };
  assert.equal(bytes.toString('utf8'), JSON.stringify(expected, null, 2) + '\n');
  assert.equal(sha(fs.readFileSync(path.join(prospective, expected.body.path))), bodyHash);
  assert.deepEqual(fs.readdirSync(prospective).sort(), ['SKILL.md', 'skill.json']);
});
test('guidance delta is exclusively the version and conditional stored-artifact reread rule', () => {
  const old = fs.readFileSync(path.join(original, 'SKILL.md'), 'utf8'), actual = fs.readFileSync(path.join(prospective, 'SKILL.md'));
  assert(old.endsWith(oldEnding)); assert.equal(old.split(oldEnding).length, 2);
  const expected = old.replace('Original VCP guidance, version 1.0.4.', 'Original VCP guidance, version 1.0.5.').replace(oldEnding, newEnding);
  assert(actual.equals(Buffer.from(expected, 'utf8')));
  assert.equal(actual.includes(13), false); assert.equal(actual.at(-1), 10);
});
test('both existing-file fidelity and no-write files-map delivery remain explicit', () => {
  const body = fs.readFileSync(path.join(prospective, 'SKILL.md'), 'utf8');
  assert(body.includes('file that already exists or that you actually wrote'));
  assert(body.includes('exact stored bytes, including its final\nnewline state'));
  assert(body.includes('byte length or hash with the reread file'));
  assert(body.includes('files map under no-write\nauthority, return the requested exact content directly in that map'));
  assert(body.includes('stored output\nfile or directory is not a prerequisite'));
  assert(body.includes('do not invent a file write, claim a stored\nartifact reread'));
});
