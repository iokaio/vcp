// SPDX-License-Identifier: Apache-2.0
'use strict';
// One fixed prospective DOC experiment. Historical allocations and observations
// remain immutable; this module never resumes or rewrites their native ledgers.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), prep = require('./authoring-prepare.cjs');
const policy = require('./cs3-comparison-policy.cjs'), capture = require('./developer-runner.cjs');
const { inspectAssets, portable } = require('../skills/builtin-assets.cjs');
const { requireEmbeddedCatalog } = require('./builtin-generation-prepare.cjs');
const { plain, read, write, within, safeChild, noParentInstructions, privateDirectory, noSecrets } = prior.boundaries;
const root = path.resolve(__dirname, '../..'), fixture = path.join(root, 'src/evals/skills/cs3-document-remediation');
const campaign = () => require('./cs3-comparison.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = file => JSON.parse(read(file));
const hex = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
function bound(ref, maximum = 16 * 1024 * 1024) {
  if (!ref || !path.isAbsolute(ref.path || '') || !hex(ref.sha256)) throw Error('Absolute bound remediation reference required');
  const bytes = read(plain(ref.path), maximum); if (sha(bytes) !== ref.sha256) throw Error('Remediation evidence changed'); return bytes;
}
const reference = file => ({ path: plain(path.resolve(file)), sha256: sha(read(file, 1024 * 1024 * 1024)) });
const limits = Object.freeze({ runs: 18, slot_micros: 600000, slot_requests: 16, aggregate_micros: 10800000, aggregate_requests: 288, output_tokens: '2048' });
const buildScope = Object.freeze(['src/crates', 'src/evals', 'src/tests/fixtures', 'src/skills/builtin',
  'src/third_party/codex/codex-rs/Cargo.toml', 'src/third_party/codex/codex-rs/Cargo.lock', 'src/third_party/codex/codex-rs/rust-toolchain.toml', 'src/third_party/codex/codex-rs/.cargo',
  'src/third_party/munarium/server/src', 'src/third_party/munarium/server/Cargo.toml', 'src/third_party/munarium/server/Cargo.lock',
  'src/third_party/components', 'src/third_party/upstreams.toml', 'scripts/upstream', 'src/tests/support',
  'scripts/skills/builtin-assets.cjs', 'scripts/evals', 'src/tests/package.json', 'src/tests/package-lock.json']);
function decision(ref) {
  const value = JSON.parse(bound(ref));
  if (!equal(value, json(path.join(fixture, 'decision.json'))) || value.schema !== 'cs3-document-remediation-decision/1'
    || value.authority !== 'owner_explicit_cs3_completion_within_existing_100_usd') throw Error('Source-pinned remediation decision required');
  const fixed = { outer_cap_micros: 100000000, preserved_prior_allocation_micros: 66613737, preserved_prior_request_ceiling: 1779,
    campaign_cap_micros: 10800000, campaign_requests: 288, runtime_preflight_cap_micros: 600000, runtime_preflight_requests: 16,
    qualification_cap_micros: 250000, qualification_requests: 2, new_allocation_micros: 22450000, new_request_ceiling: 594,
    combined_allocation_micros: 89063737, combined_request_ceiling: 2373, remaining_unallocated_micros: 10936263,
    fresh_skill_authoring_cap_micros: 10800000, fresh_skill_authoring_requests: 288, transferred_cap_micros: 43200000, transferred_requests: 1152,
    model: 'deepseek/deepseek-v3.2', endpoint: 'deepinfra/fp4',
    deadline_seconds: 600, provider_timeout_seconds: 120, output_tokens: '2048', slot_cap_micros: 600000, slot_requests: 16,
    candidate_version: '1.0.5', cohort_revision: 'cs-3-doc-remediation-fixtures-v3', unknown_actual_cost: 'null_not_settled',
    unknown_slot_debit_micros: 600000, unknown_quality: 'failed_never_replayed', historical_allocations: 'preserved_in_full_only_exact_72_retired_untouched_reservations_transferred_once',
    historical_claims: 'all_immutable_no_replay', prerequisite: 'authenticated_old_skill_terminal_and_72_permanently_retired_before_shared_probe_then_all_five_new_groups_terminal_before_doc', qualification_or_promotion_waiver: false };
  if (Object.entries(fixed).some(([key, expected]) => value[key] !== expected) || !hex(value.prior_terminal_audit_sha256) || !hex(value.executable_sha256)) throw Error('Final terminal audit, new executable and exact separate allocation required');
  return value;
}
function tasks() {
  const cohort = require('../../src/evals/skills/cs3-document-remediation/cohort.cjs');
  if (cohort.revision !== 'cs-3-doc-remediation-fixtures-v3' || cohort.tasks.length !== 6 || new Set(cohort.tasks.map(t => t.id)).size !== 6
    || cohort.tasks.some(t => t.skill !== 'document-authoring') || !equal(cohort.tasks.map(t => t.kind), ['normal', 'normal', 'boundary', 'hostile', 'missing', 'near_miss'])) throw Error('Exact fresh DOC cohort required');
  const old = [...require('../../src/evals/skills/cs3-comparison/cohort.cjs').tasks, ...require('../../src/evals/skills/cs3-comparison/cohort-doc-successor.cjs').tasks];
  if (cohort.tasks.some(t => old.some(p => p.id === t.id || p.request === t.request))) throw Error('Consumed tasks cannot be renamed into remediation');
  return structuredClone(cohort.tasks);
}
const sourceId = 'vcp-cs3-document-remediation';
const candidateRoot = path.join(fixture, 'candidates/document-authoring');
function qualified(id) { if (id !== 'document-authoring') throw Error('DOC remediation candidate only'); return `${sourceId}::.::${id}`; }
function inspectCandidate() {
  const descriptorBytes = read(path.join(candidateRoot, 'skill.json')), descriptor = JSON.parse(descriptorBytes);
  if (descriptor.schema_version !== 1 || descriptor.id !== 'document-authoring' || descriptor.version !== '1.0.5' || !Array.isArray(descriptor.resources)) throw Error('Exact DOC 1.0.5 descriptor required');
  const parts = [descriptor.body, ...descriptor.resources], files = [{ path: 'document-authoring/skill.json', sha256: sha(descriptorBytes), bytes: descriptorBytes.length }], expected = ['skill.json'];
  for (const part of parts) { portable(part.path); if (expected.includes(part.path)) throw Error('Duplicate candidate part'); expected.push(part.path); const bytes = read(safeChild(candidateRoot, part.path)); if (sha(bytes) !== part.sha256) throw Error('Remediation candidate part changed'); files.push({ path: 'document-authoring/' + part.path, sha256: sha(bytes), bytes: bytes.length }); }
  const inventory = prep.identity(candidateRoot, ['.']);
  if (!equal(inventory.files.map(f => f.path.slice(2)).sort(), expected.sort())) throw Error('Unexpected remediation candidate file');
  return { source_id: sourceId, path: path.dirname(candidateRoot), files, entries: [{ id: descriptor.id, version: descriptor.version, qualified_id: qualified(descriptor.id), descriptor_sha256: sha(descriptorBytes), parts }] };
}
const candidateRegistry = { inspect: inspectCandidate, qualified, configuration(id) { qualified(id); inspectCandidate(); return { version: 1, revision: '0', sources: [{ id: sourceId, root_id: 'a6c8f74e-1d5a-43e7-8d6c-e3450a538ca2', kind: 'user', enabled: true, path: candidateRoot }], disabled: [] }; } };
function priorTerminal(ref, approved) {
  if (ref.sha256 !== approved.prior_terminal_audit_sha256) throw Error('Exact owner-bound terminal audit required');
  const audit = JSON.parse(bound(ref)), manifest = JSON.parse(bound(audit.manifest));
  if (audit.schema !== 'cs3-comparison-isolation-retirement/1' || manifest.schema !== 'cs3-comparison-isolation/1'
    || !equal(JSON.parse(bound(audit.claim)), { directory: manifest.directory, manifest_sha256: audit.manifest.sha256 })
    || !equal(audit.source_archive?.source, manifest.source) || !path.isAbsolute(audit.source_archive?.path || '')
    || !equal(prep.identity(audit.source_archive.path, manifest.source.scope), manifest.source)
    || !equal(prep.identity(manifest.directory, ['.']), audit.controls_inventory)
    || !equal(audit.prefix, manifest.prefix) || !equal(audit.excluded_ids, manifest.excluded_ids) || audit.excluded_ids.length !== 7) throw Error('Historical isolation source, ownership or terminal inventory differs');
  require('./cs3-runtime-amendment.cjs').retirement(audit, manifest, ref);
  const skills = ['skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
  if (!equal(audit.groups.map(g => g.skill), skills) || audit.groups.length !== 5) throw Error('One terminal skill and four exact retired groups required');
  const previous = JSON.parse(bound(manifest.spec.predecessor)), previousAudit = JSON.parse(bound(manifest.spec.audit));
  if (previousAudit.schema !== 'cs3-comparison-segment-terminal-audit/1' || previousAudit.plan_sha256 !== manifest.spec.predecessor.sha256
    || previousAudit.consumed.length !== 11 || !equal(previousAudit.totals, manifest.prefix)
    || sha(read(require('./cs3-comparison-segment.cjs').claimFile())) !== previousAudit.claim_sha256
    || sha(read(path.join(previous.control_directory, 'halt.json'))) !== previousAudit.halt_sha256
    || sha(read(path.join(previous.control_directory, 'result-document-authoring.json'))) !== previousAudit.block_sha256
    || sha(read(path.join(previous.control_directory, 'active-block.json'))) !== previousAudit.active_block_sha256) throw Error('Historical DOC terminal chain changed');
  for (const claim of previousAudit.claims) if (sha(read(path.join(previous.control_directory, 'claims', claim.name))) !== claim.sha256) throw Error('Historical DOC claim changed');
  for (const retained of previousAudit.consumed) {
    const base = path.join(previous.directory, retained.id), reportPath = previous.segment.addenda.some(row => row.id === retained.id) ? path.join(previous.control_directory, 'addenda', retained.id + '.json') : path.join(base, 'result.json');
    if (!equal(prep.identity(base, ['.']), retained.inventory) || sha(read(reportPath)) !== retained.result_sha256 || capture.runEvidence(base) !== retained.evidence_sha256) throw Error('Historical DOC observation changed');
  }
  for (const id of manifest.excluded_ids) {
    const row = previous.runs.find(r => r.id === id); if (!row || row.skill !== 'document-authoring') throw Error('Historical DOC exclusion changed');
    const base = path.join(previous.directory, id);
    if (sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256 || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256
      || !equal(campaign().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path)))) throw Error('Excluded historical DOC inputs changed');
    require('./cs3-comparison-segment.cjs').pristine(previous, row);
  }
  const totals = { ...manifest.prefix }, all = new Set();
  for (const group of audit.groups) {
    const plan = JSON.parse(bound(group.plan)), retired = group.status === 'retired_undispatched', disposition = retired ? null : JSON.parse(bound(group.disposition));
    const expected = manifest.groups.find(g => g.skill === group.skill)?.ids;
    if (!equal(plan.runs.map(r => r.id), expected) || expected.length !== 18 || plan.isolated?.skill !== group.skill
      || !equal(plan.isolated.manifest, audit.manifest) || !retired && (disposition.plan_sha256 !== group.plan.sha256 || disposition.skill !== group.skill || group.status !== disposition.status)
      || !['qualified', 'unqualified', 'terminal_unqualified', 'retired_undispatched'].includes(group.status) || !Array.isArray(group.claimed_ids)
      || !equal(group.claimed_ids, expected.slice(0, group.claimed_ids.length)) || !equal(group.undispatched_ids, expected.slice(group.claimed_ids.length))
      || !equal(group.slots.map(s => s.id), group.claimed_ids)) throw Error('Historical terminal membership differs');
    if (retired) {
      if (group.skill === 'skill-authoring' || group.disposition !== null || group.claimed_ids.length || group.slots.length || group.review_evidence.length
        || !equal(group.untouched.map(row => row.id), expected)) throw Error('Only exact untouched reservations may be retired');
    } else if (group.skill !== 'skill-authoring') throw Error('Only original skill-authoring may have consumed observations');
    else if (group.status === 'terminal_unqualified') {
      if (disposition.schema !== 'cs3-comparison-isolated-terminal-disposition/1' || disposition.candidate_qualified !== false
        || !group.claimed_ids.length || !equal(disposition.claimed_ids, group.claimed_ids) || !equal(disposition.undispatched_ids, group.undispatched_ids)
        || disposition.evidence?.reason !== 'supplied_synthetic_canary' || disposition.evidence.no_unresolved_execution_effects !== true) throw Error('Partial historical block lacks authentic terminal disposition');
    } else if (disposition.schema !== 'cs3-comparison-disposition/1' || group.claimed_ids.length !== 18 || disposition.independent_blind_readers !== 2 || disposition.readers?.length !== 2 || !group.review_evidence?.length) throw Error('Complete historical block requires actual independent reader evidence');
    for (const evidence of group.review_evidence || []) bound(evidence);
    if (!retired && group.status !== 'terminal_unqualified') {
      const expectedRefs = [path.join(disposition.review_directory, 'private-mappings.json'), ...disposition.readers.flatMap(reader => [reader.file, path.join(disposition.review_directory, 'reader-' + reader.reader + '.json')]), ...(disposition.browser_grades ? [disposition.browser_grades.path] : [])];
      if (!equal(group.review_evidence.map(r => plain(r.path)).sort(), [...new Set(expectedRefs.map(plain))].sort())) throw Error('Historical reader evidence inventory differs');
      for (const reader of disposition.readers) if (!group.review_evidence.some(r => plain(r.path) === plain(reader.file) && r.sha256 === reader.sha256)) throw Error('Historical reader receipt changed');
      require('./cs3-comparison-review.cjs').browserGrades(plan, group.plan.sha256, group.skill, json(path.join(plan.control_directory, 'result-' + group.skill + '.json')), disposition.browser_grades?.path);
    }
    for (const row of plan.runs) {
      if (all.has(row.id)) throw Error('Historical duplicate slot'); all.add(row.id);
      const base = path.join(plan.directory, row.id), retained = group.slots.find(s => s.id === row.id);
      if (!retained) {
        if (!equal(fs.readdirSync(base).sort(), ['data', 'profile.json', 'prompt.txt', 'workspace']) || fs.readdirSync(path.join(base, 'data')).length
          || sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256 || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256
          || !equal(campaign().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path)))
          || retired && !equal(prep.identity(base, ['.']), group.untouched.find(item => item.id === row.id)?.inventory)) throw Error('Historical undispatched slot changed');
        continue;
      }
      const report = json(path.join(base, 'result.json'));
      if (!equal(prep.identity(base, ['.']), retained.inventory) || sha(read(path.join(base, 'result.json'))) !== retained.result_sha256 || capture.runEvidence(base) !== retained.evidence_sha256) throw Error('Historical raw observation changed');
      const money = require('./cs3-comparison-isolated.cjs').safeReport(plan, row, report);
      if (!equal(policy.fields(money), retained.accounting) || money.attempts.length !== retained.observed_attempts) throw Error('Historical canonical money differs');
      for (const key of ['known_settled_micros', 'conservative_debit_micros', 'unresolved_liability_micros', 'unresolved_attempts']) totals[key] += money[key];
      totals.observed_attempts += money.attempts.length;
    }
  }
  if (all.size !== 90 || !equal(totals, audit.totals)) throw Error('Historical shared accounting total differs');
  return { audit_sha256: ref.sha256, manifest_sha256: audit.manifest.sha256, terminal_groups: 1, retired_slots: 72, totals };
}
function build(spec, approved) {
  bound(spec.executable, 1024 * 1024 * 1024); if (spec.executable.sha256 !== approved.executable_sha256) throw Error('Exact approved remediation executable required');
  const receipt = JSON.parse(bound(spec.build_receipt));
  if (receipt.schema !== 'cs3-document-remediation-build/1' || receipt.status !== 'passed' || receipt.exit_code !== 0
    || receipt.executable_sha256 !== spec.executable.sha256 || receipt.executable !== spec.executable.path || receipt.source_inputs_unchanged !== true
    || receipt.toolchain_unchanged !== true || receipt.qualification_build !== true || receipt.production_release !== false || receipt.provider_calls !== 0
    || receipt.tests_executed !== 0 || receipt.builder_sha256 !== sha(read(path.join(root, 'scripts/evals/cs3-document-remediation-build.ps1')))
    || !equal(JSON.parse(bound(receipt.source_manifest)), receipt.source_inputs)) throw Error('Exact successful new-source qualification build required');
  const artifact = receipt.compiler_artifact;
  if (!path.isAbsolute(receipt.target_directory || '') || artifact?.reason !== 'compiler-artifact' || artifact.target?.name !== 'vcp'
    || artifact.profile?.test !== false || !equal(artifact.features, ['qualification']) || !path.isAbsolute(artifact.executable || '') || path.resolve(artifact.executable) !== path.resolve(receipt.target_directory, 'debug/vcp.exe')) throw Error('Exact new qualification compiler artifact required');
  if (!equal(receipt.source_inputs?.scope, buildScope)) throw Error('Complete fixed build source closure required');
  const { build_attestation, ...current } = prep.identity(root, buildScope);
  if (!equal(current, receipt.source_inputs)) throw Error('Native and verifier build inputs differ from new successful build');
  return receipt;
}
function allocationClaim() { return path.join(path.dirname(campaign().claimFile(true)), 'vcp-cs3-document-remediation-allocation1.json'); }
function claimFile() { return path.join(path.dirname(campaign().claimFile(true)), 'vcp-cs3-document-remediation-campaign1.json'); }
function reserve(specFile, destination) {
  const spec = json(specFile), approved = decision(spec.decision); noSecrets(spec);
  if (!equal(Object.keys(spec).sort(), ['build_receipt', 'decision', 'executable', 'prior_terminal', 'runtime_decision'])) throw Error('Exact remediation reservation inputs required');
  require('./cs3-runtime-amendment.cjs').decision(spec.runtime_decision);
  const terminal = priorTerminal(spec.prior_terminal, approved); build(spec, approved);
  const directory = plain(path.resolve(destination)); if (within(root, directory) || within(directory, root) || fs.existsSync(directory)) throw Error('New private reservation directory required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const receipt = { schema: 'cs3-document-remediation-allocation/2', directory, spec, terminal, cap_micros: 22450000, request_ceiling: 594, preserved_prior_allocation_micros: 66613737, combined_cap_micros: 89063737, transferred_cap_micros: 43200000, transferred_requests: 1152, model_calls: 0 };
  const ref = { path: path.join(directory, 'allocation.json'), sha256: sha(JSON.stringify(receipt, null, 2) + '\n') };
  write(allocationClaim(), { allocation: ref }); fs.mkdirSync(directory, { mode: 0o700 }); write(ref.path, receipt); return ref;
}
function validateAllocation(spec) {
  const approved = decision(spec.remediation.decision), allocation = JSON.parse(bound(spec.remediation.allocation));
  if (allocation.schema !== 'cs3-document-remediation-allocation/2' || allocation.cap_micros !== 22450000 || allocation.request_ceiling !== 594
    || allocation.preserved_prior_allocation_micros !== 66613737 || allocation.combined_cap_micros !== 89063737 || allocation.transferred_cap_micros !== 43200000 || allocation.transferred_requests !== 1152
    || !equal(allocation.spec, { decision: spec.remediation.decision, prior_terminal: spec.remediation.prior_terminal, runtime_decision: spec.remediation.runtime_decision, executable: spec.executable, build_receipt: spec.build_receipt })
    || !equal(json(allocationClaim()), { allocation: spec.remediation.allocation })
    || plain(path.dirname(spec.remediation.allocation.path)) !== allocation.directory) throw Error('Separate one-shot remediation allocation required');
  if (!equal(priorTerminal(spec.remediation.prior_terminal, approved), allocation.terminal)) throw Error('Terminal predecessor proof changed');
  require('./cs3-runtime-amendment.cjs').decision(spec.remediation.runtime_decision);
  build(spec, approved); return approved;
}
function validateSpec(spec) {
  if (!equal(Object.keys(spec).sort(), ['build_receipt', 'catalog', 'executable', 'gates', 'node', 'profile', 'remediation', 'web_evidence'])
    || !equal(Object.keys(spec.remediation).sort(), ['allocation', 'decision', 'prior_terminal', 'qualification', 'runtime_decision', 'runtime_preflight', 'runtime_terminal'])) throw Error('Exact fixed remediation specification required');
  const approved = validateAllocation(spec), profile = JSON.parse(bound(spec.profile));
  if (profile.deadline_seconds !== 600 || profile.provider_timeout_seconds !== 120 || profile.max_requests !== 16 || profile.output_tokens !== '2048' || profile.max_transport_retries !== 0) throw Error('Fixed prospective profile bounds required');
  require('./cs3-read-preflight.cjs').validateQualification(spec.remediation.qualification, spec);
  require('./cs3-document-remediation-preflight.cjs').validate(spec.remediation.runtime_preflight, spec);
  const runtime = require('./cs3-runtime-amendment.cjs').terminal(spec.remediation.runtime_terminal, spec);
  return { decision_sha256: spec.remediation.decision.sha256, allocation_sha256: spec.remediation.allocation.sha256, runtime, accounting: { fixed_conservative_micros: 78263737, outer_cap_micros: 100000000 }, approved };
}
function describe(spec, directory, checkExpiry = true) {
  noSecrets(spec); const approved = validateSpec(spec);
  return { ...executionPlan(spec, directory, tasks(), candidateRegistry, checkExpiry), schema: 'cs3-document-remediation-plan/1', remediation: approved, limits };
}
function executionPlan(spec, directory, all, registry, checkExpiry = true) {
  const bytes = bound(spec.executable, 1024 * 1024 * 1024);
  const assetsRoot = path.join(path.dirname(spec.executable.path), 'skills/builtin'), assets = inspectAssets(assetsRoot).inventory;
  requireEmbeddedCatalog(bytes, read(path.join(assetsRoot, 'catalog.json')));
  const profile = JSON.parse(bound(spec.profile)), catalogBytes = bound(spec.catalog); if (profile.provider.raw_sha256 !== sha(catalogBytes)) throw Error('Catalog/profile differs');
  const gates = require('./cs3-comparison-gates.cjs').validate(spec); bound(spec.node, 128 * 1024 * 1024);
  const runs = [];
  for (const [index, task] of all.entries()) for (let offset = 0; offset < 3; offset++) {
    const arm = campaign().arms[(index + offset) % 3], id = task.id + '--' + arm;
    runs.push({ id, case_id: task.id, skill: task.skill, arm, cap_micros: 600000, call_ceiling: 16,
      skills: arm === 'none' ? [] : arm === 'candidate' ? [registry.qualified(task.skill)] : task.nearest.map(id => `vcp-builtin::${id}::${id}`),
      profile_sha256: sha(JSON.stringify(campaign().profile(spec, task, path.join(directory, id, 'workspace'), arm, checkExpiry), null, 2) + '\n'), prompt_sha256: sha(campaign().prompt(task)),
      files: Object.entries(task.files).map(([path, content]) => ({ path, sha256: sha(content), bytes: Buffer.byteLength(content) })) });
  }
  return { directory, spec, model_calls: 0, authorization: false,
    source: campaign().sourceIdentity(), executable: spec.executable.path, assets, candidate_assets: registry.inspect(), task_sha256: sha(JSON.stringify(all)), gates, runs,
    budget_preflight: prep.budgetPreflight(profile, 600000), toolchain: { platform: process.platform, architecture: process.arch, node_version: process.version, node_executable: fs.realpathSync(process.execPath), node_sha256: sha(read(fs.realpathSync(process.execPath), 128 * 1024 * 1024)) },
    phase_rule: 'Exactly eighteen fresh slots; one shot each. Safe provider-only uncertainty debits the full slot, actual remains null and quality fails. Any integrity, authority or secret failure is terminal. No retry or replay.',
    benefit_rule: 'Every candidate hard gate and both independent readers must pass; both readers identify the same normal task with usefulness at least one above both baselines, completeness and clarity no lower.',
    paid_exclusions: ['retries', 'replays', 'confirmations', 'graders', 'readers', 'adjudication'] };
}
function prepare(specFile, destination, dryRun = false) {
  const directory = plain(path.resolve(destination));
  if (within(root, directory) || within(directory, root) || fs.existsSync(directory) || fs.existsSync(claimFile())) throw Error('New private one-shot DOC campaign required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const spec = json(specFile), plan = describe(spec, directory); campaign().qualificationWindow(JSON.parse(bound(spec.profile)));
  const ref = { path: path.join(directory, 'plan.json'), sha256: sha(JSON.stringify(plan, null, 2) + '\n') };
  if (dryRun) return { status: 'validated_not_claimed', ...ref, runs: 18, model_calls: 0 };
  write(claimFile(), { plan: ref, allocation: spec.remediation.allocation }); fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'claims'));
  for (const row of plan.runs) {
    const task = tasks().find(t => t.id === row.case_id), base = path.join(directory, row.id);
    fs.mkdirSync(path.join(base, 'workspace'), { recursive: true }); fs.mkdirSync(path.join(base, 'data'));
    for (const [relative, content] of Object.entries(task.files)) { const target = safeChild(path.join(base, 'workspace'), relative); fs.mkdirSync(path.dirname(target), { recursive: true }); write(target, content); }
    write(path.join(base, 'prompt.txt'), campaign().prompt(task)); write(path.join(base, 'profile.json'), campaign().profile(spec, task, path.join(base, 'workspace'), row.arm));
  }
  write(ref.path, plan); return { ...ref, plan: ref.path, runs: 18, model_calls: 0 };
}
function validate(plan, hash) {
  if (plan.schema !== 'cs3-document-remediation-plan/1' || sha(read(path.join(plan.directory, 'plan.json'))) !== hash
    || !equal(json(claimFile()), { plan: { path: path.join(plan.directory, 'plan.json'), sha256: hash }, allocation: plan.spec.remediation.allocation })
    || !equal(plan, describe(plan.spec, plan.directory, false))) throw Error('Exact frozen remediation ownership, assignments or prerequisites differ');
  const claimed = fs.readdirSync(path.join(plan.directory, 'claims')).filter(name => name !== 'block-document-authoring.json');
  if (!equal(claimed.sort(), plan.runs.slice(0, claimed.length).map(r => r.id + '.json').sort())) throw Error('Remediation claims are not the exact one-shot prefix');
  const owner = { plan_sha256: hash, skill: 'document-authoring' }, block = path.join(plan.directory, 'claims/block-document-authoring.json'), active = path.join(plan.directory, 'active-block.json');
  if ((claimed.length || fs.existsSync(active)) && !fs.existsSync(block) || fs.existsSync(block) && !equal(json(block), owner)
    || fs.existsSync(active) && !equal(json(active), owner)) throw Error('Exact remediation block and active ownership required');
  for (const [index, row] of plan.runs.slice(0, claimed.length).entries()) {
    if (!equal(json(path.join(plan.directory, 'claims', row.id + '.json')), { plan_sha256: hash, id: row.id })) throw Error('Remediation slot claim changed');
    const result = path.join(plan.directory, row.id, 'result.json');
    if (fs.existsSync(result)) require('./cs3-comparison-isolated.cjs').safeReport(plan, row, json(result));
    else if (index !== claimed.length - 1 || !equal(json(path.join(plan.directory, 'active-block.json')), { plan_sha256: hash, skill: 'document-authoring' })) throw Error('Incomplete claimed observation requires exact active owner');
  }
  return plan;
}
function admission(plan) {
  const sum = { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 };
  for (const row of plan.runs) if (campaign().claimed(plan, row.id)) {
    const report = campaign().slotReport(plan, row.id), money = require('./cs3-comparison-isolated.cjs').safeReport(plan, row, report);
    for (const key of ['known_settled_micros', 'conservative_debit_micros', 'unresolved_liability_micros', 'unresolved_attempts']) sum[key] += money[key]; sum.observed_attempts += money.attempts.length;
  }
  if (sum.conservative_debit_micros + 600000 > 10800000 || sum.observed_attempts + 16 > 288 || 78263737 + sum.conservative_debit_micros + 600000 > 89063737) throw Error('New DOC allocation cannot reserve another slot');
  return { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros, reserved_micros: 600000, reserved_requests: 16 };
}
module.exports = { limits, buildScope, bound, reference, decision, tasks, candidateRegistry, priorTerminal, build, allocationClaim, claimFile, reserve, validateAllocation, validateSpec, executionPlan, describe, prepare, validate, admission };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); const result = command === 'reserve' ? reserve(...args) : command === 'prepare' ? prepare(...args) : command === 'dry-run' ? prepare(...args, true) : (() => { throw Error('Usage: reserve SPEC NEW_PRIVATE_DIRECTORY | prepare SPEC NEW_PRIVATE_DIRECTORY | dry-run SPEC NEW_PRIVATE_DIRECTORY'); })(); process.stdout.write(JSON.stringify(result, null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
