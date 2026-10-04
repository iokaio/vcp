// SPDX-License-Identifier: Apache-2.0
'use strict';
// Offline analysis of already-authorized bundles. No provider/network access.
const fs = require('node:fs');
const crypto = require('node:crypto');

function analyze(bundle) {
  if (bundle?.schema_version !== 1 || bundle.kind !== 'inspection_bundle' || !bundle.task?.scope?.task) throw Error('Expected a version 1 inspection bundle');
  const scope = bundle.task.scope;
  const gaps = [];
  const eventKinds = Object.create(null);
  const events = new Map();
  const edges = [];
  const recordObservations = Object.create(null);
  for (const page of bundle.history ?? []) {
    if (page.next_cursor) gaps.push({kind:'paged_history',cursor_omitted:true});
    for (const gap of page.gaps ?? []) gaps.push({kind:'history_visibility',reason:gap.reason ?? 'unspecified'});
    for (const row of page.rows ?? []) {
      const event = row.event?.event;
      if (!event?.id || event.task !== scope.task || event.workspace !== scope.workspace || event.session !== scope.session) throw Error('Event identity or bundle scope mismatch');
      if (events.has(event.id)) throw Error('Duplicate event identity');
      events.set(event.id, event);
      const reference = (collection,id) => { if (typeof collection === 'string' && typeof id === 'string') (recordObservations[`${collection}:${id}`] ??= []).push(event.id); };
      for (const fact of event.data?.facts ?? []) reference(fact.collection,fact.id);
      for (const collection of ['attempt','reservation','settlement','verification']) reference(collection,event.data?.[collection]?.id);
      eventKinds[event.kind] = (eventKinds[event.kind] ?? 0) + 1;
      if (event.causation) edges.push({cause:event.causation,event:event.id});
      if (row.content_truncated || row.event.redaction) gaps.push({kind:'event_content',event:event.id});
      for (const artifact of row.artifact_links ?? []) if (artifact.availability !== 'retained') gaps.push({kind:'artifact_visibility',artifact:artifact.id,availability:artifact.availability});
    }
  }
  // Intermediate page cursors are normal when the complete next page exists.
  const history = bundle.history ?? [];
  const tailIncomplete = history.length === 0 || !!history.at(-1).next_cursor;
  const filteredGaps = gaps.filter(gap => gap.kind !== 'paged_history');
  if (tailIncomplete) filteredGaps.push({kind:'history_tail_unavailable'});
  for (const edge of edges) if (!events.has(edge.cause)) filteredGaps.push({kind:'causal_parent_outside_projection',...edge});
  const records = name => {
    const pages = bundle.views?.[name] ?? [];
    if (!pages.length || pages.at(-1).next_cursor) filteredGaps.push({kind:'view_incomplete',view:name});
    for (const page of pages) for (const gap of page.gaps ?? []) filteredGaps.push({kind:'view_visibility',view:name,reason:gap.reason ?? 'unspecified'});
    return pages.flatMap(page => page.items ?? []);
  };
  const verification = records('verification').filter(row => row.collection === 'verification' && row.record).map(row => ({
    id:row.record.id,
    checks:(row.record.checks ?? []).map(check => ({requirement:check.specification,outcome:check.outcome,exit_code:check.exit_code,evidence:check.output})),
    outstanding_issues:row.record.outstanding_issues ?? [],unresolved_effects:row.record.unresolved_effects ?? [],
  }));
  const ledgers = records('costs').filter(row => row.collection === 'ledger');
  const accounting = ledgers.map(row => ({root:row.record?.scope?.task,
    settled:row.record?.settled ?? null,active:row.record?.active ?? null,unresolved:row.record?.unresolved ?? null,
    visibility:row.visibility ?? 'unknown'}));
  return {schema_version:1,kind:'execution_bundle_analysis',task:scope.task,source_watermark:bundle.source_watermark,
    facts:{task_state:bundle.task.state,event_count:events.size,event_kinds:eventKinds,causal_edges:edges,record_observations:recordObservations,verification,accounting,
      store_phases:bundle.store_diagnostics ?? null,lifecycle_phases:bundle.lifecycle_diagnostics ?? null},
    gaps:filteredGaps,
    relationship_semantics:'Record groups are explicit shared identities; only causal_edges assert recorded causation. Retained-with-omissions may describe intentional credential omission, not missing execution evidence.',
    assessment:{quality:'requires_independent_scenario_gates',causal_analysis:'requires_evidence_review',
      timing_comparison:'unqualified_without_matching_candidate_host_backend_workload_and_cache_conditions',
      accounting:'observations_only_unknown_charges_are_not_zero'},
    next_review:['Compare required outputs with independent scenario gates.','Trace failure, repair and verification through event and artifact identities.',
      'Inspect unavailable evidence and uncertain effects before claiming a cause.','State which change the evidence supports and what remains unknown.']};
}

function read(file) {
  const bytes = fs.readFileSync(file);
  if (bytes.length > 32 * 1024 * 1024) throw Error('Bundle input exceeds 32 MiB');
  const text = new TextDecoder('utf-8',{fatal:true}).decode(bytes);
  let bundle;
  try { bundle = JSON.parse(text); } catch {
    const rows = text.split(/\r?\n/).filter(line => line.trim()).map(line => JSON.parse(line));
    const matches = rows.filter(row => row.type === 'result' && row.exit_code === 0 && row.data?.kind === 'inspection_bundle');
    if (matches.length !== 1) throw Error('Expected exactly one successful JSONL inspection bundle');
    bundle = matches[0].data;
  }
  if (bundle.type === 'result' && bundle.exit_code === 0) bundle = bundle.data;
  return {source_sha256:crypto.createHash('sha256').update(bytes).digest('hex'),analysis:analyze(bundle)};
}
module.exports = {analyze,read};
if (require.main === module) {
  try {
    const [flag, output, ...inputs] = process.argv.slice(2);
    if (flag !== '--out' || !output || !inputs.length) throw Error('Usage: node analyze-execution-bundle.cjs --out <new-report.json> <bundle.json|jsonl> [...]');
    const report = {schema_version:1,created_at:new Date().toISOString(),runs:inputs.map(read)};
    fs.writeFileSync(output, `${JSON.stringify(report,null,2)}\n`, {flag:'wx'});
    process.stdout.write(`Analyzed ${report.runs.length} bundle(s); independent quality and causal review required.\n`);
  } catch (error) { process.stderr.write(`${error.message}\n`); process.exitCode = 1; }
}
