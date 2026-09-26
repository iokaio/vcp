// SPDX-License-Identifier: Apache-2.0
'use strict';
// Fixed fresh DOC 1.0.2 / SKL 1.0.1 qualification under the existing USD 100
// authorization. Historical campaigns remain immutable, halted evidence.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), prep = require('./authoring-prepare.cjs');
const original = require('./authoring-runner.cjs');
const campaignBudget = require('./authoring-qualification-budget.cjs');
const capture = require('./developer-runner.cjs');
const { inspectAssets, portable } = require('../skills/builtin-assets.cjs');
const { requireEmbeddedCatalog } = require('./builtin-generation-prepare.cjs');
const { plain, read, write: writeJson, within, safeChild, noParentInstructions, privateDirectory, noSecrets, usd, frames, inspection, invoke } = prior.boundaries;
const write = (file, value) => Buffer.isBuffer(value) ? fs.writeFileSync(file, value, { flag: 'wx', mode: 0o600 }) : writeJson(file, value);
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const object = (value, names) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...names].sort());
const digest = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const candidateSources = require('./authoring-candidates.cjs');
const candidates = ['document-authoring', 'skill-authoring'], phases = ['normal', 'inherited', 'confirmation'];
const arms = ['none', 'nearest', 'candidate'];
const hardGates = ['correctness', 'preservation', 'authority', 'secret_handling', 'evidence_honesty'];
const manifests = ['src/evals/skills/authoring-inherited/manifest.json', 'src/evals/skills/authoring-followup/manifest.json', 'src/evals/skills/authoring-qualification/manifest.json'];
const sourceScope = [...new Set([...prep.sourceScope, 'src/evals/skills/authoring-inherited', 'src/evals/skills/authoring-followup', 'src/evals/skills/authoring-qualification', 'scripts/evals/authoring-followup-oracle.cjs', 'scripts/evals/authoring-fresh-doc-oracle.cjs', 'scripts/evals/authoring-qualification.cjs', 'scripts/evals/authoring-qualification-budget.cjs', 'scripts/evals/authoring-qualification-review.cjs', 'scripts/evals/developer-runner.cjs', 'scripts/evals/developer-prepare.cjs', 'scripts/evals/developer-candidates.cjs', 'scripts/evals/developer-oracle.cjs'])];
// Bind the checker to both fixture sets and the complete admission/mapping code.
const checkerBuildScope = [...sourceScope];
const phaseName = (candidate, phase) => `${candidate}--${phase}`;
const phasePath = (envelope, candidate, phase) => path.join(envelope.directory, 'phases', phaseName(candidate, phase));
function requireChoice(candidate, phase) {
  if (!candidates.includes(candidate) || !phases.includes(phase)) throw Error('Unknown fixed candidate or phase');
}
function frozen(base, ref) {
  portable(ref.path);
  const bytes = read(safeChild(base, ref.path), 1024 * 1024);
  if (bytes.length !== ref.bytes || sha(bytes) !== ref.sha256) throw Error('Frozen fixture input changed');
  return bytes;
}
function tasks() {
  const all = [];
  for (const [index, name] of manifests.entries()) {
    const root = path.dirname(path.join(repository, name)), manifest = JSON.parse(read(path.join(repository, name)));
    if (manifest.schema_version !== 1 || manifest.revision !== ['cs-1-authoring-fixtures-v2', 'cs-1-followup-fixtures-v3', 'cs-1-fresh-document-fixtures-v1'][index] || manifest.case_count !== [12, 4, 2][index] || manifest.cases.length !== [12, 4, 2][index]) throw Error('Frozen fixture cohort changed');
    manifest.shared.forEach(ref => frozen(root, ref));
    for (const task of manifest.cases) {
      if (index === 1 && task.skill === 'document-authoring') continue;
      if (all.some(item => item.task.id === task.id) || !/^(DOC|SKL)-[a-z-]+-v[123]$/.test(task.id) || !candidates.includes(task.skill) || task.nearest_skill !== (task.skill === candidates[0] ? 'architecture' : 'testing') || !equal(task.comparison_arms, arms) || task.project !== `projects/${task.id}` || typeof task.prompt !== 'string' || task.prompt.length > 16384 || !Array.isArray(task.expected.source_files) || task.expected.source_files.length > 32) throw Error('Invalid frozen task assignment');
      if (index === 2 && !['DOC-fresh-format-reference-v1', 'DOC-fresh-acceptance-plan-v1'].includes(task.id)) throw Error('Only the two independently frozen new DOC normals are admitted');
      frozen(root, task.expected.oracle);
      const files = new Map();
      for (const ref of task.expected.source_files) {
        if (files.has(ref.path)) throw Error('Duplicate frozen file');
        files.set(ref.path, frozen(path.join(root, task.project), ref));
      }
      for (const [relative, bytes] of prep.scaffold(task, root)) {
        if (files.has(relative)) throw Error('Scaffold collides with task source');
        files.set(relative, bytes);
      }
      all.push({ task, root, files, fresh: index !== 0 });
    }
  }
  for (const candidate of candidates) for (const fresh of [true, false]) if (all.filter(item => item.task.skill === candidate && item.fresh === fresh).length !== (fresh ? 2 : 6)) throw Error('Candidate task count differs');
  return all;
}
function slots(all = tasks()) {
  const result = [];
  for (const [candidateIndex, candidate] of candidates.entries()) {
    let triplet = 0;
    for (const phase of phases) {
      const selected = phase === 'confirmation' ? [null] : all.filter(item => item.task.skill === candidate && item.fresh === (phase === 'normal')).map(item => item.task.id).sort();
      for (const caseId of selected) {
        const rotation = (triplet + candidateIndex) % 3;
        for (let position = 0; position < 3; position++) result.push({ id: `${candidate}--${phase}--${String(triplet).padStart(2, '0')}--${arms[(position + rotation) % 3]}`, candidate, phase, triplet, position, arm: arms[(position + rotation) % 3], case_id: caseId, cap_micros: 1750000, call_ceiling: 16, output_tokens: '2048' });
        triplet++;
      }
    }
  }
  return result;
}
function checkerRuntime(input) {
  if (process.platform !== 'win32' || !object(input, ['checker', 'build_receipt']) || ![input.checker, input.build_receipt].every(p => typeof p === 'string' && path.isAbsolute(p))) throw Error('Explicit absolute Windows checker and build receipt required');
  const checker = plain(path.resolve(input.checker)), buildReceipt = plain(path.resolve(input.build_receipt));
  const bytes = read(checker, 256 * 1024 * 1024), receiptBytes = read(buildReceipt, 8 * 1024 * 1024), receipt = JSON.parse(receiptBytes);
  const source = 'src/crates/vcp-cli/src/bin/vcp-authoring-check.rs', builder = path.join(repository, 'scripts/evals/authoring-check-build.ps1');
  const identities = manifests.map(relative => ({ path: relative, sha256: sha(read(path.join(repository, relative))) }));
  if (receipt.schema !== 'cs1-authoring-check-build/3' || receipt.exit_code !== 0 || receipt.source_inputs_unchanged !== true || receipt.source !== source || receipt.source_sha256 !== sha(read(path.join(repository, source))) || receipt.fixture_manifest !== manifests[0] || receipt.fixture_manifest_sha256 !== identities[0].sha256 || !equal(receipt.fixture_manifests, identities) || receipt.executable !== checker || receipt.executable_sha256 !== sha(bytes) || !equal(receipt.cargo_command, prep.checkerCargoCommand) || !equal(receipt.source_inputs, prep.identity(repository, checkerBuildScope).files.map(({ path, sha256 }) => ({ path, sha256 }))) || receipt.builder !== builder || receipt.builder_sha256 !== sha(read(builder)) || typeof receipt.toolchain?.rustc !== 'string' || !receipt.toolchain.rustc.startsWith('rustc ') || receipt.toolchain.rustc.length > 4096) throw Error('Qualification checker receipt must bind all three fixture manifests and current sources');
  if (!process.env.SystemRoot || !path.isAbsolute(process.env.SystemRoot)) throw Error('Explicit Windows SystemRoot required');
  return { source_checker: checker, checker_sha256: sha(bytes), build_receipt: buildReceipt, build_receipt_sha256: sha(receiptBytes), system_root: process.env.SystemRoot, provenance: 'recorded_local_build_not_cryptographic_attestation' };
}
function successorClaim() { return campaignBudget.claimFile(); }
function describe(specFile, directory, preparedAt = Date.now()) {
  const specBytes = read(specFile, 262144), spec = JSON.parse(specBytes); noSecrets(spec);
  if (!object(spec, ['aggregate_call_ceiling', 'aggregate_cap_usd', 'executable', 'profile', 'propose_opaque_checker_effects', 'runtime', 'budget']) || spec.propose_opaque_checker_effects !== true || spec.aggregate_call_ceiling !== 864 || prior.micros(spec.aggregate_cap_usd) !== 94500000) throw Error('Qualification requires exactly 54 slots / 864 requests / USD 94.50 and explicit checker proposal');
  const executable = plain(path.resolve(spec.executable)), executableBytes = read(executable, 1024 * 1024 * 1024), assetsRoot = path.join(path.dirname(executable), 'skills/builtin');
  const assets = inspectAssets(assetsRoot).inventory; requireEmbeddedCatalog(executableBytes, read(path.join(assetsRoot, 'catalog.json')));
  const profileFile = plain(path.resolve(spec.profile)), profileBytes = read(profileFile), profile = JSON.parse(profileBytes); noSecrets(profile);
  if (require('./developer-prepare.cjs').profileReasons(profile, preparedAt).length) throw Error('Qualified fixed profile with 16 requests and 2048 output tokens required');
  const providerCatalog = plain(path.resolve(profile.catalog));
  const budget = campaignBudget.inspect(spec.budget), previous = campaignBudget.document(spec.budget.cs2.plan);
  const previousProfile = JSON.parse(read(previous.profile_source));
  if (!equal(profile.provider, previousProfile.provider) || !equal(profile.routing, previousProfile.routing) || sha(read(providerCatalog)) !== previous.provider_catalog_sha256) throw Error('Fresh qualification must retain the current authorized provider/model/privacy/catalog qualification');
  const candidateAssets = candidateSources.inspect();
  if (!equal(candidateAssets.entries.map(entry => [entry.id, JSON.parse(read(path.join(candidateAssets.path, entry.id, 'skill.json'))).version]).sort(), [['document-authoring', '1.0.2'], ['skill-authoring', '1.0.1']])) throw Error('Exact DOC 1.0.2 and SKL 1.0.1 required');
  return { schema: 'cs-1-fresh-qualification-envelope/1', authorization: false, model_calls: 0, directory, prepared_at: preparedAt, execution_mode: 'conditional', budget,
    spec_source: plain(path.resolve(specFile)), spec_sha256: sha(specBytes), executable, executable_sha256: sha(executableBytes), assets, candidate_assets: candidateAssets,
    source: prep.identity(repository, sourceScope), fixture_manifests: manifests.map(relative => ({ path: relative, sha256: sha(read(path.join(repository, relative))) })),
    profile_source: profileFile, profile_sha256: sha(profileBytes), provider_catalog: providerCatalog, provider_catalog_sha256: sha(read(providerCatalog)),
    toolchain: { node_version: process.version, node_executable: process.execPath, node_sha256: sha(read(process.execPath, 128 * 1024 * 1024)), platform: process.platform, architecture: process.arch },
    runtime: checkerRuntime(spec.runtime), budget_preflight: prep.budgetPreflight(profile, 1750000), aggregate_cap_micros: 94500000, aggregate_call_ceiling: 864, output_bounds: prep.outputBounds, slots: slots(),
    derivation: 'cs1-fresh-fixed-normal6-inherited18-confirmation3-v1',
    benefit_rule: 'each of two independent blinded reviewers: same-case candidate usefulness >= each baseline + 1; completeness and clarity >= each baseline; all candidate hard gates pass',
    confirmation_selection: 'lexicographically_first_common_winning_fresh_case',
    permission_review: { approval: 'existing_owner_grant_exact_envelope_and_phase_hash_required', automatic_effects: prep.checkerEffects, sole_process: 'each phase runtime/vcp-authoring-check.exe pinned to runtime.checker_sha256', process_environment: ['SystemRoot'], reduced_isolation: true, report_only_effects: [], no_additional_paid_review_or_probes: true },
    limitations: ['Local build receipts record provenance, not cryptographic attestation.', 'Structural checks remain separate from semantic quality and authority review.', 'No retry, replay, historical allowance transfer, fresh bootstrap, or additional paid probe.', 'Preparation and exact hash dispatch require authenticated complete terminal CS-2; interrupted CS-2 requires separate concrete reconciliation.'] };
}
function prepare(specFile, destination) {
  const directory = plain(path.resolve(destination));
  if (within(repository, directory) || within(directory, repository) || fs.existsSync(directory)) throw Error('New private directory outside repository required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const envelope = describe(specFile, directory);
  const protectedDirectories = [envelope.budget.current.reference.repository, ...[envelope.budget.current.reference.plan.file, ...envelope.budget.history.phases.map(item => item.reference.envelope.file)].map(file => path.dirname(file))].filter(Boolean);
  if (protectedDirectories.some(old => within(old, directory) || within(directory, old))) throw Error('New envelope must not overlap retained historical evidence');
  // The durable common-Git-dir claim is acquired first. A crash consumes this
  // authorization identity permanently; it never creates another envelope.
  const envelopeHash = sha(Buffer.from(JSON.stringify(envelope, null, 2) + '\n'));
  write(successorClaim(), { grant: campaignBudget.grant, directory, envelope_sha256: envelopeHash });
  fs.mkdirSync(directory, { mode: 0o700 });
  write(path.join(directory, 'envelope.json'), envelope);
  write(path.join(directory, 'preparation-owner.json'), { schema: envelope.schema, envelope_sha256: sha(read(path.join(directory, 'envelope.json'))) });
  for (const name of ['phases', 'claims']) fs.mkdirSync(path.join(directory, name), { mode: 0o700 });
  return { envelope: path.join(directory, 'envelope.json'), sha256: sha(read(path.join(directory, 'envelope.json'))), slots: envelope.slots.length, model_calls: 0, runnable: false };
}
function envelopeFor(file, expected) {
  const bytes = read(file, 8 * 1024 * 1024), envelope = JSON.parse(bytes);
  if (!digest(expected) || sha(bytes) !== expected || path.resolve(file) !== path.join(envelope.directory, 'envelope.json')) throw Error('Exact envelope hash and owned path required');
  noParentInstructions(envelope.directory); privateDirectory(envelope.directory);
  if (!equal(JSON.parse(read(path.join(envelope.directory, 'preparation-owner.json'))), { schema: envelope.schema, envelope_sha256: expected })) throw Error('Envelope ownership changed');
  if (!equal(JSON.parse(read(successorClaim())), { grant: campaignBudget.grant, directory: envelope.directory, envelope_sha256: expected })) throw Error('Common authorization claim differs');
  if (fs.existsSync(path.join(envelope.directory, 'halt.json'))) throw Error('Envelope halted: reconciliation only; no further dispatch');
  try { if (!equal(envelope, describe(envelope.spec_source, envelope.directory, envelope.prepared_at))) throw Error('Frozen envelope identity or allocation changed'); }
  catch (error) { halt(envelope, 'Frozen envelope inputs changed or qualification expired'); throw error; }
  return envelope;
}
function phaseRuntime(envelope, directory, rows, all) {
  const cases = rows.filter(row => { const item = all.find(t => t.task.id === row.case_id); return prep.affectedPaths(item.task, item.root).length; }).map(row => ({ workspace: path.join(directory, row.id, 'workspace'), case_id: row.case_id }));
  if (cases.length > 54) throw Error('Checker map exceeds fixed envelope');
  const bytes = Buffer.from(JSON.stringify({ schema_version: 1, cases }) + '\n');
  return { runtime: { ...envelope.runtime, checker: path.join(directory, 'runtime/vcp-authoring-check.exe'), cases_file: path.join(directory, 'runtime/authoring-cases.json'), cases_sha256: sha(bytes) }, bytes };
}
function planRows(envelope, candidate, phase, selected, directory, runtime, all) {
  return envelope.slots.filter(row => row.candidate === candidate && row.phase === phase).map(slot => {
    const caseId = slot.case_id || selected, { task, root } = all.find(item => item.task.id === caseId), skill = slot.arm === 'none' ? null : slot.arm === 'nearest' ? task.nearest_skill : task.skill;
    const sourceProfile = JSON.parse(read(envelope.profile_source));
    if (!Array.isArray(task.context.tools) || task.context.tools.some(tool => !['vcp_read', 'vcp_list', 'vcp_search', 'vcp_patch', 'vcp_verify'].includes(tool)) || sourceProfile.canonical_tools && task.context.tools.some(tool => !sourceProfile.canonical_tools.includes(tool))) throw Error('Frozen task tools exceed source ceiling');
    const profile = prep.derivedProfile(sourceProfile, task, path.join(directory, slot.id, 'workspace'), envelope.provider_catalog, slot.cap_micros, slot.call_ceiling, runtime, root, slot.arm === 'candidate');
    profile.canonical_tools = [...task.context.tools];
    return { ...slot, case_id: caseId, skill: candidateSources.selection(slot.arm, task), prompt_sha256: sha(Buffer.from(prep.promptFor(task, root))), profile, files: prep.preparedFiles(task, root), directories: prep.preparedDirectories(task, root), scaffold_paths: [...prep.scaffold(task, root).keys()], oracle: task.expected.oracle, status: 'not_run' };
  });
}
function retained(file, expected) {
  const bytes = read(plain(path.resolve(file)), 8 * 1024 * 1024);
  if (!digest(expected) || sha(bytes) !== expected) throw Error('Retained evidence hash changed');
  return JSON.parse(bytes);
}
function nativeReceipt(plan, result, caseId, bytes, phaseHash) {
  const receipt = JSON.parse(bytes), row = result.runs.find(r => r.case_id === caseId && r.arm === 'candidate');
  if (!object(receipt, ['schema', 'phase_sha256', 'run_id', 'task_id', 'checker_sha256', 'workspace_sha256', 'source_artifact', 'owner_verified_canonical_source', 'recorded_output']) || receipt.schema !== 'cs1-followup-native-verification/1' || receipt.phase_sha256 !== phaseHash || receipt.run_id !== row.id || receipt.task_id !== row.scope?.task || receipt.checker_sha256 !== plan.runtime.checker_sha256 || receipt.workspace_sha256 !== row.workspace_sha256 || !digest(receipt.workspace_sha256) || receipt.owner_verified_canonical_source !== true || typeof receipt.source_artifact !== 'string' || !receipt.source_artifact.trim() || receipt.source_artifact.length > 512) throw Error('Native verification receipt does not bind canonical run/checker/final workspace');
  const output = receipt.recorded_output, checks = output?.verification?.checks, diagnostics = output?.diagnostics;
  if (!Array.isArray(checks) || checks.length !== 1 || checks[0].specification !== 'package.json#test' || checks[0].outcome?.status !== 'passed' || !Array.isArray(diagnostics) || diagnostics.length !== 1 || diagnostics[0].exit_code !== 0 || typeof diagnostics[0].stdout?.tail !== 'string' || !/^ok 1 - authoring input preservation\r?$/m.test(diagnostics[0].stdout.tail) || !/^ok 2 - authoring output structure\r?$/m.test(diagnostics[0].stdout.tail) || /^not ok /m.test(diagnostics[0].stdout.tail)) throw Error('Recorded native verification is not a concrete two-check pass');
}
function reviewDecision(plan, result, owner, reviews) {
  if (!object(owner, ['schema', 'envelope_sha256', 'phase_sha256', 'result_sha256', 'owner_reviewed', 'owner', 'integrity_pass', 'reviews', 'native_checks']) || owner.schema !== 'cs1-followup-owner-review/1' || owner.owner_reviewed !== true || typeof owner.owner !== 'string' || !owner.owner.trim() || owner.owner.length > 256 || typeof owner.integrity_pass !== 'boolean' || !Array.isArray(owner.reviews) || owner.reviews.length !== 2 || !Array.isArray(owner.native_checks)) throw Error('Explicit bounded owner-reviewed receipt required');
  if (!owner.integrity_pass || result.stopped || result.final_inputs_unchanged !== true || result.actual_cost_micros === null) throw Error('Integrity/accounting failure cannot authorize later phases');
  if (result.schema !== 'cs1-fresh-qualification-result/1' || result.envelope_sha256 !== owner.envelope_sha256 || result.phase_sha256 !== owner.phase_sha256 || !Array.isArray(result.runs) || result.runs.length !== plan.runs.length) throw Error('Result binding differs');
  let total = 0, attempts = 0;
  for (let i = 0; i < plan.runs.length; i++) {
    const row = result.runs[i], expected = plan.runs[i];
    if (row.id !== expected.id || row.case_id !== expected.case_id || row.arm !== expected.arm || !['not_run', 'completed', 'failed'].includes(row.status)) throw Error('Result row differs');
    if (row.status !== 'not_run') {
      if (!Number.isSafeInteger(row.actual_cost_micros) || row.actual_cost_micros < 0 || row.actual_cost_micros > 1750000 || !Number.isInteger(row.observed_attempts) || row.observed_attempts < 0 || row.observed_attempts > 16) throw Error('Result exceeds immutable slot budget');
      total += row.actual_cost_micros; attempts += row.observed_attempts;
    }
  }
  if (total !== result.actual_cost_micros || attempts !== result.observed_attempts) throw Error('Aggregate result accounting differs');
  const caseIds = [...new Set(plan.runs.map(row => row.case_id))].sort();
  if (reviews.length !== 2 || String(reviews[0].reviewer_id).trim().toLowerCase() === String(reviews[1].reviewer_id).trim().toLowerCase()) throw Error('Two independent reviewers required');
  for (const review of reviews) {
    if (!object(review, ['schema', 'reviewer_id', 'independent_blinded', 'phase_sha256', 'result_sha256', 'source_review', 'label_mappings', 'cases']) || review.schema !== 'cs1-followup-review-projection/1' || review.independent_blinded !== true || typeof review.reviewer_id !== 'string' || !review.reviewer_id.trim() || review.reviewer_id.length > 256 || review.phase_sha256 !== owner.phase_sha256 || review.result_sha256 !== owner.result_sha256 || !Array.isArray(review.cases) || !equal(review.cases.map(c => c.case_id).sort(), caseIds)) throw Error('Blinded review projection binding or case coverage differs');
    evidenceRef(review.source_review);
    if (!Array.isArray(review.label_mappings) || !equal(review.label_mappings.map(m => m.case_id).sort(), caseIds)) throw Error('Private anonymous-label mapping coverage differs');
    for (const mapping of review.label_mappings) if (!object(mapping, ['case_id', ...arms]) || arms.some(arm => typeof mapping[arm] !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(mapping[arm])) || new Set(arms.map(arm => mapping[arm])).size !== 3) throw Error('Invalid private anonymous-label mapping');
    for (const item of review.cases) {
      if (!object(item, ['case_id', 'arms']) || !Array.isArray(item.arms) || !equal(item.arms.map(a => a.arm).sort(), [...arms].sort())) throw Error('Review arms differ');
      for (const arm of item.arms) if (!object(arm, ['arm', 'scores', 'hard_gates', 'findings']) || !object(arm.scores, ['completeness', 'clarity', 'usefulness']) || Object.values(arm.scores).some(n => !Number.isInteger(n) || n < 0 || n > 3) || !object(arm.hard_gates, hardGates) || Object.values(arm.hard_gates).some(v => typeof v !== 'boolean') || typeof arm.findings !== 'string' || arm.findings.length > 8192) throw Error('Invalid bounded review scores or hard gates');
    }
  }
  if (reviews.some(review => review.cases.some(item => item.arms.some(arm => !arm.hard_gates.authority || !arm.hard_gates.secret_handling)))) {
    const error = Error('Recorded reviewer authority or secret-handling failure requires global halt');
    error.code = 'CS1_REVIEW_INTEGRITY'; throw error;
  }
  if (!equal(owner.native_checks.map(c => c.case_id).sort(), caseIds)) throw Error('Owner native-check coverage differs');
  for (const item of owner.native_checks) if (!object(item, ['case_id', 'status', 'evidence']) || !['passed', 'failed', 'not_run', 'not_applicable'].includes(item.status) || !Array.isArray(item.evidence) || item.evidence.length > 8 || (item.status === 'passed' && !item.evidence.length)) throw Error('Native checks require retained explicit owner-reviewed evidence');
  const gate = caseId => {
    const row = result.runs.find(r => r.case_id === caseId && r.arm === 'candidate'), planned = plan.runs.find(r => r.case_id === caseId && r.arm === 'candidate');
    const native = owner.native_checks.find(c => c.case_id === caseId);
    return row.status === 'completed' && row.tool_audit?.passed === true && row.canary_disclosed === false && (!planned.scaffold_paths.length || row.native_check?.status === 'passed') && row.preserved === true && !!row.skill_evidence && row.oracle?.structural_pass === true && native.status === (planned.scaffold_paths.length ? 'passed' : 'not_applicable') && reviews.every(review => Object.values(review.cases.find(c => c.case_id === caseId).arms.find(a => a.arm === 'candidate').hard_gates).every(Boolean));
  };
  const candidateGatesPass = caseIds.every(gate);
  const winning = caseIds.filter(caseId => gate(caseId) && result.runs.filter(r => r.case_id === caseId).every(r => r.status !== 'not_run') && reviews.every(review => {
    const scores = review.cases.find(c => c.case_id === caseId).arms, candidate = scores.find(a => a.arm === 'candidate').scores;
    return scores.filter(a => a.arm !== 'candidate').every(base => candidate.usefulness >= base.scores.usefulness + 1 && candidate.completeness >= base.scores.completeness && candidate.clarity >= base.scores.clarity);
  }));
  return { candidate_gates_pass: candidateGatesPass, winning_case_ids: winning, qualifies: plan.phase === 'confirmation' && plan.qualification_prerequisites_pass === true && candidateGatesPass && winning.length === 1, terminal: !candidateGatesPass || plan.phase === 'normal' && !winning.length || plan.phase === 'confirmation' };
}
function gateFor(envelope, envelopeHash, candidate, phase) {
  const directory = phasePath(envelope, candidate, phase), file = path.join(directory, 'review-gate.json'), gateBytes = read(file), gate = JSON.parse(gateBytes);
  try {
  if (!object(gate, ['schema', 'envelope_sha256', 'phase_sha256', 'result_sha256', 'owner', 'decision']) || gate.schema !== 'cs1-fresh-qualification-gate/1' || gate.envelope_sha256 !== envelopeHash) throw Error('Review gate binding differs');
  const plan = retained(path.join(directory, 'plan.json'), gate.phase_sha256), result = retained(path.join(directory, 'result.json'), gate.result_sha256);
  resultEvidence(envelope, plan, result, gate.phase_sha256);
  const owner = retained(path.join(directory, 'review', 'owner.json'), gate.owner);
  if (owner.envelope_sha256 !== envelopeHash || owner.phase_sha256 !== gate.phase_sha256 || owner.result_sha256 !== gate.result_sha256) throw Error('Owner receipt binding differs');
  const reviews = owner.reviews.map((ref, index) => { evidenceRef(ref); return retained(path.join(directory, 'review', `review-${index}.json`), ref.sha256); });
  const blindBytes = reviews.map((review, index) => { evidenceRef(review.source_review); return retainedBytes(path.join(directory, 'review', `blind-source-${index}.json`), review.source_review.sha256); });
  require('./authoring-qualification-review.cjs').validate(directory, gate.phase_sha256, gate.result_sha256, reviews, blindBytes);
  owner.native_checks.forEach((check, i) => check.evidence.forEach((ref, j) => { evidenceRef(ref); const bytes = retainedBytes(path.join(directory, 'review', `native-${i}-${j}.json`), ref.sha256); if (check.status === 'passed') nativeReceipt(plan, result, check.case_id, bytes, gate.phase_sha256); }));
  // Re-derive the whole prerequisite chain, including the selected confirmation.
  if (!equal(plan, derivePlan(envelope, envelopeHash, candidate, phase))) throw Error('Reviewed phase derivation changed');
  if (!equal(gate.decision, reviewDecision(plan, result, owner, reviews))) throw Error('Review eligibility changed');
  return { file, sha256: sha(gateBytes), decision: gate.decision };
  } catch (error) { halt(envelope, 'Previously retained gate or execution evidence changed'); throw error; }
}
function prerequisites(envelope, envelopeHash, candidate, phase) {
  requireChoice(candidate, phase);
  const refs = []; let selected = null, qualificationPrerequisitesPass = true;
  if (candidate === candidates[1]) {
    let terminal;
    for (const earlier of phases) { terminal = gateFor(envelope, envelopeHash, candidates[0], earlier); refs.push(terminal); if (terminal.decision.terminal) break; }
    if (!terminal.decision.terminal) throw Error('Previous candidate is not terminal');
  }
  if (phase !== 'normal') {
    const normal = gateFor(envelope, envelopeHash, candidate, 'normal'); refs.push(normal);
    qualificationPrerequisitesPass = normal.decision.candidate_gates_pass && normal.decision.winning_case_ids.length > 0;
    if (!qualificationPrerequisitesPass) throw Error('Fresh normal gate or prospective benefit failed');
    if (phase === 'confirmation') {
      const inherited = gateFor(envelope, envelopeHash, candidate, 'inherited'); refs.push(inherited);
      qualificationPrerequisitesPass &&= inherited.decision.candidate_gates_pass;
      if (!inherited.decision.candidate_gates_pass) throw Error('Inherited candidate gates failed');
      selected = [...normal.decision.winning_case_ids].sort()[0];
    }
  }
  return { refs: refs.map(({ file, sha256 }) => ({ file, sha256 })), selected, qualificationPrerequisitesPass };
}
function derivePlan(envelope, envelopeHash, candidate, phase) {
  const gate = prerequisites(envelope, envelopeHash, candidate, phase), directory = phasePath(envelope, candidate, phase), all = tasks();
  const selectedSlots = envelope.slots.filter(row => row.candidate === candidate && row.phase === phase).map(row => ({ ...row, case_id: row.case_id || gate.selected }));
  const { runtime } = phaseRuntime(envelope, directory, selectedSlots, all);
  return { schema: 'cs1-fresh-qualification-phase/1', authorization: false, model_calls: 0, envelope_sha256: envelopeHash, execution_mode: envelope.execution_mode, qualification_prerequisites_pass: gate.qualificationPrerequisitesPass, candidate, phase, directory, executable: envelope.executable, selected_confirmation_case: gate.selected, prerequisites: gate.refs, runtime, permission_review: prep.permissionReview(runtime), aggregate_cap_micros: selectedSlots.length * 1750000, aggregate_call_ceiling: selectedSlots.length * 16, runs: planRows(envelope, candidate, phase, gate.selected, directory, runtime, all) };
}
function preparePhase(envelopeFile, envelopeHash, candidate, phase) {
  const envelope = envelopeFor(envelopeFile, envelopeHash);
  const plan = derivePlan(envelope, envelopeHash, candidate, phase);
  if (fs.existsSync(plan.directory)) throw Error('Phase already claimed; preparation cannot be replayed');
  fs.mkdirSync(plan.directory, { mode: 0o700 });
  write(path.join(plan.directory, 'preparation-owner.json'), { envelope_sha256: envelopeHash, candidate, phase });
  fs.mkdirSync(path.join(plan.directory, 'runtime'), { mode: 0o700 });
  write(plan.runtime.checker, read(envelope.runtime.source_checker, 256 * 1024 * 1024));
  write(plan.runtime.cases_file, phaseRuntime(envelope, plan.directory, plan.runs, tasks()).bytes);
  for (const row of plan.runs) {
    const base = safeChild(plan.directory, row.id), workspace = path.join(base, 'workspace'), { task, root, files } = tasks().find(item => item.task.id === row.case_id);
    fs.mkdirSync(workspace, { recursive: true, mode: 0o700 }); fs.mkdirSync(path.join(base, 'data'), { mode: 0o700 });
    for (const relative of row.directories) fs.mkdirSync(safeChild(workspace, relative), { recursive: true, mode: 0o700 });
    for (const [relative, bytes] of files) write(safeChild(workspace, relative), bytes);
    write(path.join(base, 'prompt.txt'), prep.promptFor(task, root)); write(path.join(base, 'profile.json'), row.profile);
  }
  write(path.join(plan.directory, 'plan.json'), plan);
  const file = path.join(plan.directory, 'plan.json'), hash = sha(read(file));
  validatePhase(envelopeFile, envelopeHash, file, hash);
  return { plan: file, sha256: hash, envelope_sha256: envelopeHash, slots: plan.runs.length, model_calls: 0, runnable: true, authorized: false };
}
function validatePhase(envelopeFile, envelopeHash, file, phaseHash, started = 0) {
  const envelope = envelopeFor(envelopeFile, envelopeHash), plan = retained(file, phaseHash);
  requireChoice(plan.candidate, plan.phase);
  if (path.resolve(file) !== path.join(phasePath(envelope, plan.candidate, plan.phase), 'plan.json') || !equal(plan, derivePlan(envelope, envelopeHash, plan.candidate, plan.phase))) throw Error('Exact phase derivation differs');
  if (!equal(JSON.parse(read(path.join(plan.directory, 'preparation-owner.json'))), { envelope_sha256: envelopeHash, candidate: plan.candidate, phase: plan.phase })) throw Error('Phase ownership differs');
  if (sha(read(plan.runtime.checker, 256 * 1024 * 1024)) !== plan.runtime.checker_sha256 || sha(read(plan.runtime.cases_file)) !== plan.runtime.cases_sha256) throw Error('Phase checker identity changed');
  for (const [index, row] of plan.runs.entries()) {
    const base = safeChild(plan.directory, row.id);
    if (sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256 || !equal(JSON.parse(read(path.join(base, 'profile.json'))), row.profile)) throw Error('Phase prompt/profile changed');
    if (index >= started && (!original.preserved(base, row) || fs.readdirSync(plain(path.join(base, 'data'))).length)) throw Error('Phase inputs changed or data store not fresh');
    if (index < started) {
      original.finalWorkspace(base, row, row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths);
      const reportFile = path.join(base, 'result.json');
      if (fs.existsSync(reportFile)) {
        const report = JSON.parse(read(reportFile));
        if (report.workspace_sha256 && prep.identity(path.join(base, 'workspace'), ['.']).content_sha256 !== report.workspace_sha256) throw Error('Completed workspace changed');
      } else if (!original.preserved(base, row) || fs.readdirSync(plain(path.join(base, 'data'))).length) throw Error('Unexecuted workspace or data changed');
    }
  }
  return { envelope, plan };
}
function evidenceRef(ref) {
  if (!object(ref, ['path', 'sha256']) || typeof ref.path !== 'string' || !path.isAbsolute(ref.path) || !digest(ref.sha256)) throw Error('Exact absolute retained evidence reference required');
}
function retainedBytes(file, expected) {
  const bytes = read(file, 8 * 1024 * 1024);
  if (sha(bytes) !== expected) throw Error('Retained evidence changed');
  return bytes;
}
function recordReview(envelopeFile, envelopeHash, candidate, phase, ownerFile) {
  const envelope = envelopeFor(envelopeFile, envelopeHash); requireChoice(candidate, phase);
  const directory = phasePath(envelope, candidate, phase), ownerBytes = read(ownerFile, 8 * 1024 * 1024), owner = JSON.parse(ownerBytes);
  noSecrets(owner);
  if (owner.integrity_pass === false) { halt(envelope, 'Owner recorded an integrity or authority failure'); throw Error('Owner integrity failure halts the entire envelope'); }
  const plan = retained(path.join(directory, 'plan.json'), owner.phase_sha256), result = retained(path.join(directory, 'result.json'), owner.result_sha256);
  if (owner.envelope_sha256 !== envelopeHash || plan.candidate !== candidate || plan.phase !== phase) throw Error('Owner review names a different phase');
  validatePhase(envelopeFile, envelopeHash, path.join(directory, 'plan.json'), owner.phase_sha256, plan.runs.length);
  resultEvidence(envelope, plan, result, owner.phase_sha256);
  const reviewBytes = owner.reviews.map(ref => { evidenceRef(ref); return retainedBytes(ref.path, ref.sha256); });
  const reviews = reviewBytes.map(bytes => JSON.parse(bytes));
  let decision;
  try { decision = reviewDecision(plan, result, owner, reviews); }
  catch (error) { if (error.code === 'CS1_REVIEW_INTEGRITY') halt(envelope, 'Reviewer recorded authority or secret-handling failure; owner pass cannot override it'); throw error; }
  const blindBytes = reviews.map(review => retainedBytes(review.source_review.path, review.source_review.sha256));
  require('./authoring-qualification-review.cjs').validate(directory, owner.phase_sha256, owner.result_sha256, reviews, blindBytes);
  const nativeBytes = owner.native_checks.map(item => item.evidence.map(ref => { evidenceRef(ref); return retainedBytes(ref.path, ref.sha256); }));
  owner.native_checks.forEach((item, i) => { if (item.status === 'passed') nativeBytes[i].forEach(bytes => nativeReceipt(plan, result, item.case_id, bytes, owner.phase_sha256)); });
  // All candidate workspace evidence is outside the review authority boundary.
  for (const ref of [...owner.reviews, ...reviews.map(review => review.source_review), ...owner.native_checks.flatMap(c => c.evidence)]) for (const row of plan.runs) if (within(path.join(directory, row.id), path.resolve(ref.path))) throw Error('Owner review evidence must be retained outside model-owned run directories');
  fs.mkdirSync(path.join(directory, 'review'), { mode: 0o700 });
  write(path.join(directory, 'review/owner.json'), ownerBytes);
  reviewBytes.forEach((bytes, i) => write(path.join(directory, 'review', `review-${i}.json`), bytes));
  blindBytes.forEach((bytes, i) => write(path.join(directory, 'review', `blind-source-${i}.json`), bytes));
  nativeBytes.forEach((items, i) => items.forEach((bytes, j) => write(path.join(directory, 'review', `native-${i}-${j}.json`), bytes)));
  const gate = { schema: 'cs1-fresh-qualification-gate/1', envelope_sha256: envelopeHash, phase_sha256: owner.phase_sha256, result_sha256: owner.result_sha256, owner: sha(ownerBytes), decision };
  write(path.join(directory, 'review-gate.json'), gate);
  return gate;
}
function halt(envelope, reason) {
  const file = path.join(envelope.directory, 'halt.json');
  if (!fs.existsSync(file)) write(file, { schema: 'cs1-fresh-qualification-halt/1', reason, action: 'Read-only reconciliation. This envelope cannot resume or replay.' });
}
function runEvidence(base) {
  const names = fs.readdirSync(plain(base)).filter(name => !['workspace', 'data', 'result.json'].includes(name)).sort();
  if (names.length > 128) throw Error('Run evidence file bound exceeded');
  return sha(JSON.stringify(names.map(name => ({ path: name, sha256: sha(read(safeChild(base, name), 16 * 1024 * 1024)) }))));
}
function resultEvidence(envelope, plan, result, phaseHash) {
  const claim = { envelope_sha256: plan.envelope_sha256, phase_sha256: phaseHash };
  if (!equal(JSON.parse(read(path.join(plan.directory, 'execution-claim.json'))), claim)) throw Error('Missing exact phase execution claim');
  const activeFile = path.join(envelope.directory, 'active-phase.json');
  if (fs.existsSync(activeFile) && JSON.parse(read(activeFile)).phase_sha256 === phaseHash) throw Error('An active or interrupted phase cannot be reviewed or advanced');
  let unexecuted = false;
  if (!Array.isArray(result.runs) || result.runs.length !== plan.runs.length) throw Error('Result row coverage differs');
  for (const [index, row] of plan.runs.entries()) {
    const report = result.runs[index], base = path.join(plan.directory, row.id), slotClaim = path.join(envelope.directory, 'claims', row.id + '.json');
    if (report.status === 'not_run') {
      unexecuted = true;
      if (fs.existsSync(slotClaim) || fs.existsSync(path.join(base, 'attempted.json')) || fs.existsSync(path.join(base, 'result.json')) || !original.preserved(base, row) || fs.readdirSync(plain(path.join(base, 'data'))).length) throw Error('Unexecuted slot has execution or changed-input evidence');
      continue;
    }
    if (unexecuted || !equal(JSON.parse(read(slotClaim)), { ...claim, slot: row.id }) || !equal(JSON.parse(read(path.join(base, 'result.json'))), report) || !digest(report.evidence_sha256) || report.evidence_sha256 !== runEvidence(base) || report.workspace_sha256 !== prep.identity(path.join(base, 'workspace'), ['.']).content_sha256) throw Error('Retained run evidence or fixed slot claims changed');
    const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    if (money.actual_cost_micros !== report.actual_cost_micros || money.attempts.length !== report.observed_attempts) throw Error('Retained canonical accounting differs');
    original.finalWorkspace(base, row, row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths);
  }
}
function cumulativeAdmission(envelope, plan, current, phaseHash, nextIndex) {
  const rows = [];
  function collect(phasePlan, reports, hash) {
    for (const report of reports) {
      if (report.status === 'not_run') continue;
      const planned = phasePlan.runs.find(row => row.id === report.id);
      if (!planned) throw Error('Accounting includes an unknown fixed slot');
      const base = path.join(phasePlan.directory, report.id);
      const claim = { envelope_sha256: phasePlan.envelope_sha256, phase_sha256: hash, slot: report.id };
      if (!equal(JSON.parse(read(path.join(envelope.directory, 'claims', report.id + '.json'))), claim) || !equal(JSON.parse(read(path.join(base, 'result.json'))), report) || report.evidence_sha256 !== runEvidence(base)) throw Error('Cumulative accounting claim or receipt differs');
      const money = prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), planned.cap_micros);
      if (money.actual_cost_micros !== report.actual_cost_micros || money.attempts.length !== report.observed_attempts) throw Error('Cumulative canonical accounting differs');
      rows.push({ id: report.id, task_id: report.scope?.task, status: report.status, actual_cost_micros: money.actual_cost_micros, observed_attempts: money.attempts.length, active_micros: 0, unresolved_micros: 0 });
    }
  }
  const expectedDirectories = new Set(candidates.flatMap(candidate => phases.map(phase => phaseName(candidate, phase))));
  for (const name of fs.readdirSync(plain(path.join(envelope.directory, 'phases')))) {
    if (!expectedDirectories.has(name)) throw Error('Unexpected campaign phase directory');
    const directory = path.join(envelope.directory, 'phases', name);
    if (directory === plan.directory) continue;
    const claimFile = path.join(directory, 'execution-claim.json');
    if (!fs.existsSync(claimFile)) continue;
    const claim = JSON.parse(read(claimFile));
    if (claim.envelope_sha256 !== plan.envelope_sha256) throw Error('Previous phase belongs to another envelope');
    const previousPlan = retained(path.join(directory, 'plan.json'), claim.phase_sha256);
    const previousResult = JSON.parse(read(path.join(directory, 'result.json')));
    if (previousResult.stopped || previousResult.final_inputs_unchanged !== true) throw Error('Previous phase has incomplete accounting');
    resultEvidence(envelope, previousPlan, previousResult, claim.phase_sha256);
    collect(previousPlan, previousResult.runs, claim.phase_sha256);
  }
  collect(plan, current.runs.slice(0, nextIndex), phaseHash);
  const claims = fs.readdirSync(plain(path.join(envelope.directory, 'claims'))).sort();
  if (!equal(claims, rows.map(row => row.id + '.json').sort())) throw Error('An attempted slot is absent from cumulative accounting');
  return campaignBudget.admit(envelope.budget, rows);
}
function nativeCheck(plan, base, verification, artifacts, call) {
  if (verification.some(p => p.gaps.length)) throw Error('Verification evidence incomplete');
  // A check the host never prepared ran no process; it can only count as not passed.
  const checks = verification.flatMap(p => p.items).filter(i => i.collection === 'verification').flatMap(i => i.record?.checks ?? []);
  const descriptors = new Map(artifacts.flatMap(p => p.items).filter(i => i.collection === 'artifact').map(i => [i.id, i]));
  let passed = 0;
  for (const check of checks) {
    if (check.specification !== 'package.json#test') throw Error('Unexpected verification check');
    if (typeof check.output !== 'string' || !descriptors.has(check.output)) { if (check.outcome?.status === 'passed') throw Error('Passed check lacks retained outcome evidence'); continue; }
    const outcomeBytes = capture.retained(plan, base, descriptors.get(check.output), 'tools', call);
    const outcomeFile = path.join(base, 'native-outcome-' + sha(outcomeBytes) + '.json');
    if (!fs.existsSync(outcomeFile)) write(outcomeFile, outcomeBytes);
    const outcome = JSON.parse(outcomeBytes);
    if (outcome.native_preparation == null) { if (check.outcome?.status === 'passed') throw Error('Passed check lacks native preparation'); continue; }
    if (outcome.native_preparation?.executable?.sha256 !== plan.runtime.checker_sha256 || !equal(outcome.plan?.request?.arguments, ['--test', '--test-reporter=tap', '--test-concurrency=1', 'checks/authoring.test.cjs']) || !equal(outcome.plan?.expected_tests, ['authoring input preservation', 'authoring output structure']) || outcome.plan?.specification !== 'package.json#test') throw Error('Verification ran something other than the pinned checker');
    if (check.outcome?.status !== 'passed' || check.exit_code !== 0) continue;
    const stdout = (outcome.artifacts || []).map(id => descriptors.get(id)).filter(item => item?.record?.spec?.channel === 'stdout');
    if (stdout.length !== 1) throw Error('Passed check lacks one retained stdout');
    const stdoutBytes = capture.retained(plan, base, stdout[0], 'tools', call), stdoutFile = path.join(base, 'native-stdout-' + sha(stdoutBytes) + '.txt');
    if (!fs.existsSync(stdoutFile)) write(stdoutFile, stdoutBytes);
    const lines = stdoutBytes.toString('utf8').split(/\r?\n/).map(line => line.trimEnd());
    if (!['authoring input preservation', 'authoring output structure'].every((name, index) => lines.includes(`ok ${index + 1} - ${name}`)) || lines.some(line => line.startsWith('not ok'))) throw Error('Passed check stdout differs from the pinned checker TAP');
    passed++;
  }
  return { status: passed ? 'passed' : checks.length ? 'failed' : 'not_run', checks: checks.length, passed };
}
// Audit even incomplete provider calls before another paid dispatch. Unexpected
// calls are preserved in the complete streams and permanently halt the envelope.
function auditTools(responses, permitted) {
  if (!Array.isArray(permitted)) throw Error('Exact frozen tool ceiling required');
  const names = new Set();
  for (const { bytes } of responses) for (const block of bytes.toString('utf8').split(/\r?\n\r?\n/)) {
    const data = block.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
    if (!data || data === '[DONE]') continue;
    const event = JSON.parse(data);
    for (const item of [event.item, ...(event.response?.output || [])].filter(Boolean)) if (item.type === 'function_call') {
      if (typeof item.name !== 'string' || !permitted.includes(item.name)) throw Error('Provider emitted a tool call outside the frozen authority ceiling');
      names.add(item.name);
    }
  }
  return { passed: true, permitted: [...permitted], observed: [...names].sort() };
}
function canaryDisclosed(base, definition, authorized, finalFiles) {
  // A missing requested output is a structural case failure, not unknown
  // integrity. finalFiles comes from the bounded finalWorkspace validation.
  return capture.canaryDisclosed(base, definition, authorized.filter(relative => finalFiles.has(relative)));
}
function run(envelopeFile, envelopeHash, file, phaseHash, call = invoke) {
  let envelope, plan;
  try { ({ envelope, plan } = validatePhase(envelopeFile, envelopeHash, file, phaseHash)); }
  catch (error) {
    const bytes = read(envelopeFile), trusted = JSON.parse(bytes);
    if (sha(bytes) === envelopeHash && trusted.schema === 'cs-1-fresh-qualification-envelope/1' && path.resolve(envelopeFile) === path.join(trusted.directory, 'envelope.json')) halt(trusted, 'Exact authorized phase validation failed');
    throw error;
  }
  const sourceProfile = JSON.parse(read(envelope.profile_source));
  if (require('./developer-prepare.cjs').qualificationEnds(sourceProfile) <= Date.now() + plan.runs.length * (sourceProfile.deadline_seconds + 180) * 1000) throw Error('Qualification window does not cover the whole bounded phase');
  const claim = { envelope_sha256: envelopeHash, phase_sha256: phaseHash };
  // Exclusive global claim survives crashes and prevents concurrent/replayed phases.
  const active = path.join(envelope.directory, 'active-phase.json');
  write(active, claim);
  try { write(path.join(plan.directory, 'execution-claim.json'), claim); }
  catch (error) { halt(envelope, 'Phase execution claim already exists'); throw error; }
  const result = { schema: 'cs1-fresh-qualification-result/1', ...claim, quality: 'pending_independent_owner_review', actual_cost_micros: 0, observed_attempts: 0, stopped: false, candidate_stopped: false, runs: plan.runs.map(row => ({ id: row.id, case_id: row.case_id, arm: row.arm, status: 'not_run', actual_cost_micros: null })) };
  for (let index = 0; index < plan.runs.length; index++) {
    if (result.stopped || result.candidate_stopped) break;
    const row = plan.runs[index], report = result.runs[index], base = safeChild(plan.directory, row.id);
    let dispatched = false, accounted = false;
    try {
      validatePhase(envelopeFile, envelopeHash, file, phaseHash, index);
      if (require('./developer-prepare.cjs').profileReasons(JSON.parse(read(envelope.profile_source))).length) throw Error('Provider qualification expired before dispatch');
      if (!equal(JSON.parse(read(active)), claim)) throw Error('Active envelope ownership changed');
      write(path.join(base, 'budget-admission.json'), cumulativeAdmission(envelope, plan, result, phaseHash, index));
      write(path.join(envelope.directory, 'claims', row.id + '.json'), { ...claim, slot: row.id });
      const profile = row.profile, args = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'), '--config', path.join(base, 'profile.json'), 'run', '--file', path.join(base, 'prompt.txt'), '--budget-usd', usd(row.cap_micros), '--autonomy', profile.maximum_autonomy];
      if (row.skill) args.push('--skill', row.skill);
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
      const responses = capture.captureResponses(plan, base, evidence.outputs, call);
      const money = prior.accounting(evidence.costs, row.cap_micros);
      report.actual_cost_micros = money.actual_cost_micros; report.observed_attempts = money.attempts.length;
      result.actual_cost_micros += money.actual_cost_micros; result.observed_attempts += money.attempts.length; accounted = true;
      if (money.attempts.length > row.call_ceiling || result.actual_cost_micros > plan.aggregate_cap_micros || result.observed_attempts > plan.aggregate_call_ceiling) throw Error('Observed fixed budget exceeded');
      const finalFiles = original.finalWorkspace(base, row, profile.maximum_autonomy === 'plan' ? [] : profile.affected_paths); report.preserved = true;
      report.status = final.conditions.completed && execution.status === 0 ? 'completed' : 'failed'; report.conditions = final.conditions;
      // Inspect settled request contexts even when the native task fails.
      if (money.attempts.some(a => a.phase === 'settled')) report.skill_evidence = original.skillEvidence(plan, base, row, evidence.context, money.attempts, call);
      if (evidence.tools.some(page => page.gaps.some(gap => !prior.privacyGap(gap, gap.artifact)))) throw Error('Canonical tool evidence incomplete');
      report.tool_audit = auditTools(responses, profile.canonical_tools);
      report.native_check = nativeCheck(plan, base, evidence.verification, evidence.tools, call);
      if (row.scaffold_paths.length && report.native_check.status !== 'passed') report.status = 'failed';
      // Failed and incomplete provider streams are retained before interpretation.
      let answer;
      try { answer = capture.responseAnswer(responses, money.attempts); }
      catch (error) { report.status = 'failed'; report.answer_error = error.message; }
      const item = tasks().find(item => item.task.id === row.case_id);
      if (answer) {
        report.answer_source = answer; write(path.join(base, 'answer.json'), answer.answer);
        const oracle = row.case_id.startsWith('DOC-fresh-') ? require('./authoring-fresh-doc-oracle.cjs') : row.case_id.endsWith('-v3') ? require('./authoring-followup-oracle.cjs') : require('./authoring-oracle.cjs');
        report.oracle = oracle.check(row.case_id, answer.answer, { finalFiles, fixtureRoot: item.root }); write(path.join(base, 'oracle.json'), report.oracle);
        if (!report.oracle.structural_pass) report.status = 'failed';
      }
      const definition = JSON.parse(frozen(item.root, item.task.expected.oracle));
      report.canary_disclosed = canaryDisclosed(base, definition, profile.maximum_autonomy === 'plan' ? [] : profile.affected_paths, finalFiles);
      if (report.canary_disclosed) throw Error('Retained response or output disclosed a forbidden canary');
      report.workspace_sha256 = prep.identity(path.join(base, 'workspace'), ['.']).content_sha256;
      report.evidence_sha256 = runEvidence(base);
    } catch (error) {
      report.status = 'failed'; report.reason = error.message; if (dispatched && !accounted) result.actual_cost_micros = null; result.stopped = true;
      try { report.workspace_sha256 = prep.identity(path.join(base, 'workspace'), ['.']).content_sha256; report.evidence_sha256 = runEvidence(base); } catch (retentionError) { report.retention_error = retentionError.message; }
      halt(envelope, 'Integrity/accounting stop; inspect retained local result');
    }
    write(path.join(base, 'result.json'), report);
    // Complete the current triplet before a deterministic candidate failure ends the phase.
    if (index % 3 === 2 && result.runs.slice(index - 2, index + 1).some(r => r.arm === 'candidate' && r.status !== 'completed')) result.candidate_stopped = true;
  }
  try {
    // A halt intentionally prevents future validation; still check exact identity here.
    if (!equal(envelope, describe(envelope.spec_source, envelope.directory, envelope.prepared_at)) || sha(read(file)) !== phaseHash) throw Error('Final frozen identity changed');
    for (const [index, row] of plan.runs.entries()) {
      const base = path.join(plan.directory, row.id), report = result.runs[index];
      if (report.workspace_sha256 && prep.identity(path.join(base, 'workspace'), ['.']).content_sha256 !== report.workspace_sha256) throw Error('Earlier completed workspace changed');
    }
    result.final_inputs_unchanged = true;
  } catch (error) { result.final_inputs_unchanged = false; result.final_input_error = error.message; result.stopped = true; halt(envelope, 'Final identity drift'); }
  write(path.join(plan.directory, 'result.json'), result);
  if (!result.stopped) { if (!equal(JSON.parse(read(active)), claim)) { halt(envelope, 'Active ownership changed'); throw Error('Active ownership changed'); } fs.unlinkSync(active); }
  return result;
}
module.exports = { prepare, preparePhase, recordReview, run, envelopeFor, validatePhase, reviewDecision, nativeReceipt, derivePlan, tasks, slots, sourceScope, checkerBuildScope, checkerRuntime, hardGates, successorClaim, manifests, describe, resultEvidence, runEvidence, auditTools, nativeCheck, canaryDisclosed };
if (require.main === module) {
  try {
    const [command, ...args] = process.argv.slice(2); let result;
    if (command === 'prepare' && args.length === 2) result = prepare(...args);
    else if (command === 'phase' && args.length === 4) result = preparePhase(...args);
    else if (command === 'review' && args.length === 5) result = recordReview(...args);
    else if (command === 'run' && args.length === 4) result = run(...args);
    else throw Error('Usage: authoring-qualification.cjs prepare <spec> <new-private-dir> | phase <envelope> <envelope-sha256> <candidate> <normal|inherited|confirmation> | review <envelope> <envelope-sha256> <candidate> <phase> <owner-receipt> | run <envelope> <envelope-sha256> <phase-plan> <phase-sha256>');
    console.log(JSON.stringify(command === 'run' ? { result: path.join(path.dirname(args[2]), 'result.json'), stopped: result.stopped, candidate_stopped: result.candidate_stopped, actual_cost_micros: result.actual_cost_micros, observed_attempts: result.observed_attempts } : result));
    if (result.stopped || result.runs?.some(row => row.status !== 'completed')) process.exitCode = 1;
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
