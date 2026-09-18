// SPDX-License-Identifier: Apache-2.0
'use strict';
function parseArgs(args) {
  const result = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i];
    if (!['--binary', '--assets', '--output-root'].includes(key) || Object.hasOwn(result, key) || !args[i + 1] || args[i + 1].startsWith('--')) throw Error('Invalid local-memory arguments');
    result[key] = args[i + 1];
  }
  if (!result['--binary'] || !result['--assets'] || !result['--output-root']) throw Error('Required: --binary, --assets and --output-root');
  return result;
}
function validateBuild(result, corpus, corpusHash, modelHash) {
  validateGovernance(result, corpus);
  if (result.status !== 'pass' || result.phase !== 'build' || result.documents !== corpus.documents.length ||
      result.corpus_sha256 !== corpusHash || result.model_spec_sha256 !== modelHash || result.dimensions !== 384 ||
      result.metric !== 'cosine' || result.vector_engine !== 'diskann' || result.lexical_engine !== 'tantivy' ||
      !/^[a-f0-9]{64}$/.test(result.receipt_sha256)) throw Error('Incomplete local-memory build result');
  return result.receipt_sha256;
}
function validateGovernance(result, corpus) {
  const report = result.governance;
  const workspaces = [...new Set(corpus.documents.map(d => d.workspace))].sort();
  if (report?.backend !== 'munarium-store-mem' || report.durable !== false || !Array.isArray(report.workspaces) ||
      report.workspaces.length !== workspaces.length) throw Error('Missing or misleading governance evidence');
  const seen = new Set();
  for (const row of report.workspaces) {
    if (!workspaces.includes(row.workspace) || seen.has(row.workspace)) throw Error('Unexpected governance workspace');
    seen.add(row.workspace);
    const documents = corpus.documents.filter(d => d.workspace === row.workspace);
    const expected = documents.filter(d => d.current).map(d => d.id).sort();
    if (row.recorded !== documents.length || row.historical_checks !== documents.filter(d => d.supersedes).length ||
        !Number.isSafeInteger(row.findings) || row.findings < 0 ||
        JSON.stringify(row.current_ids) !== JSON.stringify(expected)) throw Error('Incomplete governance history or visibility');
  }
}
function validateQuery(result, corpus) {
  validateGovernance(result, corpus);
  if (result.status !== 'pass' || result.phase !== 'query' || !Array.isArray(result.queries) || result.queries.length !== corpus.queries.length) throw Error('Incomplete local-memory query result');
  const seen = new Set();
  for (const row of result.queries) {
    const expected = corpus.queries.find(query => query.id === row.case);
    if (!expected || seen.has(row.case) || row.workspace !== expected.workspace || row.ann_recall_at_3 !== 1) throw Error('Unexpected query or inadequate recall');
    seen.add(row.case);
    for (const leg of ['lexical', 'semantic']) {
      const ids = row[leg];
      if (!Array.isArray(ids) || ids.length > 3 || new Set(ids).size !== ids.length ||
          ids.some(id => !corpus.documents.some(document => document.id === id && document.workspace === expected.workspace && document.current)) ||
          !expected[leg + '_required'].every(id => ids.includes(id))) throw Error('Wrong relevance, workspace or current-version result');
    }
    for (const field of ['reopen_ms', 'inference_us', 'query_us']) if (!Number.isFinite(row[field]) || row[field] < 0) throw Error('Missing query measurements');
  }
  return result.queries.map(({ case: id, workspace, lexical, semantic, ann_recall_at_3 }) => ({ id, workspace, lexical, semantic, ann_recall_at_3 }));
}
function expectedRejection(attempt, stderr, message, code = 1) {
  if (attempt.status !== 'fail' || attempt.exit_code !== code || attempt.reason !== 'child_failed' ||
      attempt.capture_complete !== true || !stderr.includes(message)) throw Error('Rejection case did not observe its intended failure');
}
module.exports = { parseArgs, validateBuild, validateQuery, expectedRejection };
