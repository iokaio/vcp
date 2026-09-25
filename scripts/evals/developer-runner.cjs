// SPDX-License-Identifier: Apache-2.0
'use strict';
// One-shot CS-2 developer campaign blocks. The harness never executes or applies
// model JSON: artifact edits occur only through the scoped native vcp_patch tool,
// and the only process a run may start is the pinned data-only checker.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs');
const cs1 = require('./authoring-runner.cjs'), { identity } = require('./authoring-prepare.cjs');
const prep = require('./developer-prepare.cjs');
const oracle = require('./developer-oracle.cjs');
const candidates = require('./developer-candidates.cjs');
const { plain, read, write, safeChild, frames, inspection, invoke, usd } = prior.boundaries;
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const { limits } = prep;

function claimed(plan) {
  return new Set(plan.runs.filter(row => fs.existsSync(path.join(plan.directory, 'claims', row.id + '.json'))).map(row => row.id));
}
// Re-derives the whole preparation from its spec and compares it exactly, so any
// changed source, fixture, candidate, profile, qualification window or pinned
// identity refuses dispatch. Unstarted runs must still hold pristine inputs.
function validate(plan, file, started = claimed(plan)) {
  prior.boundaries.noParentInstructions(plan.directory); prior.boundaries.privateDirectory(plan.directory);
  if (plan.schema !== 'cs-2-developer-preparation/1' || plain(path.dirname(path.resolve(file))) !== plan.directory) throw Error('Prepared plan contract differs');
  if (!equal(JSON.parse(read(path.join(plan.directory, 'preparation-owner.json'))), { schema: plan.schema, spec_sha256: plan.spec_sha256 })) throw Error('Preparation ownership changed');
  let expected;
  try { expected = prep.describe(plan.spec_source, plan.directory); }
  catch (error) { throw Error('Frozen preparation inputs changed or qualification expired: ' + error.message); }
  if (!equal(plan, expected)) throw Error('Frozen preparation identity, allocation or pinned identity changed');
  const { cases } = prep.staged(plan.directory, plan.runtime, prep.order(prep.tasks().items));
  if (sha(read(plan.runtime.checker, 256 * 1024 * 1024)) !== plan.runtime.checker_sha256 || !read(plan.runtime.cases_file).equals(cases)) throw Error('Staged checker or case map changed');
  for (const row of plan.runs) {
    const base = safeChild(plan.directory, row.id);
    if (sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256 || !equal(JSON.parse(read(path.join(base, 'profile.json'))), row.profile)) throw Error('Prepared prompt or profile changed');
    if (!started.has(row.id) && (!cs1.preserved(base, row) || fs.readdirSync(plain(path.join(base, 'data'))).length)) throw Error('Prepared inputs changed or data store not fresh');
  }
}
// One owner-local claim keyed to this campaign: a second preparation cannot draw
// on the same pre-authorization. The Git control directory is outside every task.
function campaignClaim() {
  const common = execFileSync('git', ['-c', `safe.directory=${repository}`, '-C', repository, 'rev-parse', '--path-format=absolute', '--git-common-dir'], { encoding: 'utf8', timeout: 30000 }).trim();
  if (!path.isAbsolute(common)) throw Error('Canonical repository control directory required');
  return path.join(plain(common), 'vcp-cs2-developer-campaign.json');
}
function halt(plan, reason) {
  const file = path.join(plan.directory, 'halt.json');
  if (!fs.existsSync(file)) write(file, { schema: 'cs-2-developer-halt/1', reason, action: 'Read-only reconciliation. This campaign cannot resume or replay.' });
}
function runEvidence(base) {
  const names = fs.readdirSync(plain(base)).filter(name => !['workspace', 'data', 'result.json'].includes(name)).sort();
  if (names.length > 128) throw Error('Run evidence file bound exceeded');
  return sha(JSON.stringify(names.map(name => ({ path: name, sha256: sha(read(safeChild(base, name), 16 * 1024 * 1024)) }))));
}
// Cumulative settled accounting over every claimed run must still reserve the
// next full USD 3 / 16-request slot inside USD 162 / 864 requests.
function admission(plan) {
  let cost = 0, attempts = 0;
  for (const row of plan.runs) {
    if (!fs.existsSync(path.join(plan.directory, 'claims', row.id + '.json'))) continue;
    const base = path.join(plan.directory, row.id), file = path.join(base, 'result.json');
    if (!fs.existsSync(file)) throw Error('A claimed run has no retained result; reconcile unknown liability');
    const report = JSON.parse(read(file));
    if (!Number.isSafeInteger(report.actual_cost_micros) || report.evidence_sha256 !== runEvidence(base)) throw Error('A claimed run has unknown accounting or changed evidence');
    const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    if (money.actual_cost_micros !== report.actual_cost_micros || money.attempts.length !== report.observed_attempts) throw Error('Retained canonical accounting differs');
    cost += money.actual_cost_micros; attempts += money.attempts.length;
  }
  if (!Number.isSafeInteger(cost) || cost + limits.slot_micros > limits.cap_micros) throw Error('Cumulative USD 162 cap cannot reserve the next USD 3 run');
  if (attempts + limits.slot_requests > limits.requests) throw Error('Cumulative 864 request cap cannot reserve the next 16 requests');
  return { actual_cost_micros: cost, observed_attempts: attempts, reserved_micros: limits.slot_micros, reserved_requests: limits.slot_requests, remaining_micros: limits.cap_micros - cost, remaining_requests: limits.requests - attempts };
}
// Exact bytes of one retained artifact, read by range and checked against its digest.
function retained(plan, base, item, view, call) {
  const descriptor = item.record, length = Number(descriptor?.length);
  if (descriptor?.state !== 'complete' || !Number.isSafeInteger(length) || length < 1 || length > 1024 * 1024) throw Error('Incomplete retained artifact');
  const chunks = [];
  for (let offset = 0; offset < length; offset += 65536) {
    const pages = inspection(plan, base, item.id, view, call, ['--offset', String(offset), '--length', '65536']);
    const row = pages[0]?.items[0], end = Math.min(offset + 65536, length);
    if (pages.length !== 1 || pages[0].items.length !== 1 || pages.some(p => p.gaps.some(g => !prior.privacyGap(g, item.id))) || row?.range?.start !== offset || row.range.end !== end || row.artifact !== undefined && row.artifact !== item.id || row.visibility !== undefined && row.visibility !== 'available' || !Array.isArray(row.bytes) || row.bytes.length !== end - offset || row.bytes.some(b => !Number.isInteger(b) || b < 0 || b > 255)) throw Error('Retained artifact range unavailable');
    chunks.push(Buffer.from(row.bytes));
  }
  const bytes = Buffer.concat(chunks);
  if (sha(bytes) !== descriptor.sha256) throw Error('Retained artifact digest mismatch');
  return bytes;
}
// Context parts the host names skill-<sha256(qualified id)>-<part index> for each active skill.
function skillParts(plan, row) {
  const catalog = JSON.parse(read(path.join(path.dirname(plan.executable), 'skills/builtin/catalog.json')));
  const entries = candidates.inspect().entries;
  return row.skills.flatMap(qualified => {
    const candidate = entries.find(entry => entry.qualified_id === qualified), builtin = catalog.skills.find(skill => qualified === `vcp-builtin::${skill.id}::${skill.id}`);
    if (!candidate === !builtin) throw Error('Unknown or ambiguous selected skill');
    const parts = candidate ? candidate.parts : [builtin.body, ...(builtin.resources || [])];
    return parts.map((part, index) => ({ id: `skill-${sha(Buffer.from(qualified))}-${index}`, hash: part.sha256 }));
  });
}
function skillEvidence(plan, base, row, pages, attempts, call) {
  if (pages.some(p => p.gaps.some(g => !prior.privacyGap(g, g.artifact)))) throw Error('Context evidence incomplete');
  const manifests = pages.flatMap(p => p.items).filter(i => i.collection === 'artifact' && i.record?.spec?.schema === 'context-manifest/1').map(item => ({ artifact: item.id, manifest: JSON.parse(retained(plan, base, item, 'context', call)) }));
  const expected = skillParts(plan, row), dispatched = attempts.filter(a => a.phase === 'settled');
  if (!dispatched.length) throw Error('No settled model attempt');
  for (const attempt of dispatched) {
    const matched = manifests.filter(m => m.manifest.request_sha256 === attempt.request_digest);
    if (!matched.length) throw Error('No canonical context for dispatched request');
    for (const { manifest } of matched) {
      if (!Array.isArray(manifest.included)) throw Error('Invalid context manifest');
      const active = manifest.included.filter(p => p.kind === 'skill');
      if (active.length !== expected.length || expected.some(part => active.filter(p => p.id === part.id && p.source_hash === part.hash && p.trust === 'active_skill').length !== 1)) throw Error('Dispatched skill context differs from the arm selection');
    }
  }
  return { skills: row.skills, parts: expected.length, checked_attempts: dispatched.length, manifests: manifests.map(m => m.artifact) };
}
// The pinned checker ran iff a passed package.json#test check's retained outcome
// binds the pinned executable hash and fixed invocation, and its retained stdout
// carries both TAP lines. Any check run by another identity is an authority stop.
function nativeCheck(plan, base, verification, artifacts, call) {
  if (verification.some(p => p.gaps.length)) throw Error('Verification evidence incomplete');
  const checks = verification.flatMap(p => p.items).filter(i => i.collection === 'verification').flatMap(i => i.record?.checks ?? []);
  const descriptors = new Map(artifacts.flatMap(p => p.items).filter(i => i.collection === 'artifact').map(i => [i.id, i]));
  let passed = 0;
  for (const check of checks) {
    if (check.specification !== 'package.json#test') throw Error('Unexpected verification check');
    if (typeof check.output !== 'string' || !descriptors.has(check.output)) { if (check.outcome?.status === 'passed') throw Error('Passed check lacks retained outcome evidence'); continue; }
    const outcome = JSON.parse(retained(plan, base, descriptors.get(check.output), 'tools', call));
    if (outcome.native_preparation?.executable?.sha256 !== plan.runtime.checker_sha256 || !equal(outcome.plan?.request?.arguments, prep.checkerArguments) || !equal(outcome.plan?.expected_tests, prep.checkerTests) || outcome.plan?.specification !== 'package.json#test') throw Error('Verification ran something other than the pinned checker');
    if (check.outcome?.status !== 'passed' || check.exit_code !== 0) continue;
    const stdout = (outcome.artifacts || []).map(id => descriptors.get(id)).filter(item => item?.record?.spec?.channel === 'stdout');
    if (stdout.length !== 1) throw Error('Passed check lacks one retained stdout');
    const lines = retained(plan, base, stdout[0], 'tools', call).toString('utf8').split(/\r?\n/).map(line => line.trimEnd());
    if (!prep.checkerTests.every((name, index) => lines.includes(`ok ${index + 1} - ${name}`)) || lines.some(line => line.startsWith('not ok'))) throw Error('Passed check stdout differs from the pinned checker TAP');
    passed++;
  }
  return { status: passed ? 'passed' : checks.length ? 'failed' : 'not_run', checks: checks.length, passed };
}
// The synthetic canary may appear only in inputs. Its presence in any retained
// model output or answer is a case-level failure, never a campaign halt.
function canaryDisclosed(base, definition) {
  const literals = definition.forbidden_output_literals || [];
  if (!literals.length) return false;
  const names = fs.readdirSync(plain(base)).filter(name => name === 'answer.json' || name === 'stdout.jsonl' || /^response-[a-f0-9]{64}\.sse$/.test(name));
  return names.some(name => { const text = read(safeChild(base, name), 16 * 1024 * 1024).toString('utf8'); return literals.some(literal => text.includes(literal)); });
}
function run(file, authorization, block, call = invoke) {
  const bytes = read(file, 16 * 1024 * 1024);
  if (sha(bytes) !== authorization) throw Error('Authorization must name the exact prepared plan hash');
  if (!candidates.ids.includes(block)) throw Error('Unknown campaign block');
  const plan = JSON.parse(bytes);
  if (fs.existsSync(path.join(plan.directory, 'halt.json'))) throw Error('Campaign halted: reconciliation only; no further dispatch');
  validate(plan, file);
  const claim = { plan_sha256: authorization, directory: plan.directory }, claimFile = campaignClaim();
  if (fs.existsSync(claimFile)) { if (!equal(JSON.parse(read(claimFile)), claim)) throw Error('Another prepared plan already holds the CS-2 campaign authorization'); }
  else write(claimFile, claim);
  const position = candidates.ids.indexOf(block);
  for (const earlier of candidates.ids.slice(0, position)) {
    const previous = path.join(plan.directory, `result-${earlier}.json`);
    if (!fs.existsSync(previous)) throw Error('Blocks run in campaign order');
    const result = JSON.parse(read(previous));
    if (result.stopped || result.final_inputs_unchanged !== true) throw Error('An earlier block stopped or changed inputs');
  }
  const active = path.join(plan.directory, 'active-block.json');
  if (fs.existsSync(active)) { halt(plan, 'An interrupted block left an active claim'); throw Error('An interrupted block requires reconciliation'); }
  // Exclusive block claims survive crashes and prevent concurrent or replayed blocks.
  write(path.join(plan.directory, 'claims', `block-${block}.json`), { ...claim, block });
  write(active, { ...claim, block });
  const rows = plan.runs.filter(row => row.block === block);
  const result = { schema: 'cs-2-developer-block-result/1', plan_sha256: authorization, block, quality: 'pending_functional_grading_and_blind_review', actual_cost_micros: 0, observed_attempts: 0, stopped: false, runs: rows.map(row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'not_run', actual_cost_micros: null })) };
  for (let index = 0; index < rows.length; index++) {
    if (result.stopped) break;
    const row = rows[index], report = result.runs[index], base = safeChild(plan.directory, row.id);
    let dispatched = false, accounted = false;
    try {
      if (sha(read(file, 16 * 1024 * 1024)) !== authorization) throw Error('Authorized plan changed');
      validate(plan, file);
      if (!equal(JSON.parse(read(active)), { ...claim, block })) throw Error('Active block ownership changed');
      write(path.join(base, 'budget-admission.json'), admission(plan));
      write(path.join(plan.directory, 'claims', row.id + '.json'), { ...claim, run: row.id });
      const profile = row.profile, args = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'), '--config', path.join(base, 'profile.json'), 'run', '--file', path.join(base, 'prompt.txt'), '--budget-usd', usd(row.cap_micros), '--autonomy', profile.maximum_autonomy];
      for (const skill of row.skills) args.push('--skill', skill);
      write(path.join(base, 'attempted.json'), { ...claim, args });
      dispatched = true;
      const start = Date.now(), execution = call(plan.executable, args, (profile.deadline_seconds + 180) * 1000);
      report.latency_ms = Date.now() - start;
      write(path.join(base, 'stdout.jsonl'), execution.stdout); write(path.join(base, 'stderr.txt'), execution.stderr);
      if (execution.error) throw Error('CLI interrupted; reconcile unknown liability');
      const output = frames(execution.stdout), accepted = output.find(f => f.type === 'accepted'), final = output.findLast(f => f.type === 'result');
      if (!accepted?.scope?.task || !final?.conditions) throw Error('Missing durable task result');
      report.scope = accepted.scope;
      const evidence = {};
      for (const view of ['costs', 'routing', 'outputs', 'context', 'tools', 'verification']) { evidence[view] = inspection(plan, base, accepted.scope.task, view, call); write(path.join(base, view + '.json'), evidence[view]); }
      const money = prior.accounting(evidence.costs, row.cap_micros);
      report.actual_cost_micros = money.actual_cost_micros; report.observed_attempts = money.attempts.length;
      result.actual_cost_micros += money.actual_cost_micros; result.observed_attempts += money.attempts.length; accounted = true;
      if (money.attempts.length > row.call_ceiling) throw Error('Observed per-run request ceiling exceeded');
      // Writes outside the editable paths, deletions or scaffold changes are authority failures.
      const finalFiles = cs1.finalWorkspace(base, row, profile.maximum_autonomy === 'plan' ? [] : profile.affected_paths);
      report.preserved = true;
      report.status = final.conditions.completed && execution.status === 0 ? 'completed' : 'failed'; report.conditions = final.conditions;
      if (money.attempts.some(a => a.phase === 'settled')) report.skill_evidence = skillEvidence(plan, base, row, evidence.context, money.attempts, call);
      report.native_check = row.write ? nativeCheck(plan, base, evidence.verification, evidence.tools, call) : { status: 'not_applicable' };
      if (row.write && report.native_check.status !== 'passed') report.status = 'failed';
      if (report.status === 'completed') {
        try {
          report.answer_source = prior.responseAnswer(plan, base, evidence.outputs, money.attempts, call);
          write(path.join(base, 'answer.json'), report.answer_source.answer);
          report.oracle = oracle.check(row.case_id, report.answer_source.answer, { finalFiles });
          write(path.join(base, 'oracle.json'), report.oracle);
          if (!report.oracle.structural_pass) report.status = 'failed';
        } catch (error) { report.status = 'failed'; report.reason = 'canonical_answer_or_oracle: ' + error.message; }
      }
      report.canary_disclosed = canaryDisclosed(base, oracle.load(row.case_id).oracle);
      if (report.canary_disclosed) { report.status = 'failed'; report.reason = 'synthetic_canary_disclosed'; }
      report.workspace_sha256 = identity(path.join(base, 'workspace'), ['.']).content_sha256;
      report.evidence_sha256 = runEvidence(base);
    } catch (error) { report.status = 'failed'; report.reason = error.message; if (dispatched && !accounted) result.actual_cost_micros = null; result.stopped = true; halt(plan, 'Integrity, authority or accounting stop; inspect the retained local result'); }
    write(path.join(base, 'result.json'), report);
  }
  try {
    // A halt intentionally prevents future validation; still check exact identity here.
    if (sha(read(file, 16 * 1024 * 1024)) !== authorization || !equal(plan, prep.describe(plan.spec_source, plan.directory))) throw Error('Final frozen identity changed');
    for (const [index, row] of rows.entries()) {
      const report = result.runs[index];
      if (report.workspace_sha256 && identity(path.join(plan.directory, row.id, 'workspace'), ['.']).content_sha256 !== report.workspace_sha256) throw Error('Earlier completed workspace changed');
    }
    result.final_inputs_unchanged = true;
  } catch (error) { result.final_inputs_unchanged = false; result.final_input_error = error.message; result.stopped = true; halt(plan, 'Final identity drift'); }
  write(path.join(plan.directory, `result-${block}.json`), result);
  if (!result.stopped) fs.unlinkSync(active);
  return result;
}
module.exports = { validate, run, admission, skillParts, skillEvidence, nativeCheck, canaryDisclosed, retained, runEvidence, campaignClaim };
if (require.main === module) {
  try {
    const [command, file, authorization, block, ...extra] = process.argv.slice(2);
    if (command !== 'run' || !file || !authorization || !block || extra.length) throw Error('Usage: developer-runner.cjs run <plan.json> <authorized-plan-sha256> <block-skill>');
    const result = run(file, authorization, block);
    console.log(JSON.stringify({ result: path.join(path.dirname(file), `result-${block}.json`), stopped: result.stopped, actual_cost_micros: result.actual_cost_micros }));
    if (result.stopped) process.exitCode = 1;
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
