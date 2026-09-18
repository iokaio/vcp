// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { parseArgs, validateBuild, validateQuery, expectedRejection } = require('../support/local-memory.cjs');
const corpus = { documents: [
  { id: 'current', workspace: 'a', current: true, supersedes: 'old' }, { id: 'old', workspace: 'a', current: false }, { id: 'foreign', workspace: 'b', current: true }
], queries: [{ id: 'query', workspace: 'a', lexical_required: ['current'], semantic_required: ['current'] }] };
const query = () => ({ status: 'pass', phase: 'query', governance: { backend: 'munarium-store-mem', durable: false, workspaces: [{ workspace: 'a', recorded: 2, current_ids: ['current'], historical_checks: 1, findings: 0 }, { workspace: 'b', recorded: 1, current_ids: ['foreign'], historical_checks: 0, findings: 0 }] }, queries: [{ case: 'query', workspace: 'a', lexical: ['current'], semantic: ['current'], ann_recall_at_3: 1, reopen_ms: 1, inference_us: 1, query_us: 1 }] });
test('local-memory invocation rejects duplicate, missing and unknown arguments', () => {
  const valid = ['--binary', 'native.exe', '--assets', 'assets', '--output-root', 'results'];
  assert.equal(parseArgs(valid)['--binary'], 'native.exe');
  for (const args of [[], valid.slice(0, -1), [...valid, '--binary', 'other.exe'], [...valid, '--unknown', 'x']]) assert.throws(() => parseArgs(args));
});
test('build evidence binds corpus, model and both real search engines', () => {
  const good = { status: 'pass', phase: 'build', governance: query().governance, documents: 3, corpus_sha256: 'a'.repeat(64), model_spec_sha256: 'b'.repeat(64), dimensions: 384, metric: 'cosine', vector_engine: 'diskann', lexical_engine: 'tantivy', receipt_sha256: 'c'.repeat(64) };
  assert.equal(validateBuild(good, corpus, 'a'.repeat(64), 'b'.repeat(64)), 'c'.repeat(64));
  for (const mutation of [{ documents: 2 }, { dimensions: 256 }, { vector_engine: 'flat' }, { lexical_engine: 'mock' }, { corpus_sha256: 'd'.repeat(64) }, { model_spec_sha256: 'e'.repeat(64) }]) {
    assert.throws(() => validateBuild({ ...good, ...mutation }, corpus, 'a'.repeat(64), 'b'.repeat(64)), /Incomplete/);
  }
});
test('query observer rejects missing relevance, stale/foreign results and incomplete recall', () => {
  assert.equal(validateQuery(query(), corpus).length, 1);
  for (const mutation of [{ lexical: [] }, { semantic: ['old'] }, { semantic: ['foreign'] }, { semantic: ['unknown'] }, { lexical: ['current', 'current'] }, { ann_recall_at_3: 0.99 }, { query_us: NaN }, { workspace: 'b' }]) {
    const result = query(); Object.assign(result.queries[0], mutation); assert.throws(() => validateQuery(result, corpus));
  }
  const missing = query(); missing.queries = []; assert.throws(() => validateQuery(missing, corpus));
  const duplicate = query(); duplicate.queries.push(duplicate.queries[0]); assert.throws(() => validateQuery(duplicate, corpus));
});
test('expected rejection cannot pass on timeout, capture failure or an unrelated error', () => {
  const good = { status: 'fail', exit_code: 1, reason: 'child_failed', capture_complete: true };
  assert.doesNotThrow(() => expectedRejection(good, 'integrity: changed bytes', 'integrity:'));
  for (const change of [{ exit_code: 0 }, { reason: 'timeout' }, { reason: 'launch_failed' }, { capture_complete: false }]) {
    assert.throws(() => expectedRejection({ ...good, ...change }, 'integrity:', 'integrity:'));
  }
  assert.throws(() => expectedRejection(good, 'missing model', 'integrity:'));
});

test('query evidence requires scoped governance replay and retained history', () => {
  for (const report of [undefined, { backend: 'fixture' }, { ...query().governance, durable: true }, { ...query().governance, workspaces: [] }]) {
    const result = query(); result.governance = report; assert.throws(() => validateQuery(result, corpus));
  }
  for (const mutation of [{ recorded: 1 }, { current_ids: ['old'] }, { current_ids: ['foreign'] }, { historical_checks: 0 }, { findings: -1 }, { workspace: 'b' }]) {
    const result = query(); Object.assign(result.governance.workspaces[0], mutation); assert.throws(() => validateQuery(result, corpus));
  }
});
