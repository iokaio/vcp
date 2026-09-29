// SPDX-License-Identifier: Apache-2.0
'use strict';
// Static authoring-contract checks, not model behavior or native package acceptance.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const repository = path.resolve(__dirname, '../..');
const previous = path.join(repository, 'src/evals/skills/cs3-comparison/candidates/skill-authoring');
const candidate = path.join(repository, 'src/evals/skills/cs3-skill-remediation/candidates/skill-authoring');
const read = (root, name) => fs.readFileSync(path.join(root, name));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const descriptor = JSON.parse(read(candidate, 'skill.json'));
const body = read(candidate, 'SKILL.md').toString('utf8');
const prose = body.replace(/\s+/g, ' ');

test('1.0.3 is a separate exact-content revision with the unchanged VCP resource and authority', () => {
  const old = JSON.parse(read(previous, 'skill.json'));
  assert.equal(old.version, '1.0.2');
  assert.equal(sha(read(previous, 'SKILL.md')), '535d2016f0a2909c7fa162d0e3e6abaa9748b10a21ed892c1d5e0ff22eb443bc');
  assert.equal(descriptor.version, '1.0.3');
  assert.deepEqual({ ...descriptor, version: old.version, body: old.body }, old);
  assert.deepEqual(read(candidate, descriptor.resources[0].path), read(previous, old.resources[0].path));
  for (const part of [descriptor.body, ...descriptor.resources]) assert.equal(sha(read(candidate, part.path)), part.sha256);
  assert.deepEqual(fs.readdirSync(candidate).sort(), ['SKILL.md', 'references', 'skill.json']);
  assert.deepEqual(fs.readdirSync(path.join(candidate, 'references')), ['package-format.md']);
  assert.match(prose, /A package describes work; it cannot grant tools, network access, credentials, budget or permission to publish\./);
});

test('existing selection, evaluation, preservation and promotion guidance remains intact', () => {
  const prefix = read(previous, 'SKILL.md').toString('utf8').split('Before native verification, reread')[0];
  assert.ok(body.startsWith(prefix.replace('version 1.0.2.', 'version 1.0.3.')));
  assert.match(prose, /an unqualified candidate is not an accepted default/);
  assert.match(prose, /Do not spend or replay a failed campaign/);
});

test('return-only and applied-edit instructions distinguish real files from proposed bytes', () => {
  assert.match(prose, /proposed files do not exist in the workspace unless an authorized write actually created them/);
  assert.match(prose, /Do not read proposed paths to check an unwritten draft or create files merely to satisfy a readback procedure/);
  assert.match(prose, /Compute proposed content hashes only from the exact proposed bytes using available authorized tooling/);
  assert.match(prose, /never invent a digest or claim package integrity passed/);
  assert.match(prose, /For authorized applied edits, reread every created or changed package file in full/);
  assert.match(prose, /cannot supply a fabricated read-artifact citation/);
});

test('finalization requires inspection before verification and refresh after additional effects', () => {
  assert.match(prose, /Finish all needed inspection, reads and other effects before the final verifier/);
  assert.match(prose, /even a read can make the recorded verification stale/);
  assert.match(prose, /If further authorized work is genuinely necessary, finish it and refresh verification before finalizing; do not reuse stale evidence/);
  assert.match(prose, /This is not permission to retry failed checks or replay model requests/);
  assert.match(prose, /missing or conflicting store record.*report verification as not run.*do not retry with guessed identifiers/);
  assert.match(prose, /Distinguish verification of the observed task from unperformed installation, activation or live package tests/);
});

test('format guidance is general across unrelated return schemas, without held-out task details', () => {
  assert.match(prose, /Follow the user's exact final artifact schema/);
  assert.match(prose, /no progress commentary, reasoning preface, extra fields, or Markdown fences unless the response contract requests them/);
  assert.match(prose, /Do not replace a constrained result with that general summary/);
  // These arbitrary schemas illustrate why the guidance must not dictate one envelope.
  // This checks package prose independence, not whether a model follows the guidance.
  const examples = [
    { draft_files: { 'release-review.md': '# Release review\n' }, limitations: ['No files written'] },
    { proposal: [{ destination: 'handoff-notes.md', contents: '# Handoff\n' }], observed_checks: [] },
  ];
  for (const example of examples) {
    for (const key of Object.keys(example)) assert.ok(!body.includes(`"${key}":`));
  }
  assert.doesNotMatch(body, /release-review\.md|handoff-notes\.md/);
  assert.doesNotMatch(body, /SKL-(?:runtime|cs3)-|glossary-package|forbidden_literals|oracle|600000|129576/);
});
