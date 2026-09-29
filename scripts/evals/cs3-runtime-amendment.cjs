// SPDX-License-Identifier: Apache-2.0
'use strict';
// One source-frozen runtime amendment. Reuses the canonical runner and isolated
// controller; old reservations can move only after irreversible old dispatch denial.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prep = require('./authoring-prepare.cjs'), prior = require('./p6-live-runner.cjs');
const policy = require('./cs3-comparison-policy.cjs'), candidates = require('./cs3-comparison-candidates.cjs');
const { read, write, plain, within, safeChild, privateDirectory, noParentInstructions, noSecrets } = prior.boundaries;
const root = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const doc = () => require('./cs3-document-remediation.cjs'), core = () => require('./cs3-comparison.cjs'), isolation = () => require('./cs3-comparison-isolated.cjs');
const json = file => JSON.parse(read(file)), bound = ref => JSON.parse(doc().bound(ref, 64 * 1024 * 1024));
const skills = ['skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
const oracleCases = ['UI-cs3-boundary-authority-v1', 'UI-cs3-hostile-note-v1', 'MCP-cs3-boundary-authority-v1', 'MCP-cs3-hostile-note-v1', 'LLM-cs3-boundary-authority-v1', 'LLM-cs3-hostile-note-v1'];
function oracleAmendment(value) {
  const expected = { id: 'cs3-runtime-boundary-attribution-v1', original_oracle_sha256: 'a588ee3c2391c07ab687b03af78e3588297547e7425103f6e789a18953340bfe',
    replacement_module: 'scripts/evals/cs3-runtime-boundary-oracle.cjs', replacement_sha256: value?.replacement_sha256, affected_case_ids: oracleCases,
    reason: 'Distinguish rejected or attributed draft text from current universal-success assertions; preserve genuine-assertion rejections and every other check.' };
  if (!equal(value, expected) || !/^[a-f0-9]{64}$/.test(value.replacement_sha256 || '')
    || sha(read(path.join(root, 'scripts/evals/cs3-comparison-oracle.cjs'))) !== value.original_oracle_sha256
    || sha(read(path.join(root, value.replacement_module))) !== value.replacement_sha256) throw Error('Exact source-bound six-case oracle amendment required');
  return value;
}
const limits = Object.freeze({ runs: 90, slot_micros: 600000, slot_requests: 16, aggregate_micros: 54000000, aggregate_requests: 1440, output_tokens: '2048' });
function claimFile() { return path.join(path.dirname(core().claimFile(true)), 'vcp-cs3-runtime-amendment1.json'); }
function retirementClaimFile() { return path.join(path.dirname(core().claimFile(true)), 'vcp-cs3-runtime-remediation-retirement1.json'); }
function decision(ref) {
  const value = bound(ref), tracked = json(path.join(root, 'src/evals/skills/cs3-runtime-remediation/decision.json'));
  if (!equal(value, tracked) || value.schema !== 'cs3-runtime-amendment-decision/1' || !equal(value.skills, skills)
    || value.runs !== 90 || value.fresh_skill_authoring_runs !== 18 || value.retired_untouched_runs !== 72
    || value.fresh_skill_authoring_cap_micros !== 10800000 || value.fresh_skill_authoring_requests !== 288
    || value.transferred_cap_micros !== 43200000 || value.transferred_request_ceiling !== 1152
    || value.runtime_cap_micros !== 54000000 || value.runtime_request_ceiling !== 1440 || value.combined_cap_micros !== 89063737 || value.combined_request_ceiling !== 2373
    || value.deadline_seconds !== 600 || value.provider_timeout_seconds !== 120 || value.slot_cap_micros !== 600000 || value.slot_requests !== 16 || value.output_tokens !== '2048'
    || value.max_transport_retries !== 0 || value.original_skill_authoring_candidate_version !== '1.0.2' || value.fresh_cohort_revision !== 'cs3-runtime-skill-authoring-v1'
    || value.authority !== 'owner_explicit_cs3_completion_within_existing_100_usd' || value.qualification_or_promotion_waiver !== false
    || value.retirement !== 'one_to_one_permanent_old_dispatch_barrier_no_replay' || value.security !== 'local_supplied_model_canary_only_global_execution_or_evidence_failure') throw Error('Exact fixed runtime amendment decision required');
  oracleAmendment(value.oracle_amendment); return value;
}
function retirement(audit, manifest, reference) {
  const ids = manifest.groups.slice(1).flatMap(group => group.ids), haltPath = path.join(manifest.directory, 'global-halt.json');
  if (audit.model_calls !== 0 || audit.archived_dispatch_denied_groups !== 5 || ids.length !== 72 || new Set(ids).size !== 72 || !equal(audit.retired_ids, ids)
    || audit.transferred_cap_micros !== 43200000 || audit.transferred_request_ceiling !== 1152 || plain(audit.retirement_halt?.path || '') !== plain(haltPath)
    || plain(audit.retirement_claim?.path || '') !== plain(retirementClaimFile())) throw Error('Exact seventy-two-slot retirement and old dispatch barrier required');
  const halt = bound(audit.retirement_halt), claim = bound(audit.retirement_claim), prepared = bound(claim.prepared);
  const expectedHalt = { schema: 'cs3-comparison-runtime-retirement-halt/1', manifest_sha256: audit.manifest.sha256,
    reason: 'untouched_skills_superseded_by_source_bound_runtime_amendment', retired_ids: ids, action: 'All original isolated dispatch permanently stopped; no replay.' };
  if (!equal(halt, expectedHalt) || !equal(claim, { schema: 'cs3-runtime-retirement-claim/1', manifest: audit.manifest, prepared: claim.prepared,
    audit_path: claim.audit_path, retirement_halt: audit.retirement_halt, retired_ids: ids, transferred_cap_micros: 43200000, transferred_request_ceiling: 1152 })
    || !path.isAbsolute(claim.audit_path || '') || plain(claim.audit_path) !== plain(reference.path) || prepared.schema !== 'cs3-comparison-isolation-retirement-prepared/1') throw Error('Retirement halt/claim/preparation identity differs');
  const { controls_inventory, retirement_claim, archived_dispatch_denied_groups, ...before } = audit;
  if (!equal(prepared, { ...before, schema: 'cs3-comparison-isolation-retirement-prepared/1' })) throw Error('Retirement preparation proof changed');
  const files = controls_inventory.files.filter(file => file.path !== './global-halt.json'), directories = controls_inventory.directories;
  const expectedBefore = { ...controls_inventory, files, content_sha256: sha(JSON.stringify({ files, directories })) };
  const added = controls_inventory.files.filter(file => file.path === './global-halt.json');
  if (!equal(expectedBefore, audit.pre_controls_inventory) || added.length !== 1 || added[0].sha256 !== audit.retirement_halt.sha256
    || !equal(prep.identity(manifest.directory, ['.']), controls_inventory)) throw Error('Retirement may add only the permanent global halt');
  for (const group of audit.groups.slice(1)) {
    const plan = bound(group.plan), control = plan.control_directory;
    if (!equal(fs.readdirSync(control).sort(), ['claims', 'plan.json']) || fs.readdirSync(path.join(control, 'claims')).length) throw Error('Retired skill has claims or execution evidence');
  }
  return { retired_ids: ids, transferred_cap_micros: 43200000, transferred_request_ceiling: 1152 };
}
function tasks(spec) {
  const fresh = require('../../src/evals/skills/cs3-runtime-remediation/cohort-skill-authoring.cjs'), old = core().cohort(spec.web_evidence, true);
  if (fresh.revision !== 'cs3-runtime-skill-authoring-v1' || fresh.tasks.length !== 6 || new Set(fresh.tasks.map(t => t.id)).size !== 6
    || !equal(fresh.tasks.map(t => t.kind), ['normal', 'normal', 'boundary', 'hostile', 'missing', 'near_miss'])
    || fresh.tasks.some(task => task.skill !== 'skill-authoring' || old.some(prior => prior.id === task.id || prior.request === task.request))) throw Error('Genuinely fresh six-task SKL cohort required');
  return [...structuredClone(fresh.tasks), ...old.filter(task => skills.slice(1).includes(task.skill))];
}
function validateSpec(spec) {
  if (!equal(Object.keys(spec).sort(), ['build_receipt', 'catalog', 'executable', 'gates', 'node', 'profile', 'remediation', 'runtime_amendment', 'web_evidence'])
    || !equal(spec.runtime_amendment, { decision: spec.remediation?.runtime_decision })
    || !equal(Object.keys(spec.remediation).sort(), ['allocation', 'decision', 'prior_terminal', 'qualification', 'runtime_decision', 'runtime_preflight'])) throw Error('Exact runtime amendment specification required');
  const approved = decision(spec.runtime_amendment.decision); doc().validateAllocation(spec);
  const profile = bound(spec.profile);
  if (profile.deadline_seconds !== 600 || profile.provider_timeout_seconds !== 120 || profile.max_requests !== 16 || profile.output_tokens !== '2048' || profile.max_transport_retries !== 0) throw Error('Runtime amendment profile limits differ');
  require('./cs3-read-preflight.cjs').validateQualification(spec.remediation.qualification, spec);
  require('./cs3-document-remediation-preflight.cjs').validate(spec.remediation.runtime_preflight, spec);
  return approved;
}
function describe(spec, destination, checkExpiry = true) {
  noSecrets(spec); const approved = validateSpec(spec), directory = plain(path.resolve(destination)), all = tasks(spec);
  const base = doc().executionPlan(spec, path.join(directory, 'slots'), all, candidates, checkExpiry), audit = bound(spec.remediation.prior_terminal);
  const mapping = [], transferred = [];
  for (const group of audit.groups.slice(1)) {
    const old = bound(group.plan), generated = base.runs.filter(row => row.skill === group.skill), next = old.runs.map(row => generated.find(item => item.id === row.id));
    const { path: currentPath, ...currentAssets } = base.candidate_assets, { path: previousPath, ...previousAssets } = old.candidate_assets;
    if (!path.isAbsolute(currentPath || '') || !path.isAbsolute(previousPath || '') || !equal(currentAssets, previousAssets)
      || !equal(generated.map(row => row.id).sort(), old.runs.map(row => row.id).sort())) throw Error('Transferred tasks, arm assignment or candidate bytes changed');
    for (const row of next) {
      const original = old.runs.find(r => r.id === row.id);
      if (!equal({ ...original, profile_sha256: row.profile_sha256 }, row)) throw Error('Retired assignment differs beyond explicitly new profile/runtime');
      mapping.push({ id: row.id, original_plan: group.plan, original_profile_sha256: original.profile_sha256, new_profile_sha256: row.profile_sha256 });
      transferred.push(row);
    }
  }
  if (base.runs.length !== 90 || new Set(base.runs.map(row => row.id)).size !== 90 || mapping.length !== 72 || !equal(mapping.map(row => row.id), audit.retired_ids)
    || base.candidate_assets.entries.find(entry => entry.id === 'skill-authoring')?.version !== '1.0.2') throw Error('Fixed ninety-slot runtime membership differs');
  base.runs = [...base.runs.filter(row => row.skill === 'skill-authoring'), ...transferred];
  return { schema: 'cs3-runtime-amendment/1', directory, spec, source: base.source, base, oracle_amendment: approved.oracle_amendment,
    groups: skills.map(skill => ({ skill, ids: base.runs.filter(row => row.skill === skill).map(row => row.id) })), mapping,
    prefix: audit.totals, excluded_ids: [...audit.excluded_ids, ...audit.groups[0].undispatched_ids], model_calls: 0 };
}
function project(manifest, reference, skill) {
  return { ...manifest.base, schema: 'cs3-comparison-isolated-plan/1', limits, control_directory: path.join(manifest.directory, skill),
    runs: manifest.base.runs.filter(row => row.skill === skill), isolated: { manifest: reference, skill },
    phase_rule: 'Five serial eighteen-slot groups. Fresh SKL plus exact seventy-two retired assignments; one shot each. Safe provider-only uncertainty debits the full slot and fails quality. Authenticated supplied model canary or reader security failures make only their skill terminal; execution, authority or evidence-integrity failures halt all groups. No retry or replay.',
    runtime_amendment: { decision: manifest.spec.runtime_amendment.decision, oracle_amendment: manifest.oracle_amendment, accounting: { fixed_conservative_micros: 35063737, outer_cap_micros: 100000000 } } };
}
function prepare(specFile, destination, dryRun = false) {
  const directory = plain(path.resolve(destination));
  if (within(root, directory) || within(directory, root) || fs.existsSync(directory) || fs.existsSync(claimFile())) throw Error('New private one-shot runtime amendment required');
  privateDirectory(directory); noParentInstructions(path.dirname(directory));
  const manifest = describe(json(specFile), directory), bytes = JSON.stringify(manifest, null, 2) + '\n', ref = { path: path.join(directory, 'manifest.json'), sha256: sha(bytes) };
  const plans = skills.map(skill => project(manifest, ref, skill));
  core().validateExecution(plans[0]); core().qualificationWindow(bound(manifest.spec.profile));
  if (dryRun) return { status: 'validated_not_claimed', manifest_sha256: ref.sha256, fresh: 18, transferred: 72, runs: 90, groups: 5, model_calls: 0 };
  write(claimFile(), { manifest: ref, allocation: manifest.spec.remediation.allocation, retirement: manifest.spec.remediation.prior_terminal });
  fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'transitions')); write(ref.path, bytes);
  for (const row of manifest.base.runs) {
    const base = path.join(manifest.base.directory, row.id), task = tasks(manifest.spec).find(task => task.id === row.case_id);
    fs.mkdirSync(path.join(base, 'workspace'), { recursive: true }); fs.mkdirSync(path.join(base, 'data'));
    for (const [relative, content] of Object.entries(task.files)) { const file = safeChild(path.join(base, 'workspace'), relative); fs.mkdirSync(path.dirname(file), { recursive: true }); write(file, content); }
    write(path.join(base, 'prompt.txt'), core().prompt(task)); write(path.join(base, 'profile.json'), core().profile(manifest.spec, task, path.join(base, 'workspace'), row.arm));
  }
  const prepared = plans.map(plan => { fs.mkdirSync(plan.control_directory); fs.mkdirSync(path.join(plan.control_directory, 'claims')); const file = path.join(plan.control_directory, 'plan.json'); write(file, plan); return { skill: plan.isolated.skill, path: file, sha256: sha(read(file)) }; });
  return { manifest: ref, plans: prepared, model_calls: 0 };
}
function load(plan) {
  const ref = plan.isolated?.manifest, manifest = bound(ref);
  if (manifest.schema !== 'cs3-runtime-amendment/1' || plain(path.dirname(ref.path)) !== manifest.directory
    || !equal(json(claimFile()), { manifest: ref, allocation: manifest.spec.remediation.allocation, retirement: manifest.spec.remediation.prior_terminal })) throw Error('Exact runtime amendment ownership required');
  privateDirectory(manifest.directory); noParentInstructions(manifest.directory);
  if (fs.existsSync(path.join(manifest.directory, 'global-halt.json'))) throw Error('Shared runtime execution integrity halted');
  return manifest;
}
function groups(manifest, reference) { return isolation().inspectGroups(skills.map(skill => project(manifest, reference, skill))); }
function controlInventory(manifest) {
  const entries = fs.readdirSync(manifest.directory).sort(), expected = ['manifest.json', 'slots', 'transitions', ...skills, ...(fs.existsSync(path.join(manifest.directory, 'active-skill.json')) ? ['active-skill.json'] : [])].sort();
  if (!equal(entries, expected) || !equal(fs.readdirSync(manifest.base.directory).sort(), manifest.base.runs.map(row => row.id).sort())) throw Error('Exact runtime controls and ninety-slot directory inventory required');
  return prep.identity(manifest.directory, entries.filter(name => name !== 'slots'));
}
function validate(plan, hash) {
  const manifest = load(plan);
  if (!equal(manifest, describe(manifest.spec, manifest.directory, false)) || !equal(plan, project(manifest, plan.isolated.manifest, plan.isolated.skill))) throw Error('Frozen runtime source, prerequisites or assignments changed');
  const all = groups(manifest, plan.isolated.manifest); isolation().validateOwners(plan, hash, manifest, all);
  controlInventory(manifest);
  for (const row of manifest.base.runs) {
    const base = path.join(manifest.base.directory, row.id);
    if (sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256 || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256
      || !equal(core().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path)))) throw Error('Frozen runtime input bytes changed');
  }
  for (const group of all) for (const [index, row] of group.claimed.entries()) {
    const file = core().reportFile(group.plan, row.id);
    if (fs.existsSync(file)) isolation().safeReport(group.plan, row, json(file));
    else if (index !== group.claimed.length - 1 || !equal(json(path.join(group.plan.control_directory, 'active-block.json')), { plan_sha256: group.hash, skill: group.plan.isolated.skill })) throw Error('Incomplete runtime observation lacks exact active owner');
  }
  return plan;
}
function admission(plan) {
  const manifest = load(plan), sum = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 };
  for (const group of groups(manifest, plan.isolated.manifest)) for (const row of group.claimed) {
    const money = isolation().safeReport(group.plan, row, core().slotReport(group.plan, row.id));
    for (const key of Object.keys(sum)) sum[key] += key === 'observed_attempts' ? money.attempts.length : money[key];
  }
  if (Object.values(sum).some(value => !Number.isSafeInteger(value) || value < 0) || sum.conservative_debit_micros + 600000 > 54000000 || sum.observed_attempts + 16 > 1440
    || 35063737 + sum.conservative_debit_micros + 600000 > 89063737) throw Error('Shared runtime reservation exhausted');
  return { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros, reserved_micros: 600000, reserved_requests: 16 };
}
function terminalRecord(reference) {
  const manifest = bound(reference), all = groups(manifest, reference), totals = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 };
  const entries = all.map(group => {
    const skill = group.plan.isolated.skill, review = require('./cs3-comparison-review.cjs'), partial = fs.existsSync(path.join(group.plan.control_directory, 'terminal-disposition-' + skill + '.json'));
    const disposition = partial ? review.validateTerminalDisposition(group.file, group.hash) : review.validateDisposition(group.file, group.hash, skill);
    const file = path.join(group.plan.control_directory, (partial ? 'terminal-disposition-' : 'disposition-') + skill + '.json');
    const slots = group.claimed.map(row => { const money = isolation().safeReport(group.plan, row, core().slotReport(group.plan, row.id));
      for (const key of Object.keys(totals)) totals[key] += key === 'observed_attempts' ? money.attempts.length : money[key];
      return { id: row.id, inventory: prep.identity(path.join(group.plan.directory, row.id), ['.']), accounting: policy.fields(money) }; });
    return { skill, plan: { path: group.file, sha256: group.hash }, disposition: doc().reference(file), status: disposition.status,
      claimed_ids: group.claimed.map(row => row.id), undispatched_ids: group.plan.runs.slice(group.claimed.length).map(row => row.id), slots };
  });
  return { schema: 'cs3-runtime-amendment-terminal/1', manifest: reference, allocation: manifest.spec.remediation.allocation,
    groups: entries, totals, controls_inventory: controlInventory(manifest), model_calls: 0 };
}
function terminal(ref, spec) {
  const receipt = bound(ref), manifest = bound(receipt.manifest);
  const expectedSpec = { ...spec, remediation: { ...spec.remediation }, runtime_amendment: { decision: spec.remediation.runtime_decision } };
  delete expectedSpec.remediation.runtime_terminal;
  if (!equal(manifest.spec, expectedSpec) || !equal(receipt, terminalRecord(receipt.manifest))) throw Error('Actual five-group terminal runtime evidence required before DOC');
  return { audit_sha256: ref.sha256, manifest_sha256: receipt.manifest.sha256, groups: 5, totals: receipt.totals };
}
function writeTerminal(manifestFile, manifestHash, destination) {
  const record = terminalRecord({ path: plain(path.resolve(manifestFile)), sha256: manifestHash });
  const file = plain(path.resolve(destination)), manifest = bound(record.manifest);
  if (within(root, file) || within(manifest.directory, file)) throw Error('Terminal audit must be a new external sibling, not mutate its captured inventory');
  write(file, record); return doc().reference(file);
}
module.exports = { decision, oracleAmendment, retirement, tasks, validateSpec, describe, project, prepare, load, groups, validate, controlInventory, admission, terminal, terminalRecord, writeTerminal, claimFile, retirementClaimFile, skills, limits };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); const result = command === 'prepare' ? prepare(...args) : command === 'dry-run' ? prepare(...args, true) : command === 'terminal' ? writeTerminal(...args) : (() => { throw Error('Usage: prepare|dry-run SPEC NEW_PRIVATE_ROOT | terminal MANIFEST HASH NEW_AUDIT_FILE'); })(); process.stdout.write(JSON.stringify(result, null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
