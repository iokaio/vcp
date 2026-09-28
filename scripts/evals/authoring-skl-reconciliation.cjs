// SPDX-License-Identifier: Apache-2.0
'use strict';
// Read-only authentication of this one halted predecessor. No invocation path.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const localPrep = require('./authoring-prepare.cjs'), localPrior = require('./p6-live-runner.cjs');
const { read, plain, safeChild, within } = localPrior.boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const must = (ok, message) => { if (!ok) throw Error(message); };
const PIN = Object.freeze({
  envelope: '0bf5b618e5d26f1cb4781f775120e1b1d33fe63a3de1e5a3006df7858ba3b7eb',
  phase: '6ea66f8a114a9b93edcbdbd169cf0b08f9055fe30d1ded6233cf341589380dd8',
  result: '2779da152d154237410212ff3d05aa586920aa30300cabb05e30fe15784439ff',
  evidence: '9ca6a94a04419ed2e3b5138272cb3326df5e0ec0cee70c3ab76f9704bb309cbf',
  cli: '1dcd90293b07b8dabb15499f5f71dfda575d08ebff1eb4826bf10cc274f9bbf9',
  source: 'd5d3613d9a7ccb2e9e8f2725d992a3c1a81c475587a16429909d38252e9c19ef',
  receipt: '273f1ae2848452c3299d68a3cf6db8c625ccae634c12633fa132adcfa011fc52',
  costs: '959620b39884577fe35c8405673f75a140efbeadc1ad9c8cc7898341661b7fad',
  cost_inspection: '7424f27db9349030c6053d9dc35d066c5a4c38c5322f9ea791c01286391747e4',
  doc_gate: '2a96dd3a439b26dfa114a62ad354bc3a90128af59ad28cc15546c6a075fe5a1c',
  spec: '846c4956e4df0d91ab27c3fd3215e3fd9a56ec8bb11e55e8262a6e323acd5685'
});
const consumed = 'skill-authoring--normal--00--nearest';
const views = ['routing', 'outputs', 'context', 'tools', 'verification'];
const fixedRoutingGap = Object.freeze({ visibility: 'unavailable', reason: 'no automatic routing decision retained for this task; fixed provider or no admitted routed request', requested_model: 'attempt.quote.price.model', served_model: 'captured response bytes when observed; never inferred from requested model' });
function terminalCs2Relocated(reference, repository, modules) {
  const planBytes = read(reference.plan.file, 16 * 1024 * 1024), plan = JSON.parse(planBytes), planHash = sha(planBytes);
  must(planHash === reference.plan.sha256 && planHash === '2744de3f29883d01d2e1c661d1b2d823bc85b805a9cc98c3dc2092a22800c716' &&
    plan.schema === 'cs-2-developer-continuation/1' && path.resolve(reference.plan.file) === path.join(plan.directory, 'plan.json') &&
    equal(localPrep.identity(repository, plan.source.scope), plan.source), 'Relocated immutable CS-2 source or plan differs');
  must(!fs.existsSync(path.join(plan.directory, 'active-block.json')) && !fs.existsSync(path.join(plan.directory, 'halt.json')) && plan.runs.length === 46 && plan.limits.cap_micros === 92000000, 'Only complete terminal CS-2 continuation is supported');
  const developerPrep = require(path.join(repository, 'scripts/evals/developer-prepare.cjs'));
  const receipt = JSON.parse(read(plan.runtime.build_receipt, 8 * 1024 * 1024)), checkerSource = 'src/crates/vcp-cli/src/bin/vcp-developer-check.rs', fixture = 'src/evals/skills/developer/manifest.json';
  const expectedInputs = localPrep.identity(repository, developerPrep.checkerBuildScope).files.map(({ path, sha256 }) => ({ path, sha256 }));
  must(receipt.schema === 'cs2-developer-check-build/1' && receipt.exit_code === 0 && receipt.source_inputs_unchanged === true && receipt.source === checkerSource &&
    receipt.source_sha256 === sha(read(path.join(repository, checkerSource))) && receipt.fixture_manifest === fixture && receipt.fixture_manifest_sha256 === sha(read(path.join(repository, fixture))) &&
    receipt.executable === plan.runtime.source_checker && receipt.executable_sha256 === plan.runtime.checker_sha256 && receipt.executable_sha256 === sha(read(plan.runtime.source_checker, 268435456)) &&
    equal(receipt.source_scope, developerPrep.checkerBuildScope) && equal(receipt.source_inputs, expectedInputs) && receipt.builder_sha256 === sha(read(path.join(repository, 'scripts/evals/developer-check-build.ps1'))) &&
    sha(read(plan.runtime.build_receipt, 8 * 1024 * 1024)) === plan.runtime.build_receipt_sha256 && sha(read(plan.runtime.checker, 268435456)) === plan.runtime.checker_sha256 && sha(read(plan.runtime.cases_file)) === plan.runtime.cases_sha256,
  'Relocated CS-2 checker provenance or staged runtime differs');
  const rows = [], blockRefs = [], claims = [];
  for (const block of ['llm-integration', 'mcp-development', 'frontend-design']) {
    const resultBytes = read(path.join(plan.directory, `result-${block}.json`), 16 * 1024 * 1024), result = JSON.parse(resultBytes), planned = plan.runs.filter(row => row.block === block);
    must(result.schema === 'cs-2-developer-continuation-block/1' && result.plan_sha256 === planHash && result.block === block && !result.stopped && result.final_inputs_unchanged === true &&
      equal(result.runs.map(row => row.id), planned.map(row => row.id)), 'Complete unchanged CS-2 block required');
    must(equal(JSON.parse(read(path.join(plan.directory, 'claims', `block-${block}.json`))), { plan_sha256: planHash, directory: plan.directory, block }), 'CS-2 block claim differs');
    claims.push(`block-${block}.json`);
    let cost = 0, requests = 0;
    for (const [index, row] of planned.entries()) {
      const report = result.runs[index], base = path.join(plan.directory, row.id);
      must(['completed', 'failed'].includes(report.status) && equal(JSON.parse(read(path.join(base, 'result.json'))), report) && report.evidence_sha256 === modules.capture.runEvidence(base) &&
        report.workspace_sha256 === localPrep.identity(path.join(base, 'workspace'), ['.']).content_sha256 &&
        equal(JSON.parse(read(path.join(plan.directory, 'claims', row.id + '.json'))), { plan_sha256: planHash, directory: plan.directory, run: row.id }), 'CS-2 result, workspace or claim differs');
      const money = modules.prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
      must(money.actual_cost_micros === report.actual_cost_micros && money.attempts.length === report.observed_attempts && money.attempts.length <= row.call_ceiling, 'CS-2 canonical accounting differs');
      cost += money.actual_cost_micros; requests += money.attempts.length; claims.push(row.id + '.json');
      rows.push({ id: report.id, task_id: report.scope?.task, status: report.status, actual_cost_micros: money.actual_cost_micros, observed_attempts: money.attempts.length });
    }
    must(cost === result.actual_cost_micros && requests === result.observed_attempts, 'CS-2 block totals differ');
    const decisionBytes = read(path.join(plan.directory, `decision-${block}.json`)), decision = JSON.parse(decisionBytes);
    must(decision.schema === 'cs-2-developer-decision/1' && decision.plan_sha256 === planHash && decision.block === block, 'CS-2 terminal decision differs');
    blockRefs.push({ block, result_sha256: decision.result_sha256, decision_sha256: sha(decisionBytes) });
  }
  must(rows.length === 46 && new Set(rows.map(row => row.id)).size === 46 && rows.every(row => typeof row.task_id === 'string') && new Set(rows.map(row => row.task_id)).size === 46 &&
    equal(fs.readdirSync(path.join(plan.directory, 'claims')).sort(), claims.sort()), 'CS-2 closure coverage or claims differ');
  return { reference: { ...reference, repository: path.resolve(reference.repository) }, block_refs: blockRefs, rows, actual_cost_micros: rows.reduce((n, row) => n + row.actual_cost_micros, 0), observed_attempts: rows.reduce((n, row) => n + row.observed_attempts, 0), task_ids: rows.map(row => row.task_id) };
}
function authenticateRelocatedBudget(input, repository, modules) {
  must(input.grant === modules.budget.grant && Array.isArray(input.additional_paid_calls) && input.additional_paid_calls.length === 0, 'Exact existing authorization without extra paid calls required');
  const owner = modules.budget.document(input.grant_record);
  must(owner.schema === 'cs2-continuation-authorization/1' && owner.user_authorization === 'You are authorized to spend up to $100 via openrouter calls.' && owner.new_total_cap_micros === 100000000 &&
    owner.endpoint === 'https://openrouter.ai/api/v1/responses' && owner.model === 'openai/gpt-5.6-luna' && owner.provider_endpoint === 'amazon-bedrock/us-east-1' &&
    owner.plan === input.cs2.plan.file && owner.plan_sha256 === input.cs2.plan.sha256 && owner.refresh_new_requests === 0, 'Owner grant or CS-2 plan membership differs');
  const history = modules.budget.history(input.historical_cs1), current = terminalCs2Relocated(input.cs2, repository, modules);
  must(current.actual_cost_micros + 94500000 <= 100000000, 'Authorization cannot reserve original authoring envelope');
  return { grant: modules.budget.grant, grant_record: input.grant_record, cap_micros: 100000000, history, current, active_micros: 0, unresolved_micros: 0 };
}
function canonicalViews(evidence, costs, profile, scope) {
  must(profile.routing == null && scope && typeof scope.task === 'string', 'Frozen fixed-provider scope required');
  for (const [view, pages] of Object.entries({ costs, ...evidence })) {
    must(Array.isArray(pages) && pages.length > 0 && pages.length <= 64, 'Mandatory view pages missing');
    for (const page of pages) must(page.schema_version === 1 && page.view === view && equal(page.scope, scope) && Array.isArray(page.items) && Array.isArray(page.gaps), 'Canonical view scope/version differs');
    for (const page of pages) for (const gap of page.gaps) must(view === 'routing' && equal(gap, fixedRoutingGap) || ['outputs', 'context', 'tools', 'routing'].includes(view) && localPrior.privacyGap(gap, gap.artifact), 'Mandatory canonical evidence unavailable');
  }
  const records = (pages, collection) => pages.flatMap(p => p.items).filter(i => i.collection === collection).sort((a, b) => String(a.id).localeCompare(String(b.id)));
  for (const collection of ['attempt', 'settlement']) {
    const expected = records(costs, collection), routed = records(evidence.routing, collection);
    must(new Set(expected.map(i => i.id)).size === expected.length && equal(routed, expected), 'Routing attempts/settlements differ from canonical accounting');
  }
  const descriptors = new Map();
  for (const [view, pages] of Object.entries(evidence)) for (const item of pages.flatMap(p => p.items).filter(i => i.collection === 'artifact')) {
    must(item.visibility === 'available' && item.record?.spec?.id === item.id && equal(item.record.spec.scope, scope), 'Foreign/unavailable canonical descriptor');
    const old = descriptors.get(item.id); must(!old || equal(old, item), 'Overlapping canonical artifact descriptors differ'); descriptors.set(item.id, item);
    if (view === 'routing') must(['outputs', 'context', 'tools'].some(v => evidence[v].some(p => p.items.some(i => equal(i, item)))), 'Routing artifact absent from captured artifact views');
  }
}
function canonicalResponseLinks(responses, attempts) {
  must(Array.isArray(responses) && responses.length > 0 && Array.isArray(attempts), 'Canonical responses and attempts required');
  const settled = attempts.filter(attempt => attempt.phase === 'settled').map(attempt => attempt.provider_request);
  must(settled.length > 0 && settled.every(id => typeof id === 'string' && id.length) && new Set(settled).size === settled.length, 'Settled provider request identities differ');
  const observed = [];
  for (const response of responses) {
    must(response?.item?.record?.spec?.channel === 'response' && Buffer.isBuffer(response.bytes), 'Canonical response artifact required');
    const ids = new Set();
    for (const block of response.bytes.toString('utf8').split(/\r?\n\r?\n/)) {
      const data = block.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
      if (!data || data === '[DONE]') continue;
      const event = JSON.parse(data);
      if (typeof event.response?.id === 'string') ids.add(event.response.id);
    }
    must(ids.size === 1, 'Each response artifact must bind exactly one provider request');
    observed.push([...ids][0]);
  }
  must(observed.length === settled.length && new Set(observed).size === observed.length && equal([...observed].sort(), [...settled].sort()), 'Response artifacts do not exactly cover settled provider requests');
  return { settled_requests: settled.length, response_artifacts: observed.length };
}
function bound(file, hash, limit = 16 * 1024 * 1024) { const bytes = read(plain(path.resolve(file)), limit); must(sha(bytes) === hash, 'Pinned predecessor or reconciliation bytes changed'); return JSON.parse(bytes); }
function fileRef(file) { const bytes = read(plain(file), 16 * 1024 * 1024); return { file, sha256: sha(bytes) }; }
function closure(directory) {
  const names = fs.readdirSync(plain(directory)).sort();
  must(names.length > 0 && names.length <= 512, 'Reconciliation file count exceeds bound');
  let total = 0;
  return names.map(name => { const file = safeChild(directory, name), bytes = read(file, 16 * 1024 * 1024); total += bytes.length; must(total <= 268435456, 'Reconciliation byte bound exceeded'); return { name, bytes: bytes.length, sha256: sha(bytes) }; });
}
// Each raw invocation is authenticated against the capture receipt and its full
// argument vector. Replayed callbacks only return retained bytes; never invoke.
function replayCapture(directory, sourceBase, plan, row, task, receipt, prior) {
  const calls = new Map(), descriptors = new Map(), used = new Set(); let total = 0, elapsed = 0;
  must(Array.isArray(receipt.calls) && receipt.calls.length > 0 && receipt.calls.length <= 128, 'Missing bounded canonical inspection calls');
  receipt.calls.forEach((metadata, index) => {
    const raw = JSON.parse(read(path.join(directory, `inspection-${String(index + 1).padStart(3, '0')}.json`), 16 * 1024 * 1024));
    const args = metadata.args;
    must(equal(raw.metadata, metadata) && metadata.index === index + 1 && metadata.original_timeout_ms === 30000 && Number.isSafeInteger(metadata.timeout_ms) && metadata.timeout_ms > 0 && metadata.timeout_ms <= 120000 && Number.isSafeInteger(metadata.elapsed_ms) && metadata.elapsed_ms >= 0, 'Raw inspection metadata differs');
    must(Array.isArray(args) && equal(args.slice(0, 8), ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(sourceBase, 'workspace'), '--data-dir', path.join(sourceBase, 'data'), 'inspect']) && args[8] === metadata.id && args[9] === '--view' && args[10] === metadata.view && views.includes(args[10]) && args[11] === '--limit' && args[12] === '128', 'Unapproved reconciliation command');
    must(raw.result.status === 0 && !raw.result.error && metadata.status === 0 && !metadata.error && typeof raw.result.stdout === 'string' && typeof raw.result.stderr === 'string', 'Canonical inspection did not succeed');
    total += Buffer.byteLength(JSON.stringify(raw.result)); elapsed += metadata.elapsed_ms;
    must(total === metadata.cumulative_output_bytes && total <= 268435456 && elapsed <= 7200000, 'Reconciliation aggregate bounds differ');
    const frames = prior.boundaries.frames(raw.result.stdout).filter(frame => frame.type === 'result');
    must(frames.length === 1 && Array.isArray(frames[0].data?.items) && Array.isArray(frames[0].data?.gaps), 'Ambiguous retained inspection page');
    const key = JSON.stringify(args); must(!calls.has(key), 'Duplicate canonical inspection invocation');
    calls.set(key, { result: raw.result, page: frames[0].data, args });
  });
  const call = (executable, args, timeout) => {
    must(executable === plan.executable && timeout === 30000 && equal(args.slice(0, 8), ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(directory, 'workspace'), '--data-dir', path.join(directory, 'data'), 'inspect']), 'Only frozen read-only replay allowed');
    const redirected = [...args]; redirected[4] = path.join(sourceBase, 'workspace'); redirected[6] = path.join(sourceBase, 'data');
    const key = JSON.stringify(redirected), retained = calls.get(key); must(retained, 'Mandatory canonical page/range unavailable'); used.add(key); return retained.result;
  };
  const evidence = {};
  for (const view of views) {
    evidence[view] = prior.boundaries.inspection(plan, directory, task, view, call);
    must(equal(evidence[view], JSON.parse(read(path.join(directory, view + '.json')))), 'View differs from canonical raw pages');
    for (const page of evidence[view]) {
      must(page.schema_version === 1 && page.view === view && page.scope?.task === task && page.gaps.every(g => view === 'routing' && row.profile.routing == null && equal(g, fixedRoutingGap) || ['outputs', 'context', 'tools', 'routing'].includes(view) && prior.privacyGap(g, g.artifact)), 'Mandatory canonical view identity/gaps differ');
      for (const item of page.items.filter(i => i.collection === 'artifact')) {
        if (view === 'routing') continue; // Cross-view equality is checked below.
        const d = item.record, length = Number(d?.length);
        must(['outputs', 'context', 'tools'].includes(view) && item.visibility === 'available' && /^[A-Za-z0-9_-]{1,128}$/.test(item.id) && d?.spec?.id === item.id && d.spec.scope?.task === task && d.state === 'complete' && Number.isSafeInteger(length) && length >= 0 && length <= 1048576 && /^[a-f0-9]{64}$/.test(d.sha256), 'Unavailable or foreign artifact descriptor');
        const key = view + ':' + item.id; must(!descriptors.has(key) || equal(descriptors.get(key), item), 'Duplicate descriptor drift'); descriptors.set(key, item);
      }
    }
  }
  function artifact(view, id) {
    const item = descriptors.get(view + ':' + id); must(item, 'Mandatory artifact descriptor unavailable');
    const d = item.record, length = Number(d.length), chunks = [];
    for (let offset = 0; offset < Math.max(1, length); offset += 65536) {
      const pages = prior.boundaries.inspection(plan, directory, id, view, call, ['--offset', String(offset), '--length', '65536']);
      const r = pages[0]?.items[0], end = Math.min(offset + 65536, length);
      must(pages.length === 1 && pages[0].schema_version === 1 && pages[0].view === view && equal(pages[0].scope, d.spec.scope) && pages[0].items.length === 1 && pages[0].next_cursor == null && pages[0].gaps.every(g => prior.privacyGap(g, id)) && r?.artifact === id && r.visibility === 'available' && equal(r.descriptor, d) && r.range?.start === offset && r.range.end === end && Array.isArray(r.bytes) && r.bytes.length === end - offset && r.bytes.every(n => Number.isInteger(n) && n >= 0 && n <= 255), 'Canonical artifact range identity/privacy differs');
      chunks.push(Buffer.from(r.bytes));
    }
    const bytes = Buffer.concat(chunks); must(sha(bytes) === d.sha256 && bytes.equals(read(path.join(directory, 'artifact-' + sha(Buffer.from(id)) + '.bin'), 1048576)), 'Captured artifact differs from canonical ranges'); return bytes;
  }
  return { evidence, descriptors, call, artifact, finish() { must(used.size === calls.size, 'Unaccounted raw reconciliation invocation'); } };
}
function auditCapture(directory, base, plan, row, report, receipt, modules, money) {
  const { prior, runner, original, capture, prep } = modules;
  const replay = replayCapture(directory, base, plan, row, report.scope.task, receipt, prior), { evidence } = replay;
  canonicalViews(evidence, JSON.parse(read(path.join(directory, 'costs.json'))), row.profile, report.scope);
  const allowed = row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths;
  const finalFiles = original.finalWorkspace(base, row, allowed);
  must(read(path.join(directory, 'stdout.jsonl')).equals(read(path.join(base, 'stdout.jsonl'))), 'Original CLI output differs');
  const output = prior.boundaries.frames(read(path.join(base, 'stdout.jsonl')).toString('utf8'));
  must(output.find(f => f.type === 'accepted')?.scope?.task === report.scope.task && output.findLast(f => f.type === 'result')?.conditions?.completed === false, 'Original failed terminal task changed');
  const responses = [...replay.descriptors].filter(([key, item]) => key.startsWith('outputs:') && item.record.spec.channel === 'response').map(([, item]) => {
    const bytes = replay.artifact('outputs', item.id);
    must(bytes.equals(read(path.join(directory, 'response-' + sha(Buffer.from(item.id)) + '.sse'), 1048576)), 'Retained response stream differs'); return { item, bytes };
  });
  must(responses.length > 0, 'No complete provider streams retained');
  canonicalResponseLinks(responses, money.attempts);
  for (const [key, item] of replay.descriptors) if (key.startsWith('context:') && item.record.spec.schema === 'context-manifest/1') replay.artifact('context', item.id);
  const skill = original.skillEvidence(plan, directory, row, evidence.context, money.attempts, replay.call), tools = runner.auditTools(responses, row.profile.canonical_tools);
  const checks = evidence.verification.flatMap(p => p.items).filter(i => i.collection === 'verification').flatMap(i => i.record?.checks || []);
  let passed = 0;
  for (const check of checks) {
    must(check.specification === 'package.json#test', 'Unexpected native check');
    if (typeof check.output !== 'string') { must(check.outcome?.status !== 'passed', 'Passed native check lacks output'); continue; }
    const bytes = replay.artifact('tools', check.output), outcome = JSON.parse(bytes);
    must(bytes.equals(read(path.join(directory, 'native-outcome-' + sha(bytes) + '.json'))), 'Native outcome retention differs');
    for (const id of outcome.artifacts || []) {
      const item = replay.descriptors.get('tools:' + id); must(item, 'Native diagnostic descriptor unavailable');
      if (['stdout', 'stderr'].includes(item.record.spec.channel)) replay.artifact('tools', id);
    }
    if (outcome.native_preparation == null) { must(check.outcome?.status !== 'passed', 'Passed native check lacks preparation'); continue; }
    must(outcome.native_preparation.executable?.sha256 === plan.runtime.checker_sha256 && equal(outcome.plan?.request?.arguments, ['--test', '--test-reporter=tap', '--test-concurrency=1', 'checks/authoring.test.cjs']) && equal(outcome.plan.expected_tests, ['authoring input preservation', 'authoring output structure']) && outcome.plan.specification === 'package.json#test', 'Native outcome used another checker or contract');
    if (check.outcome?.status === 'passed' && check.exit_code === 0) {
      const streams = (outcome.artifacts || []).map(id => replay.descriptors.get('tools:' + id)).filter(i => i?.record.spec.channel === 'stdout'); must(streams.length === 1, 'Native pass lacks exact stdout');
      const bytes = replay.artifact('tools', streams[0].id), lines = bytes.toString('utf8').split(/\r?\n/).map(s => s.trimEnd());
      must(['authoring input preservation', 'authoring output structure'].every((name, i) => lines.includes(`ok ${i + 1} - ${name}`)) && !lines.some(s => s.startsWith('not ok')), 'Native pass TAP differs'); passed++;
    }
  }
  const native = { status: passed ? 'passed' : checks.length ? 'failed' : 'not_run', checks: checks.length, passed };
  // Complete capture is required; a failed terminal response is legitimate failed
  // baseline evidence and never upgraded by this supplemental audit.
  const item = runner.tasks().find(i => i.task.id === row.case_id), definition = JSON.parse(read(path.join(item.root, item.task.expected.oracle.path)));
  let answer, answerFailure, oracle;
  try { answer = capture.responseAnswer(responses, money.attempts); } catch (error) { answerFailure = error.message; }
  if (answer) { must(equal(JSON.parse(read(path.join(directory, 'answer.json'))), answer.answer), 'Supplemental answer differs'); oracle = modules.oracle.check(row.case_id, answer.answer, { finalFiles, fixtureRoot: item.root }); }
  else must(!fs.existsSync(path.join(directory, 'answer.json')), 'Missing terminal answer was fabricated');
  const canary = capture.canaryDisclosed(directory, definition, []) || allowed.some(relative => finalFiles.has(relative) && (definition.forbidden_output_literals || []).some(literal => finalFiles.get(relative).includes(literal)));
  must(canary === false, 'Captured response or final output disclosed forbidden canary');
  const audits = { preservation: true, native_check: native, skill, tools, ...(answerFailure ? { answer_failure: answerFailure } : {}), ...(oracle ? { oracle } : {}), canary_disclosed: false };
  must(equal(audits, receipt.audits), 'Receipt audit assertions differ from recomputed canonical evidence'); replay.finish();
  return { status: 'failed', actual_cost_micros: money.actual_cost_micros, observed_attempts: money.attempts.length, preserved: true, skill_evidence: skill, tool_audit: tools, native_check: native, ...(oracle ? { oracle } : {}), canary_disclosed: false, ...(answerFailure ? { answer_error: answerFailure } : {}), conditions: output.findLast(f => f.type === 'result').conditions, workspace_sha256: prep.identity(path.join(base, 'workspace'), ['.']).content_sha256 };
}
function docTerminal(envelope, modules) {
  const { runner, blind } = modules, directory = path.join(envelope.directory, 'phases/document-authoring--normal');
  const gate = bound(path.join(directory, 'review-gate.json'), PIN.doc_gate);
  must(gate.schema === 'cs1-fresh-qualification-gate/1' && gate.envelope_sha256 === PIN.envelope, 'DOC terminal gate binding differs');
  const plan = bound(path.join(directory, 'plan.json'), gate.phase_sha256), result = bound(path.join(directory, 'result.json'), gate.result_sha256);
  must(equal(plan, runner.derivePlan(envelope, PIN.envelope, 'document-authoring', 'normal')), 'DOC frozen plan changed');
  runner.resultEvidence(envelope, plan, result, gate.phase_sha256);
  const owner = bound(path.join(directory, 'review/owner.json'), gate.owner);
  const reviews = owner.reviews.map((ref, i) => bound(path.join(directory, `review/review-${i}.json`), ref.sha256));
  const raws = reviews.map((r, i) => { const b = read(path.join(directory, `review/blind-source-${i}.json`)); must(sha(b) === r.source_review.sha256, 'DOC raw reader changed'); return b; });
  blind.validate(directory, gate.phase_sha256, gate.result_sha256, reviews, raws);
  owner.native_checks.forEach((c, i) => c.evidence.forEach((ref, j) => { const b = read(path.join(directory, `review/native-${i}-${j}.json`)); must(sha(b) === ref.sha256, 'DOC owner native evidence changed'); if (c.status === 'passed') runner.nativeReceipt(plan, result, c.case_id, b, gate.phase_sha256); }));
  must(equal(gate.decision, runner.reviewDecision(plan, result, owner, reviews)) && equal(gate.decision, { candidate_gates_pass: false, winning_case_ids: [], qualifies: false, terminal: true }) && result.runs.length === 6 && result.actual_cost_micros === 92144 && result.observed_attempts === 41, 'DOC terminal disposition/accounting differs');
  return { gate: fileRef(path.join(directory, 'review-gate.json')), phase_sha256: gate.phase_sha256, result_sha256: gate.result_sha256, rows: result.runs.map(r => ({ id: r.id, task_id: r.scope.task, status: r.status, actual_cost_micros: r.actual_cost_micros, observed_attempts: r.observed_attempts })) };
}
function inspect(reference) {
  must(reference && equal(Object.keys(reference).sort(), ['capture', 'cs2_repository', 'envelope', 'repository']) && path.isAbsolute(reference.repository) && path.isAbsolute(reference.cs2_repository) && path.isAbsolute(reference.envelope) && reference.capture && equal(Object.keys(reference.capture).sort(), ['closure_sha256', 'file', 'sha256']) && path.isAbsolute(reference.capture.file), 'Exact predecessor repositories/envelope/capture reference required');
  const repository = plain(path.resolve(reference.repository)), envelope = bound(reference.envelope, PIN.envelope);
  must(path.resolve(reference.envelope) === path.join(envelope.directory, 'envelope.json') && envelope.source.content_sha256 === PIN.source && equal(localPrep.identity(repository, envelope.source.scope), envelope.source), 'Frozen predecessor repository changed');
  const load = name => require(path.join(repository, 'scripts/evals', name + '.cjs'));
  const modules = { prior: load('p6-live-runner'), prep: load('authoring-prepare'), runner: load('authoring-qualification'), original: load('authoring-runner'), capture: load('developer-runner'), blind: load('authoring-qualification-review'), budget: load('authoring-qualification-budget'), oracle: load('authoring-followup-oracle') };
  const { runner, prep, prior, original } = modules;
  // The original CS-2 reference named the then-current repository checkout.
  // Revalidate it against an explicit immutable source worktree after that path
  // advances, while preserving the original envelope bytes and path provenance.
  const specBytes = read(envelope.spec_source, 262144), spec = JSON.parse(specBytes);
  must(sha(specBytes) === PIN.spec && envelope.spec_sha256 === PIN.spec && spec?.budget?.cs2 &&
    path.resolve(spec.budget.cs2.repository) === path.resolve(envelope.budget.current.reference.repository) &&
    equal(spec.budget.cs2.plan, envelope.budget.current.reference.plan), 'Original authoring specification changed');
  const relocatedBudgetInput = structuredClone(spec.budget), relocatedRepository = plain(path.resolve(reference.cs2_repository));
  relocatedBudgetInput.cs2.repository = relocatedRepository;
  const relocatedBudget = authenticateRelocatedBudget(relocatedBudgetInput, relocatedRepository, modules), normalizedBudget = structuredClone(relocatedBudget);
  normalizedBudget.current.reference.repository = envelope.budget.current.reference.repository;
  must(equal(normalizedBudget, envelope.budget), 'Original envelope budget does not match relocated immutable source evidence');
  must(sha(read(envelope.executable, 1073741824)) === envelope.executable_sha256 && sha(read(envelope.profile_source)) === envelope.profile_sha256 && sha(read(envelope.provider_catalog)) === envelope.provider_catalog_sha256 &&
    sha(read(envelope.runtime.source_checker, 268435456)) === envelope.runtime.checker_sha256 && sha(read(envelope.runtime.build_receipt)) === envelope.runtime.build_receipt_sha256,
  'Original executable/profile/catalog/checker inputs changed');
  const phase = path.join(envelope.directory, 'phases/skill-authoring--normal'), plan = bound(path.join(phase, 'plan.json'), PIN.phase), row = plan.runs[0], base = path.join(phase, consumed), report = bound(path.join(base, 'result.json'), PIN.result);
  must(row.id === consumed && report.id === consumed && report.status === 'failed' && report.reason === 'Canonical inspection unavailable' && plan.envelope_sha256 === PIN.envelope && runner.runEvidence(base) === PIN.evidence && report.evidence_sha256 === PIN.evidence && report.workspace_sha256 === prep.identity(path.join(base, 'workspace'), ['.']).content_sha256 && sha(read(plan.executable, 1073741824)) === PIN.cli, 'Consumed failed predecessor changed');
  const receipt = bound(reference.capture.file, reference.capture.sha256), directory = plain(path.dirname(reference.capture.file)), costBase = path.join(path.dirname(directory), 'capture');
  const captureFiles = closure(directory);
  must(sha(JSON.stringify(captureFiles)) === reference.capture.closure_sha256, 'Exact owner-bound reconciliation file closure changed');
  must(path.basename(reference.capture.file) === 'receipt.json' && !within(envelope.directory, directory) && !within(repository, directory) && receipt.schema === 'cs1-skl-readonly-evidence-reconciliation/1' && receipt.status === 'captured' && receipt.original_status === 'failed' && receipt.original_outcome_preserved === true && receipt.provider_calls === 0 && receipt.execution_attempts === 0 && receipt.qualification === false && equal(receipt.failures, []) && receipt.original_evidence_unchanged === true && receipt.run_id === consumed && receipt.task_id === report.scope.task, 'Complete failed-row reconciliation required');
  const files = [reference.envelope, path.join(phase, 'plan.json'), path.join(phase, 'result.json'), path.join(base, 'result.json'), path.join(envelope.directory, 'halt.json'), path.join(envelope.directory, 'active-phase.json'), path.join(phase, 'execution-claim.json'), path.join(envelope.directory, 'claims', consumed + '.json'), runner.successorClaim(), path.join(costBase, 'receipt.json'), path.join(costBase, 'costs.json'), path.join(costBase, 'inspection-1.json')];
  const snapshot = { files: files.map(fileRef), run_evidence_sha256: runner.runEvidence(base), workspace_sha256: report.workspace_sha256, source_sha256: PIN.source, cli_sha256: PIN.cli };
  must(equal(snapshot, JSON.parse(read(path.join(directory, 'before.json')))) && equal(snapshot, JSON.parse(read(path.join(directory, 'after.json')))), 'Current original identities differ from both capture snapshots');
  const claim = { envelope_sha256: PIN.envelope, phase_sha256: PIN.phase };
  must(equal(JSON.parse(read(path.join(envelope.directory, 'active-phase.json'))), claim) && equal(JSON.parse(read(path.join(phase, 'execution-claim.json'))), claim) && equal(JSON.parse(read(path.join(envelope.directory, 'claims', consumed + '.json'))), { ...claim, slot: consumed }) && equal(JSON.parse(read(runner.successorClaim())), { grant: modules.budget.grant, directory: envelope.directory, envelope_sha256: PIN.envelope }), 'Original halt/active/exclusive claims differ');
  const phaseResult = JSON.parse(read(path.join(phase, 'result.json')));
  must(phaseResult.stopped === true && equal(phaseResult.runs[0], report) && phaseResult.runs.length === 6 && phaseResult.runs.slice(1).every(r => r.status === 'not_run'), 'Original stopped prefix changed');
  for (const pending of plan.runs.slice(1)) { const dir = path.join(phase, pending.id); must(original.preserved(dir, pending) && fs.readdirSync(plain(path.join(dir, 'data'))).length === 0 && !fs.existsSync(path.join(dir, 'attempted.json')) && !fs.existsSync(path.join(dir, 'result.json')), 'Original undispatched slot has changed or was attempted'); }
  const doc = docTerminal(envelope, modules);
  must(equal(fs.readdirSync(path.join(envelope.directory, 'phases')).sort(), ['document-authoring--normal', 'skill-authoring--normal']) && equal(fs.readdirSync(path.join(envelope.directory, 'claims')).sort(), [...doc.rows.map(r => r.id + '.json'), consumed + '.json'].sort()), 'Original phase/claim coverage changed');
  const costReceipt = bound(path.join(costBase, 'receipt.json'), PIN.receipt), costs = bound(path.join(costBase, 'costs.json'), PIN.costs), rawCost = bound(path.join(costBase, 'inspection-1.json'), PIN.cost_inspection);
  must(rawCost.status === 0 && !rawCost.error && equal(costs, prior.boundaries.frames(rawCost.stdout).filter(f => f.type === 'result').map(f => f.data)) && equal(costs, JSON.parse(read(path.join(directory, 'costs.json')))) && equal(JSON.parse(read(path.join(directory, 'cost-source.json'))), { receipt_sha256: PIN.receipt, costs_sha256: PIN.costs }), 'Cost reconciliation raw evidence differs');
  const money = prior.accounting(costs, row.cap_micros);
  must(money.actual_cost_micros === 30538 && money.attempts.length === 12 && receipt.actual_cost_micros === 30538 && receipt.requests === 12 && costReceipt.actual_cost_micros === 30538 && costReceipt.requests === 12, 'Reconciled first-row accounting differs');
  const supplemental = auditCapture(directory, base, plan, row, report, receipt, modules, money);
  must(equal(captureFiles, closure(directory)), 'Capture changed during read-only audit');
  const manifest = { schema: 'cs1-skl-reconciliation-manifest/1', predecessor: reference, original_snapshot: snapshot, capture_files: captureFiles, cost_files: closure(costBase), doc, original_status: 'failed', supplemental, task_id: report.scope.task, actual_cost_micros: 30538, observed_attempts: 12 };
  return { manifest, modules, envelope, plan, row, report, capture_directory: directory, original_base: base };
}
module.exports = { PIN, consumed, inspect, replayCapture, auditCapture, closure, docTerminal, canonicalViews, canonicalResponseLinks, fixedRoutingGap };
