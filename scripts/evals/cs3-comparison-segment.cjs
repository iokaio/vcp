// SPDX-License-Identifier: Apache-2.0
'use strict';
// One verifier-only continuation. Original observations and ownership stay immutable.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prep = require('./authoring-prepare.cjs'), policy = require('./cs3-comparison-policy.cjs'), capture = require('./developer-runner.cjs');
const { read, write, plain, within, privateDirectory, noParentInstructions, frames } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const originHash = '42d9b02b785408a1e31987b3336f99c17535af0dba1d5939a2f6e785155733e8';
const auditHash = 'af41954ec07086f6197076e45d320521d4f9241f58146ba0e61fea3f2978681c';
const allowedChanges = ['cs3-comparison.cjs', 'cs3-comparison-policy.cjs', 'cs3-comparison-policy.test.cjs', 'cs3-comparison-review.cjs',
  'cs3-comparison-host.test.cjs', 'cs3-comparison-segment.cjs', 'cs3-comparison-segment.test.cjs'].map(name => 'scripts/evals/' + name);
function campaign() { return require('./cs3-comparison.cjs'); }
function bound(ref) {
  if (!ref || !path.isAbsolute(ref.path || '') || !/^[a-f0-9]{64}$/.test(ref.sha256 || '')) throw Error('Exact segment reference required');
  const bytes = read(ref.path, 32 * 1024 * 1024);
  if (sha(bytes) !== ref.sha256) throw Error('Segment bound evidence changed');
  return JSON.parse(bytes);
}
function claimFile() { return path.join(path.dirname(campaign().claimFile(true)), 'vcp-cs3-deepseek-20260928-successor-v2-segment1-claim.json'); }
function pristine(origin, row) {
  const base = path.join(origin.directory, row.id);
  if (!equal(fs.readdirSync(base).sort(), ['data', 'profile.json', 'prompt.txt', 'workspace']) || fs.readdirSync(path.join(base, 'data')).length) throw Error('Untouched segment slot already contains execution evidence');
}
function addendum(origin, row, audited) {
  const base = path.join(origin.directory, row.id), previous = JSON.parse(read(path.join(base, 'result.json')));
  const money = policy.reread(base, row.cap_micros), output = frames(read(path.join(base, 'stdout.jsonl')).toString()), final = output.findLast(x => x.type === 'result');
  if (previous.status !== 'failed' || previous.id !== row.id || previous.case_id !== row.case_id || previous.arm !== row.arm || final?.conditions?.completed !== false
    || !equal(previous.scope, money.ledger_scope) || money.unresolved_attempts) throw Error('Consumed prefix must remain an authenticated failed observation');
  if (row.id === origin.runs[0].id && (!equal(policy.fields(previous), policy.fields(money)) || previous.evidence_sha256 !== audited.evidence_sha256)) throw Error('First consumed observation differs');
  if (row.id === origin.runs[1].id && (money.attempts.length || money.actual_cost_micros !== 0 || final.exit_code !== 1 || final.conditions.internal_failure !== true)) throw Error('Second consumed observation is not the exact zero-dispatch native failure');
  return { ...previous, ...policy.fields(money), observed_attempts: money.attempts.length, status: 'failed', accounted: true, preserved: true,
    conditions: final.conditions, output_error: previous.output_error || 'Native internal failure before provider dispatch; original failed observation retained',
    evidence_sha256: audited.evidence_sha256, verifier_addendum: { schema: 'cs3-comparison-verifier-addendum/1', original_result_sha256: audited.result_sha256,
      original_plan_sha256: originHash, correction: 'Known zero-dispatch accounting is zero, not unknown; quality remains failed; no replay.' } };
}
function verifyOrigin(spec) {
  if (!spec || !equal(Object.keys(spec).sort(), ['audit', 'origin']) || spec.origin.sha256 !== originHash || spec.audit.sha256 !== auditHash) throw Error('Exact single-segment origin/audit required');
  const origin = bound(spec.origin), audit = bound(spec.audit);
  if (origin.schema !== 'cs3-comparison-plan/2' || origin.segment || origin.runs.length !== 108 || path.dirname(spec.origin.path) !== origin.directory
    || audit.schema !== 'cs3-comparison-segment-origin/1' || audit.plan_sha256 !== originHash
    || !equal(audit.consumed.map(r => r.id), origin.runs.slice(0, 2).map(r => r.id)) || !equal(audit.remaining_ids, origin.runs.slice(2).map(r => r.id))) throw Error('Original exact two-plus-106 segmentation required');
  if (!path.isAbsolute(spec.origin.source_archive || '') || !equal(prep.identity(spec.origin.source_archive, origin.source.scope), origin.source)) throw Error('Original archived source identity changed');
  const globalClaim = campaign().claimFile(true);
  if (sha(read(globalClaim)) !== audit.global_claim_sha256 || !equal(JSON.parse(read(globalClaim)), { directory: origin.directory, plan_sha256: originHash })) throw Error('Original campaign global ownership changed');
  const headers = { 'halt.json': audit.halt_sha256, 'result-document-authoring.json': audit.block_result_sha256, 'active-block.json': audit.active_block_sha256 };
  for (const [name, hash] of Object.entries(headers)) if (sha(read(path.join(origin.directory, name))) !== hash) throw Error('Original halted control evidence changed');
  if (!equal(fs.readdirSync(path.join(origin.directory, 'claims')).sort(), audit.original_claims.map(r => path.basename(r.path)).sort())) throw Error('Original claim union changed');
  for (const item of audit.original_claims) if (sha(read(path.join(origin.directory, item.path))) !== item.sha256) throw Error('Original claim changed');
  for (const row of origin.runs) {
    const base = path.join(origin.directory, row.id);
    if (sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256 || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256
      || !equal(campaign().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path)))) throw Error('Original frozen slot inputs changed');
  }
  for (const item of audit.consumed) {
    const base = path.join(origin.directory, item.id);
    if (!equal(prep.identity(base, ['.']), item.inventory) || sha(read(path.join(base, 'result.json'))) !== item.result_sha256 || capture.runEvidence(base) !== item.evidence_sha256) throw Error('Consumed original raw evidence changed');
  }
  const current = campaign().sourceIdentity();
  for (const name of new Set([...origin.source.files, ...current.files].map(f => f.path))) {
    if (!equal(origin.source.files.find(f => f.path === name), current.files.find(f => f.path === name)) && !allowedChanges.includes(name)) throw Error('Segment changed non-verifier execution source: ' + name);
  }
  if (!equal(current.directories, origin.source.directories)) throw Error('Segment source directory scope changed');
  const build = bound(origin.spec.build_receipt); campaign().buildProvenance(build, origin.spec.executable);
  const nativeScope = build.source_inputs.scope.filter(p => p.startsWith('src/crates') || p.startsWith('src/skills/builtin') || p.startsWith('src/third_party'));
  if (!nativeScope.length) throw Error('Original native build source closure unavailable');
  const native = prep.identity(root, nativeScope), selected = p => nativeScope.some(prefix => p === prefix || p.startsWith(prefix + '/'));
  if (!equal(native.files, build.source_inputs.files.filter(f => selected(f.path))) || !equal(native.directories, build.source_inputs.directories.filter(selected))) throw Error('Original native executable source changed');
  return { origin, audit, current, addenda: origin.runs.slice(0, 2).map((row, index) => addendum(origin, row, audit.consumed[index])) };
}
function describe(spec, destination) {
  const control = plain(path.resolve(destination));
  if (within(root, control) || within(control, root) || fs.existsSync(control) || fs.existsSync(claimFile())) throw Error('New private control directory and unclaimed segment1 required');
  privateDirectory(control); noParentInstructions(path.dirname(control));
  const evidence = verifyOrigin(spec), { origin, audit } = evidence;
  if (within(origin.directory, control) || within(control, origin.directory)) throw Error('Segment control must be separate from original campaign');
  for (const row of origin.runs.slice(2)) pristine(origin, row);
  return { ...origin, schema: 'cs3-comparison-segment-plan/1', source: evidence.current, control_directory: control,
    segment: { schema: 'cs3-comparison-segment/1', spec, consumed_ids: audit.consumed.map(r => r.id), remaining_ids: audit.remaining_ids, addenda: evidence.addenda,
      policy: 'Verifier-only correction; two immutable failed observations; exactly106 untouched slots, no replay or added budget.' } };
}
function prepare(specFile, destination, dryRun = false) {
  const plan = describe(JSON.parse(read(specFile)), destination), bytes = JSON.stringify(plan, null, 2) + '\n', hash = sha(bytes);
  // Dry validation precedes every durable claim, including the real preparation path.
  campaign().validateExecution(plan);
  if (dryRun) return { status: 'validated_not_claimed', plan_sha256: hash, consumed: 2, remaining: 106, model_calls: 0 };
  write(claimFile(), { control_directory: plan.control_directory, original_plan_sha256: originHash, plan_sha256: hash });
  fs.mkdirSync(plan.control_directory, { mode: 0o700 }); fs.mkdirSync(path.join(plan.control_directory, 'claims')); fs.mkdirSync(path.join(plan.control_directory, 'addenda'));
  for (const row of plan.segment.addenda) write(path.join(plan.control_directory, 'addenda', row.id + '.json'), row);
  write(path.join(plan.control_directory, 'plan.json'), bytes);
  return { plan: path.join(plan.control_directory, 'plan.json'), sha256: hash, consumed: 2, remaining: 106, model_calls: 0 };
}
function validate(plan, hash) {
  const control = plain(plan.control_directory);
  if (plan.schema !== 'cs3-comparison-segment-plan/1' || sha(read(path.join(control, 'plan.json'))) !== hash
    || !equal(JSON.parse(read(claimFile())), { control_directory: control, original_plan_sha256: originHash, plan_sha256: hash })) throw Error('Exact segment ownership required');
  privateDirectory(control); noParentInstructions(control);
  if (within(plan.directory, control) || within(control, plan.directory)) throw Error('Segment control overlaps original campaign');
  const evidence = verifyOrigin(plan.segment.spec), { origin, audit } = evidence;
  const expected = { ...origin, schema: 'cs3-comparison-segment-plan/1', source: evidence.current, control_directory: control,
    segment: { schema: 'cs3-comparison-segment/1', spec: plan.segment.spec, consumed_ids: audit.consumed.map(r => r.id), remaining_ids: audit.remaining_ids, addenda: evidence.addenda,
      policy: 'Verifier-only correction; two immutable failed observations; exactly106 untouched slots, no replay or added budget.' } };
  if (!equal(plan, expected)) throw Error('Segment changed original assignments, policy or verifier addenda');
  for (const row of evidence.addenda) if (!equal(JSON.parse(read(path.join(control, 'addenda', row.id + '.json'))), row)) throw Error('Verifier addendum changed');
  const claims = fs.readdirSync(path.join(control, 'claims'));
  for (const name of claims) {
    const value = JSON.parse(read(path.join(control, 'claims', name))), row = origin.runs.slice(2).find(r => name === r.id + '.json');
    if (row) {
      if (!equal(value, { plan_sha256: hash, id: row.id }) || !claims.includes('block-' + row.skill + '.json')) throw Error('Segment slot ownership differs');
    } else {
      const skill = require('./cs3-comparison-candidates.cjs').ids.find(id => name === 'block-' + id + '.json');
      if (!skill || !equal(value, { plan_sha256: hash, skill })) throw Error('Unknown, duplicated original or changed segment claim');
    }
  }
  for (const row of origin.runs.slice(2)) if (!claims.includes(row.id + '.json')) pristine(origin, row);
  return plan;
}
module.exports = { describe, prepare, validate, verifyOrigin, addendum, pristine, claimFile };
if (require.main === module) {
  try { const [command, spec, destination] = process.argv.slice(2); if (!['prepare', 'dry-run'].includes(command)) throw Error('Usage: dry-run|prepare SPEC NEW_PRIVATE_CONTROL_DIRECTORY');
    process.stdout.write(JSON.stringify(prepare(spec, destination, command === 'dry-run'), null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
