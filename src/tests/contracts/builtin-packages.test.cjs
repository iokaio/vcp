// SPDX-License-Identifier: Apache-2.0
'use strict';
// Shipped package contracts beyond hashes: validator parity, context budgets
// and baseline body structure/safety (SH-06).
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { rehashAssets } = require('../../../scripts/skills/builtin-assets.cjs');
const { validateSkill } = require('../../skills/builtin/skill-authoring/scripts/validate.cjs');
const builtin = path.resolve(__dirname, '../../skills/builtin');
const catalog = JSON.parse(fs.readFileSync(path.join(builtin, 'catalog.json'), 'utf8'));
const descriptor = id => JSON.parse(fs.readFileSync(path.join(builtin, id, 'skill.json'), 'utf8'));
const baseline = catalog.skills.map(entry => entry.id).filter(id => {
  const d = descriptor(id);
  return d.source === 'vcp-original' && d.resources.length === 0;
});

test('catalog, descriptor and coverage digests are current', () => {
  assert.deepEqual(rehashAssets(builtin, { check: true }), { changed: [], warnings: [] });
});

test('every shipped package passes the authoring validator without warnings', () => {
  for (const entry of catalog.skills) {
    const result = validateSkill(path.join(builtin, entry.id));
    assert.deepEqual(result.warnings, [], entry.id);
  }
});

// Context bytes are sent on every activation; references that are only needed
// on request belong in a non-context role.
const CONTEXT_BUDGET = 24 * 1024;
const CONTEXT_ALLOWANCE = { 'llm-integration': 32 * 1024 }; // until SH-11 moves provider references
test('activation context stays within the per-package budget', () => {
  for (const entry of catalog.skills) {
    const d = descriptor(entry.id);
    const bytes = [d.body, ...d.resources.filter(r => (r.use ?? 'context') === 'context')]
      .reduce((total, content) => total + fs.statSync(path.join(builtin, entry.id, content.path)).size, 0);
    const budget = CONTEXT_ALLOWANCE[entry.id] ?? CONTEXT_BUDGET;
    assert.ok(bytes <= budget, `${entry.id}: ${bytes} context bytes exceed ${budget}`);
  }
});

// Quoted VCP commands must name surfaces that exist in the CLI source: terminal
// commands as "/name" literals (plus a literal subcommand word) and `vcp` CLI
// subcommands as clap enum variants. Placeholders and flags are not checked.
function cliSources(directory = path.resolve(__dirname, '../../crates/vcp-cli/src')) {
  return fs.readdirSync(directory, { withFileTypes: true }).map(entry => {
    const full = path.join(directory, entry.name);
    return entry.isDirectory() ? cliSources(full) : entry.name.endsWith('.rs') ? fs.readFileSync(full, 'utf8') : '';
  }).join('\n');
}
test('quoted VCP commands in skill bodies exist in the CLI', () => {
  const source = cliSources();
  const pascal = word => word.split('-').map(part => part[0].toUpperCase() + part.slice(1)).join('');
  for (const entry of catalog.skills) {
    const text = fs.readFileSync(path.join(builtin, entry.id, descriptor(entry.id).body.path), 'utf8');
    for (const span of text.match(/`(\/[a-z]|vcp [a-z])[^`\n]*`/g) || []) {
      const words = span.slice(1, -1).split(/\s+/);
      if (words[0].startsWith('/')) {
        assert.ok(source.includes(`"${words[0]}"`), `${entry.id}: unknown terminal command ${span}`);
        if (/^[a-z][a-z-]*$/.test(words[1] || '')) {
          assert.ok(source.includes(`"${words[1]}"`), `${entry.id}: unknown subcommand in ${span}`);
        }
      } else {
        for (const word of words.slice(1, 3).filter(w => /^[a-z][a-z-]*$/.test(w))) {
          assert.ok(new RegExp(`\\b${pascal(word)}\\b`).test(source), `${entry.id}: unknown vcp subcommand in ${span}`);
        }
      }
    }
  }
});

// A code span with arguments is a runnable suggestion and must carry the flag
// that prevents hidden downloads or effects; a bare mention describes behavior.
const unsafe = [
  [/^uv run \S/, /--frozen|--offline|--no-sync/, 'uv run without --frozen/--offline/--no-sync'],
  [/^flutter (test|analyze) \S/, /--no-pub/, 'flutter test/analyze without --no-pub'],
  [/^dotnet (build|test|format) \S/, /--no-restore/, 'dotnet build/test/format without --no-restore'],
  [/^kubectl apply \S/, /--dry-run=client/, 'kubectl apply without --dry-run=client'],
];
test('baseline bodies keep their structure and safe command forms', () => {
  assert.equal(baseline.length, 21);
  for (const id of baseline) {
    const text = fs.readFileSync(path.join(builtin, id, 'SKILL.md'), 'utf8');
    const lines = text.split('\n');
    assert.ok(Buffer.byteLength(text) <= 6 * 1024, `${id}: body over 6 KiB`);
    assert.ok(!text.includes('\r'), `${id}: CRLF`);
    assert.ok(lines[2].startsWith('Original VCP guidance.') && !/version \d/.test(lines[2]), `${id}: line 3`);
    assert.ok(lines.filter(Boolean).at(-1).startsWith('Authority:'), `${id}: final Authority line`);
    for (const span of text.match(/`[^`\n]+`/g) || []) {
      const command = span.slice(1, -1);
      for (const [pattern, required, message] of unsafe) {
        assert.ok(!pattern.test(command) || required.test(command), `${id}: ${message}: ${span}`);
      }
    }
  }
});
