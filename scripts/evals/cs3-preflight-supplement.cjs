// SPDX-License-Identifier: Apache-2.0
'use strict';
// One additional allocation, not an open-ended retry allowance. Each replacement
// is separately claimed by the preflight adapter. Historical unknown costs remain
// unknown and consume their complete original reservation.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const { boundaries } = require('./p6-live-runner.cjs');
const { plain, read, write, within, noParentInstructions, privateDirectory, noSecrets } = boundaries;
const doc = () => require('./cs3-document-remediation.cjs');
const preflight = () => require('./cs3-document-remediation-preflight.cjs');
const root = path.resolve(__dirname, '../..');
const decisionFile = path.join(root, 'src/evals/skills/cs3-runtime-remediation/preflight-supplement.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = file => JSON.parse(read(file));
const boundJson = ref => JSON.parse(doc().bound(ref));
function need(value, reason) { if (!value) throw Error(reason); }
function decision(ref) {
  const approved = boundJson(ref);
  need(equal(approved, json(decisionFile)), 'Source-pinned preflight supplement decision required');
  need(approved.schema === 'cs3-preflight-supplement-decision/1'
    && approved.authority === 'owner_explicit_cs3_completion_within_existing_100_usd'
    && approved.slots === 3 && approved.slot_cap_micros === 600000 && approved.slot_requests === 16
    && approved.additional_cap_micros === 1800000 && approved.additional_requests === 48
    && approved.preserved_cap_micros === 89063737 && approved.preserved_requests === 2373
    && approved.combined_cap_micros === 90863737 && approved.combined_requests === 2421
    && approved.outer_cap_micros === 100000000 && approved.unknown_actual_cost === 'null_not_settled'
    && approved.unknown_debit_micros === 600000 && approved.successful_preflight_stops_replacements === true
    && approved.comparison_replay === false && approved.qualification_or_promotion_waiver === false,
  'Exact bounded supplementary allocation required');
  return approved;
}
function identity(spec) {
  return { executable: spec.executable, build_receipt: spec.build_receipt, catalog: spec.catalog, node: spec.node, profile: spec.profile,
    remediation: Object.fromEntries(['decision', 'prior_terminal', 'runtime_decision', 'allocation', 'qualification'].map(key => [key, spec.remediation?.[key]])) };
}
function lineage(originalFailure, spec, approved) {
  need(originalFailure?.result?.sha256 === approved.original_failure_sha256, 'Exact original failed preflight required');
  const result = boundJson(originalFailure.result);
  need(result.plan?.sha256 === approved.original_plan_sha256, 'Exact original preflight plan required');
  const plan = boundJson(result.plan), old = plan.spec;
  need(plan.source?.content_sha256 === approved.original_source_sha256
    && old?.remediation?.allocation?.sha256 === approved.original_allocation_sha256
    && old?.build_receipt?.sha256 === approved.original_build_receipt_sha256
    && old?.executable?.sha256 === approved.original_executable_sha256, 'Exact historical source/build/allocation lineage required');
  const allocation = boundJson(old.remediation.allocation), build = boundJson(old.build_receipt);
  doc().bound(old.executable, 1024 * 1024 * 1024);
  need(equal(old.remediation.allocation, spec.remediation?.allocation)
    && equal(allocation.spec, { decision: old.remediation.decision, prior_terminal: old.remediation.prior_terminal,
      runtime_decision: old.remediation.runtime_decision, executable: old.executable, build_receipt: old.build_receipt })
    && allocation.schema === 'cs3-document-remediation-allocation/2' && allocation.cap_micros === 22450000
    && allocation.request_ceiling === 594 && allocation.combined_cap_micros === approved.preserved_cap_micros
    && allocation.model_calls === 0 && plain(path.dirname(old.remediation.allocation.path)) === allocation.directory
    && equal(json(doc().allocationClaim()), { allocation: old.remediation.allocation }), 'Original exclusive allocation must remain unchanged');
  need(build.schema === 'cs3-document-remediation-build/1' && build.status === 'passed' && build.exit_code === 0
    && build.executable === old.executable.path && build.executable_sha256 === old.executable.sha256
    && build.qualification_build === true && build.production_release === false && build.provider_calls === 0
    && equal(boundJson(build.source_manifest), build.source_inputs), 'Original genuine build evidence changed');
  for (const key of ['catalog', 'node', 'profile']) need(equal(old[key], spec[key]), `Historical ${key} identity changed`);
  for (const key of ['decision', 'prior_terminal', 'runtime_decision', 'qualification'])
    need(equal(old.remediation[key], spec.remediation?.[key]), `Historical ${key} lineage changed`);
  need(old.executable.sha256 === spec.executable?.sha256, 'Same native executable bytes required');
  const failed = preflight().validateFailedPredecessor(originalFailure, spec, { ordinal: 0, predecessors: [] });
  need(failed.status === 'conservative_failed_preflight_preserved' && failed.conservative_debit_micros === 600000
    && failed.reserved_requests === 16 && failed.actual_cost_micros === null, 'Authentic failed original preflight required');
  return { original_failure: originalFailure, original_failure_sha256: approved.original_failure_sha256,
    original_plan_sha256: approved.original_plan_sha256, original_allocation: old.remediation.allocation,
    original_build_receipt: old.build_receipt, original_executable: old.executable, original_observation: failed };
}
function claimFile() { return path.join(path.dirname(doc().allocationClaim()), 'vcp-cs3-preflight-supplement1.json'); }
function projection(approved, historical) {
  return { slots: approved.slots, slot_cap_micros: approved.slot_cap_micros, slot_requests: approved.slot_requests,
    additional_cap_micros: approved.additional_cap_micros, additional_requests: approved.additional_requests,
    combined_cap_micros: approved.combined_cap_micros, combined_requests: approved.combined_requests, ...historical };
}
function reserve(specFile, destination) {
  const input = json(specFile); noSecrets(input);
  need(equal(Object.keys(input).sort(), ['decision', 'original_failure', 'spec']) && equal(input.spec, identity(input.spec)), 'Exact supplementary reservation inputs required');
  const approved = decision(input.decision), historical = lineage(input.original_failure, input.spec, approved);
  // No recursive validateAllocation call: the original lineage was checked above,
  // and the current build must independently bind every current source byte.
  const currentDecision = doc().decision(input.spec.remediation.decision);
  doc().priorTerminal(input.spec.remediation.prior_terminal, currentDecision);
  doc().build(input.spec, currentDecision);
  const directory = plain(path.resolve(destination));
  need(!within(root, directory) && !within(directory, root) && !fs.existsSync(directory) && !fs.existsSync(claimFile()), 'New private one-shot supplement required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const receipt = { schema: 'cs3-preflight-supplement/1', directory, decision: input.decision, current: identity(input.spec),
    ...projection(approved, historical), model_calls: 0 };
  const ref = { path: path.join(directory, 'allocation.json'), sha256: sha(JSON.stringify(receipt, null, 2) + '\n') };
  // Claim first; a partial publication cannot be retried as a second allocation.
  write(claimFile(), { schema: 'cs3-preflight-supplement-claim/1', allocation: ref });
  fs.mkdirSync(directory, { mode: 0o700 }); write(ref.path, receipt); return ref;
}
function validate(ref, spec) {
  if (spec === undefined) { spec = ref; ref = spec.remediation?.preflight_supplement; }
  const receipt = boundJson(ref), approved = decision(receipt.decision);
  need(equal(ref, spec.remediation?.preflight_supplement) && receipt.schema === 'cs3-preflight-supplement/1'
    && receipt.model_calls === 0 && equal(receipt.current, identity(spec))
    && plain(path.dirname(ref.path)) === receipt.directory
    && equal(json(claimFile()), { schema: 'cs3-preflight-supplement-claim/1', allocation: ref }), 'Exact current supplementary allocation and exclusive claim required');
  const historical = lineage(receipt.original_failure, spec, approved), observed = projection(approved, historical);
  need(equal(receipt, { schema: 'cs3-preflight-supplement/1', directory: receipt.directory, decision: receipt.decision,
    current: identity(spec), ...observed, model_calls: 0 }), 'Supplementary allocation accounting or history differs');
  // The caller's ordinary admission still invokes doc.build(spec, decision).
  // This helper cannot waive that check or recursively invoke validateAllocation.
  return observed;
}
module.exports = { reserve, validate, decision, identity, claimFile };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); need(command === 'reserve', 'Usage: reserve SPEC NEW_PRIVATE_DIRECTORY');
    process.stdout.write(JSON.stringify(reserve(...args), null, 2) + '\n');
  } catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
