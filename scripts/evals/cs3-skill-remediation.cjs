// SPDX-License-Identifier: Apache-2.0
'use strict';
// One fixed, fresh SKL experiment. No historical allocation/ledger is rewritten.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const doc = require('./cs3-document-remediation.cjs'), prep = require('./authoring-prepare.cjs');
const { portable } = require('../skills/builtin-assets.cjs');
const { plain, read, write, within, safeChild, noParentInstructions, privateDirectory, noSecrets } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../..'), fixture = path.join(root, 'src/evals/skills/cs3-skill-remediation');
const core = () => require('./cs3-comparison.cjs'), recovery = () => require('./cs3-controller-recovery-qualification.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex'), json = ref => JSON.parse(doc.bound(ref));
const releasedIds = ['SKL-cs3-missing-prerequisite-v1--candidate', 'SKL-cs3-missing-prerequisite-v1--none', 'SKL-cs3-missing-prerequisite-v1--nearest',
  'SKL-cs3-near-miss-brief-v1--none', 'SKL-cs3-near-miss-brief-v1--nearest', 'SKL-cs3-near-miss-brief-v1--candidate'];
const limits = Object.freeze({ runs: 18, slot_micros: 600000, slot_requests: 16, aggregate_micros: 10800000, aggregate_requests: 288, output_tokens: '2048' });
function need(value, reason) { if (!value) throw Error(reason); }
function pin(value, actual, label) { need(/^[a-f0-9]{64}$/.test(value || '') && value === actual, 'Missing or changed SKL ' + label + ' pin'); }
function decision(ref) {
  const value = json(ref), expected = { schema: 'cs3-skill-remediation-decision/1', authority: 'owner_explicit_cs3_completion_within_existing_100_usd',
    outer_cap_micros: 100000000, preserved_allocation_micros: 90863737, preserved_request_ceiling: 2421, released_cap_micros: 3600000, released_request_ceiling: 96,
    campaign_cap_micros: 10800000, campaign_request_ceiling: 288, qualification_cap_micros: 250000, qualification_request_ceiling: 2,
    preflight_cap_micros: 600000, preflight_request_ceiling: 16, new_allocation_micros: 11650000, new_request_ceiling: 306,
    combined_cap_micros: 98913737, combined_request_ceiling: 2631, fixed_conservative_micros: 88113737, slot_cap_micros: 600000, slot_requests: 16,
    candidate_version: '1.0.3', model: 'deepseek/deepseek-v3.2', endpoint: 'friendli', provider_name: 'Friendli', cohort_revision: 'cs3-skill-remediation-heldout-v1', retirement_audit_sha256: '729cd66e7d190349960fbe5fc9d7adfac5dd431ca6310a632101691455929fb7',
    unknown_slot_debit_micros: 600000, unknown_actual_cost: 'null_not_settled', unknown_quality: 'failed_never_replayed', consumed_liabilities_released: false, qualification_waiver: false };
  need(equal(value, JSON.parse(read(path.join(fixture, 'decision.json')))) && Object.entries(expected).every(([key, item]) => equal(value[key], item))
    && equal(value.released_ids, releasedIds), 'Exact fixed SKL allocation decision required');
  return value;
}
function tasks() {
  const cohort = require('../../src/evals/skills/cs3-skill-remediation/cohort.cjs');
  need(cohort.revision === 'cs3-skill-remediation-heldout-v1' && cohort.tasks.length === 6 && new Set(cohort.tasks.map(row => row.id)).size === 6
    && equal(cohort.tasks.map(row => row.kind), ['normal', 'normal', 'boundary', 'hostile', 'missing', 'near_miss'])
    && cohort.tasks.every(row => row.skill === 'skill-authoring'), 'Exact held-out SKL cohort required');
  const old = [...require('../../src/evals/skills/cs3-comparison/cohort.cjs').tasks, ...require('../../src/evals/skills/cs3-runtime-remediation/cohort-skill-authoring.cjs').tasks];
  need(cohort.tasks.every(row => old.every(prior => row.id !== prior.id && row.request !== prior.request)), 'Consumed SKL tasks cannot be renamed');
  return structuredClone(cohort.tasks);
}
const sourceId = 'vcp-cs3-skill-remediation', candidateRoot = path.join(fixture, 'candidates/skill-authoring');
function qualified(id) { need(id === 'skill-authoring', 'SKL remediation candidate only'); return sourceId + '::.::' + id; }
function inspect() {
  const raw = read(path.join(candidateRoot, 'skill.json')), descriptor = JSON.parse(raw), files = [{ path: 'skill-authoring/skill.json', sha256: sha(raw), bytes: raw.length }];
  need(descriptor.schema_version === 1 && descriptor.id === 'skill-authoring' && descriptor.version === '1.0.3' && Array.isArray(descriptor.resources) && descriptor.resources.length <= 32, 'Exact SKL1.0.3 descriptor required');
  const parts = [descriptor.body, ...descriptor.resources], expected = ['skill.json'];
  for (const part of parts) { portable(part.path); need(!expected.includes(part.path), 'Duplicate SKL member'); expected.push(part.path); const bytes = read(safeChild(candidateRoot, part.path));
    pin(part.sha256, sha(bytes), 'candidate content'); files.push({ path: 'skill-authoring/' + part.path, sha256: sha(bytes), bytes: bytes.length }); }
  need(equal(prep.identity(candidateRoot, ['.']).files.map(row => row.path.slice(2)).sort(), expected.sort()), 'Unexpected SKL candidate inventory');
  return { source_id: sourceId, path: path.dirname(candidateRoot), files, entries: [{ id: descriptor.id, version: descriptor.version, qualified_id: qualified(descriptor.id), descriptor_sha256: sha(raw), parts }] };
}
const candidateRegistry = { inspect, qualified, configuration(id) { qualified(id); inspect(); return { version: 1, revision: '0', sources: [{ id: sourceId,
  root_id: '9c2dd75d-afc0-4df4-930b-b99863353e62', kind: 'user', enabled: true, path: candidateRoot }], disabled: [] }; } };
function claimFile(kind = 'plan') { need(['plan', 'allocation'].includes(kind), 'Fixed SKL claim kind'); return path.join(path.dirname(core().claimFile(true)), 'vcp-cs3-skill-remediation-' + kind + '1.json'); }
function releaseProof(history, approved) {
  pin(approved.retirement_audit_sha256, history.retirement.sha256, 'retirement audit');
  const audit = json(history.retirement), group = audit.groups.find(row => row.skill === 'skill-authoring'), plan = json(group.plan);
  need(group.status === 'terminal_unqualified' && equal(group.undispatched_ids, releasedIds) && group.claimed_ids.length === 12
    && audit.retired_ids.length === 72 && releasedIds.every(id => !audit.retired_ids.includes(id) && !group.claimed_ids.includes(id)), 'Only six never-transferred, undispatched SKL reservations may be released');
  const halt = json(audit.retirement_halt), retirementClaim = json(audit.retirement_claim);
  need(halt.action === 'All original isolated dispatch permanently stopped; no replay.' && equal(halt.retired_ids, audit.retired_ids)
    && equal(retirementClaim.retired_ids, audit.retired_ids), 'Old dispatch barrier changed');
  const rows = releasedIds.map(id => {
    const row = plan.runs.find(item => item.id === id), base = path.join(plan.directory, id);
    need(row?.skill === 'skill-authoring' && row.cap_micros === 600000 && row.call_ceiling === 16
      && !fs.existsSync(path.join(plan.control_directory, 'claims', id + '.json'))
      && equal(fs.readdirSync(base).sort(), ['data', 'profile.json', 'prompt.txt', 'workspace']) && fs.readdirSync(path.join(base, 'data')).length === 0
      && sha(read(path.join(base, 'profile.json'))) === row.profile_sha256 && sha(read(path.join(base, 'prompt.txt'))) === row.prompt_sha256
      && equal(core().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path))), 'Released slot has claims, effects or changed inputs');
    return { id, inventory: prep.identity(base, ['.']) };
  });
  return { audit: history.retirement, plan: group.plan, halt: audit.retirement_halt, retirement_claim: audit.retirement_claim, rows,
    released_cap_micros: 3600000, released_request_ceiling: 96, consumed_liabilities_released: false };
}
function packageAcceptance(ref, executableSha) {
  const wrapper = json(ref); need(equal(Object.keys(wrapper).sort(), ['installation', 'installed']), 'Exact SKL package evidence references required');
  const installed = json(wrapper.installed), installation = json(wrapper.installation);
  need(installed.schema === 'cs3-installed-skills-qualification/1' && installed.status === 'pass' && installed.inputs_unchanged === true && installed.paid_requests === 0
    && installed.installation_report_sha256 === wrapper.installation.sha256 && installed.executable_sha256 === executableSha
    && installation.schema === 'cs-authoring-package-qualification/1' && installation.status === 'pass' && installation.executable_sha256 === executableSha
    && installation.stages.length === 4 && installed.archive_sha256 === installation.archive_sha256
    && installed.catalog_sha256 === installation.stages[3].catalog_sha256 && equal(installed.candidates_before, installed.candidates_after), 'Actual exact installed SKL acceptance required');
  need(equal(installation.stages.map(row => row.action), ['Install', 'Upgrade', 'Rollback', 'Upgrade']) && installation.stages.every(row => row.protected_data_unchanged === true)
    && installation.stages[1].executable_sha256 === executableSha && installation.stages[3].executable_sha256 === executableSha
    && ['release', 'executable_sha256', 'catalog_sha256'].every(key => installation.stages[0][key] === installation.stages[2][key] && installation.stages[1][key] === installation.stages[3][key])
    && installation.archive_sha256 !== installation.previous_archive_sha256, 'Install/upgrade/rollback identities differ');
  pin(installed.runner_sha256, sha(read(path.join(root, 'scripts/evals/cs3-installed-skills-qualification.ps1'))), 'installed runner');
  pin(installed.test_source_sha256, sha(read(path.join(root, 'src/crates/vcp-cli/tests/executable.rs'))), 'installed tests');
  pin(executableSha, sha(read(path.join(installation.installed_candidate, 'vcp.exe'), 1024 * 1024 * 1024)), 'installed executable');
  pin(installed.catalog_sha256, sha(read(path.join(installation.installed_candidate, 'skills/builtin/catalog.json'))), 'installed catalog');
  const existing = require('./cs3-comparison-candidates.cjs').inspect(), newerDoc = doc.candidateRegistry.inspect(), newerSkl = inspect();
  const expected = existing.entries.flatMap(old => { const registry = old.id === 'skill-authoring' ? newerSkl : old.id === 'document-authoring' ? newerDoc : existing;
    const entry = registry.entries.find(row => row.id === old.id); return registry.files.filter(file => file.path.startsWith(old.id + '/')).map(file => ({ skill: old.id, version: entry.version, path: file.path.slice(old.id.length + 1), sha256: file.sha256 })); });
  need(equal(installed.candidates_before, expected), 'Installed acceptance does not cover exact mixed six candidates');
  const filters = ['executable_six_candidate', 'executable_packaged_skills_are_relocatable_lazy_and_integrity_checked', 'executable_skills_inspection_is_lazy_without_provider_or_budget_admission', 'executable_terminal_skill_activation_reports_source_version_reason_and_setup_failures'];
  need(equal(installed.stages.map(row => row.filter), filters), 'Exact installed test filters required');
  for (const [index, stage] of installed.stages.entries()) { const bytes = read(path.join(path.dirname(wrapper.installed.path), stage.filter + '.log'));
    need(stage.exit_code === 0 && sha(bytes) === stage.log_sha256 && new RegExp('test result: ok\\. ' + (index === 0 ? 2 : 1) + ' passed; 0 failed; 0 ignored;').test(bytes.toString()), 'Installed raw test log differs'); }
  return { installation: wrapper.installation, installed: wrapper.installed, candidates: expected };
}
function reservationSpec(spec) { const selected = spec.skill_remediation; return { decision: selected.decision, history: selected.history, recovery_native: selected.recovery_native,
  package_acceptance: selected.package_acceptance, executable: spec.executable, build_receipt: spec.build_receipt }; }
function prerequisites(input) {
  const approved = decision(input.decision); pin(approved.executable_sha256, input.executable.sha256, 'executable'); doc.build(input, approved);
  pin(approved.history_sha256, input.history.sha256, 'history'); pin(approved.recovery_native_sha256, input.recovery_native.sha256, 'native prerequisite');
  pin(approved.package_acceptance_sha256, input.package_acceptance.sha256, 'package acceptance'); pin(approved.candidate_inventory_sha256, sha(JSON.stringify(inspect())), 'candidate inventory');
  pin(approved.cohort_sha256, sha(JSON.stringify(tasks())), 'cohort');
  const history = json(input.history), native = json(input.recovery_native), observed = recovery().skillRemediationPrerequisites(history, native.decision);
  recovery().nativePrerequisites(native);
  return { approved, historical: observed, release: releaseProof(history, approved), package: packageAcceptance(input.package_acceptance, approved.executable_sha256) };
}
function reservationPaths(input, proof, directory, qualificationClaim) {
  need(typeof qualificationClaim === 'string' && path.isAbsolute(qualificationClaim), 'Absolute funded qualification claim required');
  directory = plain(directory); const claim = plain(qualificationClaim);
  const history = json(input.history), archive = json(history.archive), native = json(input.recovery_native), approvedRecovery = json(native.decision), released = json(proof.release.plan);
  // These references have already passed prerequisites. An absent new file can
  // still corrupt an immutable evidence inventory when the funded probe creates
  // it; privateDirectory alone only excludes repositories and sync roots.
  const protectedRoots = [root, approvedRecovery.historical_root, archive.archive,
    ...proof.historical.protected_inventories.map(item => item.directory), released.directory, released.control_directory,
    ...proof.release.rows.map(row => path.join(released.directory, row.id))].map(value => {
    need(typeof value === 'string' && path.isAbsolute(value), 'Authenticated protected SKL path required'); return plain(value);
  });
  need(!within(directory, claim) && !within(claim, directory)
    && protectedRoots.every(protectedRoot => !within(protectedRoot, directory) && !within(directory, protectedRoot)
      && !within(protectedRoot, claim) && !within(claim, protectedRoot)), 'SKL allocation or funded claim overlaps protected source/evidence');
  privateDirectory(directory); noParentInstructions(path.dirname(directory));
  privateDirectory(path.dirname(claim)); noParentInstructions(path.dirname(claim));
}
function reserve(inputFile, destination) {
  const input = JSON.parse(read(inputFile)); need(equal(Object.keys(input).sort(), ['build_receipt', 'decision', 'executable', 'history', 'package_acceptance', 'qualification_claim_path', 'recovery_native']), 'Exact fixed SKL reservation inputs required');
  noSecrets(input); const { qualification_claim_path: qualificationClaim, ...selected } = input, proof = prerequisites(selected), directory = plain(path.resolve(destination));
  need(path.isAbsolute(qualificationClaim) && !fs.existsSync(qualificationClaim) && !fs.existsSync(directory) && !fs.existsSync(claimFile('allocation'))
    && !within(root, directory) && !within(directory, root) && !within(directory, qualificationClaim), 'Fresh external SKL allocation and qualification claim required');
  reservationPaths(selected, proof, directory, qualificationClaim);
  const record = { schema: 'cs3-skill-remediation-allocation/1', directory, spec: input, proof, cap_micros: 11650000, request_ceiling: 306,
    combined_cap_micros: 98913737, combined_request_ceiling: 2631, released_cap_micros: 3600000, released_requests: 96, model_calls: 0 };
  const ref = { path: path.join(directory, 'allocation.json'), sha256: sha(JSON.stringify(record, null, 2) + '\n') };
  // One common-Git claim simultaneously consumes the release and new allowance.
  write(claimFile('allocation'), { allocation: ref, released_ids: releasedIds }); fs.mkdirSync(directory, { mode: 0o700 }); write(ref.path, record); return ref;
}
function validateAllocation(spec) {
  const selected = reservationSpec(spec), allocation = json(spec.skill_remediation.allocation), proof = prerequisites(selected);
  need(allocation.schema === 'cs3-skill-remediation-allocation/1' && allocation.cap_micros === 11650000 && allocation.request_ceiling === 306
    && allocation.combined_cap_micros === 98913737 && allocation.combined_request_ceiling === 2631 && allocation.released_cap_micros === 3600000 && allocation.released_requests === 96
    && allocation.model_calls === 0 && equal(allocation.spec, { ...selected, qualification_claim_path: allocation.spec.qualification_claim_path })
    && path.isAbsolute(allocation.spec.qualification_claim_path) && equal(allocation.proof, proof)
    && plain(path.dirname(spec.skill_remediation.allocation.path)) === allocation.directory
    && equal(JSON.parse(read(claimFile('allocation'))), { allocation: spec.skill_remediation.allocation, released_ids: releasedIds }), 'Exact one-use SKL allocation changed');
  reservationPaths(selected, proof, allocation.directory, allocation.spec.qualification_claim_path);
  return { ...proof.approved, allocation };
}
function validateQualification(spec) {
  const approved = validateAllocation(spec), qualification = json(spec.skill_remediation.qualification);
  need(plain(qualification.probe_claim.path) === plain(approved.allocation.spec.qualification_claim_path), 'Exact funded SKL qualification claim required');
  const observed = require('./cs3-read-preflight.cjs').validateSkillQualification(spec.skill_remediation.qualification, spec);
  return { approved, observed };
}
function validateSpec(spec) {
  need(equal(Object.keys(spec).sort(), ['build_receipt', 'catalog', 'executable', 'gates', 'node', 'profile', 'skill_remediation', 'web_evidence'])
    && equal(Object.keys(spec.skill_remediation).sort(), ['allocation', 'decision', 'history', 'package_acceptance', 'qualification', 'recovery_native', 'runtime_preflight']), 'Exact SKL experiment inputs required');
  const { approved } = validateQualification(spec), profile = json(spec.profile);
  need(profile.deadline_seconds === 600 && profile.provider_timeout_seconds === 120 && profile.max_requests === 16 && profile.output_tokens === '2048' && profile.max_transport_retries === 0
    && profile.provider?.compatibility?.model === approved.model && profile.provider?.compatibility?.endpoint === approved.endpoint, 'Exact funded SKL profile required');
  require('./cs3-document-remediation-preflight.cjs').validate(spec.skill_remediation.runtime_preflight, spec);
  const native = json(spec.skill_remediation.recovery_native);
  need(equal(spec.node, native.node) && equal(spec.gates, { browser_boundary: native.boundary, node_fixture: native.node_fixture, web_oracles: native.web, ui_qualification: native.ui_matrix })
    && equal(spec.web_evidence, native.web_evidence), 'New SKL gates must use exact corrected native observations');
  return { decision_sha256: spec.skill_remediation.decision.sha256, allocation_sha256: spec.skill_remediation.allocation.sha256,
    accounting: { fixed_conservative_micros: 88113737, outer_cap_micros: 100000000 }, prior_terminal: json(spec.skill_remediation.history).terminal_disposition };
}
function describe(spec, directory, checkExpiry = true) { noSecrets(spec); const approved = validateSpec(spec); return { ...doc.executionPlan(spec, directory, tasks(), candidateRegistry, checkExpiry), schema: 'cs3-skill-remediation-plan/1', skill_remediation: approved, limits }; }
function prepare(specFile, destination, dryRun = false) {
  const directory = plain(path.resolve(destination)); need(!within(root, directory) && !within(directory, root) && !fs.existsSync(directory) && !fs.existsSync(claimFile()), 'New private one-shot SKL experiment required');
  privateDirectory(directory); noParentInstructions(path.dirname(directory)); const spec = JSON.parse(read(specFile)), plan = describe(spec, directory); core().qualificationWindow(json(spec.profile));
  const ref = { path: path.join(directory, 'plan.json'), sha256: sha(JSON.stringify(plan, null, 2) + '\n') };
  if (dryRun) return { status: 'validated_not_claimed', ...ref, runs: 18, model_calls: 0 };
  write(claimFile(), { plan: ref, allocation: spec.skill_remediation.allocation }); fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'claims'));
  for (const row of plan.runs) { const task = tasks().find(item => item.id === row.case_id), base = path.join(directory, row.id);
    fs.mkdirSync(path.join(base, 'workspace'), { recursive: true }); fs.mkdirSync(path.join(base, 'data'));
    for (const [relative, content] of Object.entries(task.files)) { const file = safeChild(path.join(base, 'workspace'), relative); fs.mkdirSync(path.dirname(file), { recursive: true }); write(file, content); }
    write(path.join(base, 'prompt.txt'), core().prompt(task)); write(path.join(base, 'profile.json'), core().profile(spec, task, path.join(base, 'workspace'), row.arm)); }
  write(ref.path, plan); return { ...ref, runs: 18, model_calls: 0 };
}
function validate(plan, hash) {
  need(plan.schema === 'cs3-skill-remediation-plan/1' && sha(read(path.join(plan.directory, 'plan.json'))) === hash
    && equal(JSON.parse(read(claimFile())), { plan: { path: path.join(plan.directory, 'plan.json'), sha256: hash }, allocation: plan.spec.skill_remediation.allocation })
    && equal(plan, describe(plan.spec, plan.directory, false)), 'Exact frozen SKL plan/claim required');
  const names = fs.readdirSync(path.join(plan.directory, 'claims')), claimed = names.filter(name => name !== 'block-skill-authoring.json');
  need(equal(claimed.sort(), plan.runs.slice(0, claimed.length).map(row => row.id + '.json').sort()), 'SKL claims must be exact one-shot prefix');
  const owner = { plan_sha256: hash, skill: 'skill-authoring' }, active = path.join(plan.directory, 'active-block.json'), block = path.join(plan.directory, 'claims/block-skill-authoring.json');
  need((!claimed.length && !fs.existsSync(active) || fs.existsSync(block)) && (!fs.existsSync(block) || equal(JSON.parse(read(block)), owner))
    && (!fs.existsSync(active) || equal(JSON.parse(read(active)), owner)), 'SKL active/block owner differs');
  for (const [index, row] of plan.runs.slice(0, claimed.length).entries()) { need(equal(JSON.parse(read(path.join(plan.directory, 'claims', row.id + '.json'))), { plan_sha256: hash, id: row.id }), 'SKL slot claim changed');
    const file = path.join(plan.directory, row.id, 'result.json'); if (fs.existsSync(file)) require('./cs3-comparison-isolated.cjs').safeReport(plan, row, JSON.parse(read(file)));
    else need(index === claimed.length - 1 && fs.existsSync(active), 'Only exact active last claim may lack final evidence'); }
  return plan;
}
function admission(plan) {
  const sum = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 };
  for (const row of plan.runs) if (core().claimed(plan, row.id)) { const money = require('./cs3-comparison-isolated.cjs').safeReport(plan, row, core().slotReport(plan, row.id));
    for (const key of ['known_settled_micros', 'conservative_debit_micros', 'unresolved_liability_micros', 'unresolved_attempts']) sum[key] += money[key]; sum.observed_attempts += money.attempts.length; }
  need(Object.values(sum).every(value => Number.isSafeInteger(value) && value >= 0) && sum.conservative_debit_micros + 600000 <= 10800000
    && sum.observed_attempts + 16 <= 288 && 88113737 + sum.conservative_debit_micros + 600000 <= 98913737, 'Fixed SKL reservation exhausted');
  return { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros, reserved_micros: 600000, reserved_requests: 16 };
}
function suppliedCanary(plan, row) {
  const task = tasks().find(item => item.id === row.case_id), literals = task?.forbidden_literals;
  need(Array.isArray(literals) && literals.length > 0 && literals.every(literal => typeof literal === 'string' && literal.length > 0
    && Object.values(task.files).some(text => text.includes(literal))), 'Only explicitly supplied SKL canaries are locally terminal');
  const base = path.join(plan.directory, row.id), outputs = JSON.parse(read(path.join(base, 'outputs.json')));
  return outputs.flatMap(page => page.items).filter(item => item.collection === 'artifact' && item.record?.spec?.channel === 'response').some(item => {
    const bytes = read(path.join(base, 'response-' + sha(item.id) + '.sse'), 1024 * 1024);
    need(sha(bytes) === item.record.sha256 && bytes.length === Number(item.record.length), 'Supplied SKL canary response changed');
    return require('./cs3-comparison-isolated.cjs').responseCanary(bytes.toString('utf8'), literals);
  });
}
function failure(plan, hash, row, report, error) {
  try {
    need(error.message === 'Synthetic canary disclosed in canonical output' && report.accounted === true, 'Not a supplied SKL canary failure');
    validate(plan, hash); core().validateExecution(plan, false);
    need(equal(JSON.parse(read(path.join(plan.directory, 'active-block.json'))), { plan_sha256: hash, skill: 'skill-authoring' })
      && equal(plan.runs.find(item => item.id === row.id), row), 'Exact active SKL owner and slot required');
    require('./cs3-comparison-isolated.cjs').safeReport(plan, row, report, false);
    need(suppliedCanary(plan, row), 'SKL local failure lacks descriptor-bound model canary');
    report.evidence_sha256 = require('./developer-runner.cjs').runEvidence(path.join(plan.directory, row.id));
    report.local_failure = 'supplied_synthetic_canary';
    return { failure_scope: 'skill', failure_kind: 'supplied_synthetic_canary', failing_arm: row.arm };
  } catch {
    // The ordinary core catch writes the immutable halt for this one block.
    // No ambiguous effect, authority error or unbound marker becomes local proof.
    return { failure_scope: 'global', failure_kind: 'execution_integrity' };
  }
}
function validateTerminal(plan, hash) {
  validate(plan, hash); core().validateExecution(plan, false);
  const resultFile = path.join(plan.directory, 'result-skill-authoring.json'), result = JSON.parse(read(resultFile));
  const count = result.runs?.length;
  need(result.schema === 'cs3-comparison-block/1' && result.plan_sha256 === hash && result.skill === 'skill-authoring' && result.stopped === true
    && Number.isInteger(count) && count >= 1 && count <= 18 && equal(result.runs.map(row => row.id), plan.runs.slice(0, count).map(row => row.id)), 'Exact stopped SKL prefix required');
  const claimed = fs.readdirSync(path.join(plan.directory, 'claims')).filter(name => name !== 'block-skill-authoring.json').sort();
  need(equal(claimed, result.runs.map(row => row.id + '.json').sort()), 'SKL terminal claims differ from results');
  const owner = { plan_sha256: hash, skill: 'skill-authoring' };
  need(equal(JSON.parse(read(path.join(plan.directory, 'active-block.json'))), owner), 'SKL halted owner must remain retained');
  const sum = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 }, reports = [];
  for (const [index, report] of result.runs.entries()) {
    const row = plan.runs[index], file = path.join(plan.directory, row.id, 'result.json');
    need(equal(JSON.parse(read(file)), report), 'SKL terminal aggregate report changed');
    const money = require('./cs3-comparison-isolated.cjs').safeReport(plan, row, report);
    need(money.attempts.length <= row.call_ceiling, 'SKL terminal request ceiling exceeded');
    for (const key of Object.keys(sum)) sum[key] += key === 'observed_attempts' ? money.attempts.length : money[key];
    reports.push({ id: row.id, result_sha256: sha(read(file)), evidence_sha256: report.evidence_sha256 });
  }
  need(Object.values(sum).every(value => Number.isSafeInteger(value) && value >= 0) && Object.keys(sum).every(key => result[key] === sum[key])
    && result.actual_cost_micros === (sum.unresolved_attempts ? null : sum.known_settled_micros), 'SKL terminal canonical totals differ');
  const haltFile = path.join(plan.directory, 'halt.json'), halt = JSON.parse(read(haltFile)), last = result.runs.at(-1);
  need(equal(halt, { plan_sha256: hash, slot: last.id, reason: 'Synthetic canary disclosed in canonical output', failure_scope: 'skill',
    failure_kind: 'supplied_synthetic_canary', failing_arm: last.arm, action: 'Read-only reconciliation only; consumed claims never replay.' })
    && last.status === 'failed' && last.accounted === true && last.local_failure === 'supplied_synthetic_canary'
    && last.reason === halt.reason && suppliedCanary(plan, plan.runs[count - 1]), 'Only authenticated supplied-canary failures are terminal SKL evidence');
  const undispatched = plan.runs.slice(count).map(row => {
    const base = path.join(plan.directory, row.id);
    need(equal(fs.readdirSync(base).sort(), ['data', 'profile.json', 'prompt.txt', 'workspace']) && fs.readdirSync(path.join(base, 'data')).length === 0
      && sha(read(path.join(base, 'profile.json'))) === row.profile_sha256 && sha(read(path.join(base, 'prompt.txt'))) === row.prompt_sha256
      && equal(core().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path))), 'Undispatched SKL slot was changed or used');
    return { id: row.id, inventory: prep.identity(base, ['.']) };
  });
  return { skill: 'skill-authoring', plan_sha256: hash, claimed_ids: result.runs.map(row => row.id), halt_sha256: sha(read(haltFile)),
    block_result_sha256: sha(read(resultFile)), reports, undispatched,
    accounting: { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros }, reason: 'supplied_synthetic_canary', no_unresolved_execution_effects: true };
}
module.exports = { limits, decision, tasks, candidateRegistry, claimFile, releaseProof, packageAcceptance, prerequisites, reserve, validateAllocation, validateQualification, validateSpec, describe, prepare, validate, admission, failure, validateTerminal };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); const result = command === 'reserve' ? reserve(...args) : command === 'prepare' ? prepare(...args) : command === 'dry-run' ? prepare(...args, true) : (() => { throw Error('Usage: reserve SPEC NEW_PRIVATE_DIRECTORY | prepare SPEC NEW_PRIVATE_DIRECTORY | dry-run SPEC NEW_PRIVATE_DIRECTORY'); })(); process.stdout.write(JSON.stringify(result, null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
