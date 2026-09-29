// SPDX-License-Identifier: Apache-2.0
'use strict';
// Fixed one-to-one transfer of ninety pristine assignments. No provider fallback,
// historical regrade, allocation release, or replay is performed by this module.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const doc = require('./cs3-document-remediation.cjs'), prep = require('./authoring-prepare.cjs');
const original = require('./cs3-comparison-candidates.cjs');
const isolation = require('./cs3-comparison-isolated.cjs'), capture = require('./developer-runner.cjs');
const { read, write, plain, within, safeChild, privateDirectory, noParentInstructions, noSecrets } = require('./p6-live-runner.cjs').boundaries;
const core = () => require('./cs3-comparison.cjs'), root = path.resolve(__dirname, '../..');
const sha = value => crypto.createHash('sha256').update(value).digest('hex'), json = file => JSON.parse(read(file)), bound = ref => JSON.parse(doc.bound(ref, 64 * 1024 * 1024));
const skills = ['frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing', 'document-authoring'];
const limits = Object.freeze({ runs: 90, slot_micros: 600000, slot_requests: 16, aggregate_micros: 54000000, aggregate_requests: 1440, output_tokens: '2048' });
function need(value, message) { if (!value) throw Error(message); }
function baseSpec(spec) { const { friendli_transfer, ...base } = spec; return base; }
function claimFile() { return path.join(path.dirname(core().claimFile(true)), 'vcp-cs3-friendli-transfer1.json'); }
function decision(ref) {
  const value = bound(ref), expected = { schema: 'cs3-friendli-transfer-decision/1', authority: 'owner_explicit_cs3_completion_within_existing_100_usd',
    model: 'deepseek/deepseek-v3.2', endpoint: 'friendli', skills, runs: 90, slot_cap_micros: 600000, slot_requests: 16,
    aggregate_micros: 54000000, aggregate_requests: 1440, fixed_conservative_micros: 44913737, fixed_reserved_requests: 1191,
    combined_cap_micros: 98913737, combined_request_ceiling: 2631, outer_cap_micros: 100000000,
    retirement_audit_sha256: value.retirement_audit_sha256, qualification: 'shared_exact_funded_skl_pair_and_preflight_no_new_allowance',
    consumed_liabilities_released: false, qualification_waiver: false };
  need(equal(value, expected) && equal(value, json(path.join(root, 'src/evals/skills/cs3-friendli-transfer/decision.json')))
    && /^[a-f0-9]{64}$/.test(value.retirement_audit_sha256 || ''), 'Exact pinned ninety-slot Friendli decision required');
  return value;
}
function retired(spec) { return require('./cs3-friendli-retirement.cjs').validate(spec.friendli_transfer.retirement); }
function tasks(spec) {
  const proof = retired(spec), all = [...proof.groups.flatMap(group => group.tasks), ...proof.document.tasks];
  need(equal(proof.groups.map(group => group.skill), skills.slice(0, 4)) && all.length === 30 && new Set(all.map(task => task.id)).size === 30
    && skills.every(skill => all.filter(task => task.skill === skill).length === 6), 'Exact thirty untouched tasks required');
  return structuredClone(all);
}
const registryFor = id => id === 'document-authoring' ? doc.candidateRegistry : original;
const candidateRegistry = {
  qualified(id) { need(skills.includes(id), 'Transferred candidate only'); return registryFor(id).qualified(id); },
  configuration(id) { this.qualified(id); return registryFor(id).configuration(id); },
  inspect() {
    const old = original.inspect(), newer = doc.candidateRegistry.inspect();
    return { source_id: 'vcp-cs3-friendli-transfer', path: root,
      files: skills.flatMap(id => (id === 'document-authoring' ? newer : old).files.filter(file => file.path.startsWith(id + '/'))),
      entries: skills.map(id => (id === 'document-authoring' ? newer : old).entries.find(entry => entry.id === id)) };
  }
};
function skillTerminal(spec) {
  const reference = spec.friendli_transfer.skill_terminal;
  need(equal(Object.keys(reference || {}).sort(), ['disposition', 'plan']), 'Exact fresh SKL terminal references required');
  const plan = bound(reference.plan), receipt = bound(reference.disposition), review = require('./cs3-comparison-review.cjs');
  need(plan.schema === 'cs3-skill-remediation-plan/1' && equal(plan.spec, baseSpec(spec)), 'Transfer must share exact funded fresh SKL specification');
  const partial = receipt.schema === 'cs3-skill-remediation-terminal-disposition/1';
  const actual = partial ? review.validateTerminalDisposition(reference.plan.path, reference.plan.sha256)
    : review.validateDisposition(reference.plan.path, reference.plan.sha256, 'skill-authoring');
  need(equal(actual, receipt) && plain(reference.disposition.path) === plain(path.join(core().controlDirectory(plan), (partial ? 'terminal-disposition-' : 'disposition-') + 'skill-authoring.json')), 'Actual fresh SKL terminal disposition required');
  return { plan: reference.plan, disposition: reference.disposition, status: actual.status };
}
function validateSpec(spec) {
  need(equal(Object.keys(spec.friendli_transfer || {}).sort(), ['decision', 'retirement', 'skill_terminal']), 'Exact Friendli transfer selector required');
  const approved = decision(spec.friendli_transfer.decision);
  need(spec.friendli_transfer.retirement.sha256 === approved.retirement_audit_sha256, 'Retirement audit is not approved');
  require('./cs3-skill-remediation.cjs').validateSpec(baseSpec(spec));
  const proof = retired(spec), terminal = skillTerminal(spec);
  need(proof.groups.length === 4 && proof.document.runs.length === 18, 'Authenticated seventy-two plus eighteen retirement required');
  return { decision_sha256: spec.friendli_transfer.decision.sha256, retirement_sha256: spec.friendli_transfer.retirement.sha256, terminal };
}
function protectedDestination(spec, directory, proof = retired(spec)) {
  const roots = [root], collect = value => {
    if (!value || typeof value !== 'object') return;
    for (const [key, item] of Object.entries(value)) {
      if (typeof item === 'string' && ['directory', 'control_directory', 'oldRoot', 'historical_root', 'archive', 'source_archive'].includes(key) && path.isAbsolute(item)) roots.push(item);
      else if (item && typeof item === 'object') collect(item);
    }
  };
  collect(proof);
  const plan = bound(spec.friendli_transfer.skill_terminal.plan), receipt = bound(spec.friendli_transfer.skill_terminal.disposition);
  collect(plan); if (receipt.review_directory) roots.push(receipt.review_directory);
  const history = bound(spec.skill_remediation.history);
  collect(history); if (history.archive) collect(bound(history.archive));
  for (const candidate of roots) need(!within(plain(candidate), directory) && !within(directory, plain(candidate)), 'Transfer destination overlaps protected source or evidence');
  return roots;
}
function compareAssets(actual, expected, skill) {
  need(equal(actual.entries.find(entry => entry.id === skill), expected.entries.find(entry => entry.id === skill))
    && equal(actual.files.filter(file => file.path.startsWith(skill + '/')), expected.files.filter(file => file.path.startsWith(skill + '/'))), 'Transferred candidate bytes or identity changed');
}
function describe(spec, destination, checkExpiry = true) {
  noSecrets(spec); const approval = validateSpec(spec), proof = retired(spec), directory = plain(path.resolve(destination)), all = tasks(spec);
  protectedDestination(spec, directory, proof);
  const base = doc.executionPlan(spec, path.join(directory, 'slots'), all, candidateRegistry, checkExpiry), mapping = [], ordered = [];
  for (const skill of skills) {
    const prior = skill === 'document-authoring' ? proof.document : proof.groups.find(group => group.skill === skill);
    const rows = prior.plan?.runs || prior.runs, assets = prior.plan?.candidate_assets || prior.candidate_assets;
    compareAssets(base.candidate_assets, assets, skill);
    const generated = base.runs.filter(row => row.skill === skill);
    need(rows.length === 18 && equal(generated.map(row => row.id).sort(), rows.map(row => row.id).sort()), 'Exact original eighteen-slot membership required');
    for (const old of rows) {
      const next = generated.find(row => row.id === old.id);
      // DOC has never had a materialized plan/profile. Its authenticated source
      // supplies the exact ordered assignment, prompt and file identities.
      const fields = ['id', 'case_id', 'skill', 'arm', 'cap_micros', 'call_ceiling', 'skills', 'prompt_sha256', 'files'];
      need(fields.every(key => equal(old[key], next[key])) && (!old.prompt || sha(old.prompt) === next.prompt_sha256), 'Original assignment changed beyond the new profile/runtime');
      mapping.push({ id: next.id, original_plan: prior.plan_ref || null, original_profile_sha256: old.profile_sha256 || null, new_profile_sha256: next.profile_sha256 }); ordered.push(next);
    }
  }
  need(ordered.length === 90 && new Set(ordered.map(row => row.id)).size === 90, 'Exactly ninety transferred assignments required'); base.runs = ordered;
  return { schema: 'cs3-friendli-transfer/1', directory, spec, source: base.source, base, approval, mapping,
    groups: skills.map(skill => ({ skill, ids: ordered.filter(row => row.skill === skill).map(row => row.id) })), model_calls: 0 };
}
function project(manifest, reference, skill) {
  need(skills.includes(skill), 'Unknown transferred skill');
  return { ...manifest.base, schema: 'cs3-friendli-transfer-plan/1', limits, control_directory: path.join(manifest.directory, skill), runs: manifest.base.runs.filter(row => row.skill === skill),
    friendli_transfer: { manifest: reference, skill, accounting: { fixed_conservative_micros: 44913737, outer_cap_micros: 100000000 } },
    phase_rule: 'Five serial one-shot transferred groups, after actual fresh SKL terminal. Safe provider uncertainty costs the full slot and fails quality. Supplied model canary ends only its skill; execution or evidence integrity halts all groups. DOC follows four actual dispositions. No replay.' };
}
function prepare(specFile, destination, dryRun = false) {
  const directory = plain(path.resolve(destination));
  need(!within(root, directory) && !within(directory, root) && !fs.existsSync(directory) && !fs.existsSync(claimFile()), 'New private one-shot transfer directory and claim required');
  privateDirectory(directory); noParentInstructions(path.dirname(directory));
  const manifest = describe(json(specFile), directory), bytes = JSON.stringify(manifest, null, 2) + '\n', reference = { path: path.join(directory, 'manifest.json'), sha256: sha(bytes) };
  const plans = skills.map(skill => project(manifest, reference, skill)), all = tasks(manifest.spec);
  core().validateExecution(plans[0]); core().qualificationWindow(bound(manifest.spec.profile));
  if (dryRun) return { status: 'validated_not_claimed', manifest_sha256: reference.sha256, transferred: 90, groups: 5, model_calls: 0 };
  write(claimFile(), { manifest: reference, retirement: manifest.spec.friendli_transfer.retirement, allocation: manifest.spec.skill_remediation.allocation });
  fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'transitions')); write(reference.path, bytes);
  for (const row of manifest.base.runs) {
    const base = path.join(manifest.base.directory, row.id), task = all.find(task => task.id === row.case_id);
    fs.mkdirSync(path.join(base, 'workspace'), { recursive: true }); fs.mkdirSync(path.join(base, 'data'));
    for (const [relative, content] of Object.entries(task.files)) { const file = safeChild(path.join(base, 'workspace'), relative); fs.mkdirSync(path.dirname(file), { recursive: true }); write(file, content); }
    write(path.join(base, 'prompt.txt'), core().prompt(task)); write(path.join(base, 'profile.json'), core().profile(manifest.spec, task, path.join(base, 'workspace'), row.arm));
  }
  const prepared = plans.map(plan => { fs.mkdirSync(plan.control_directory); fs.mkdirSync(path.join(plan.control_directory, 'claims')); const file = path.join(plan.control_directory, 'plan.json'); write(file, plan); return { skill: plan.friendli_transfer.skill, path: file, sha256: sha(read(file)) }; });
  return { manifest: reference, plans: prepared, model_calls: 0 };
}
function load(plan) {
  const reference = plan.friendli_transfer?.manifest, manifest = bound(reference);
  need(manifest.schema === 'cs3-friendli-transfer/1' && plain(path.dirname(reference.path)) === manifest.directory
    && equal(json(claimFile()), { manifest: reference, retirement: manifest.spec.friendli_transfer.retirement, allocation: manifest.spec.skill_remediation.allocation }), 'Exact shared transfer ownership required');
  privateDirectory(manifest.directory); noParentInstructions(manifest.directory);
  need(!fs.existsSync(path.join(manifest.directory, 'global-halt.json')), 'Shared transfer execution integrity halted'); return manifest;
}
function groups(manifest, reference) {
  return skills.map(skill => {
    const plan = project(manifest, reference, skill), file = path.join(plan.control_directory, 'plan.json'), hash = sha(read(file));
    need(equal(json(file), plan), 'Frozen transfer plan changed');
    const names = fs.readdirSync(path.join(plan.control_directory, 'claims')).sort(), slotNames = names.filter(name => name !== 'block-' + skill + '.json');
    need(equal(slotNames, plan.runs.slice(0, slotNames.length).map(row => row.id + '.json').sort()) && slotNames.length <= 18
      && (!slotNames.length || names.includes('block-' + skill + '.json')), 'Transfer claims must be exact one-shot prefix');
    for (const name of names) need(equal(json(path.join(plan.control_directory, 'claims', name)), name === 'block-' + skill + '.json' ? { plan_sha256: hash, skill } : { plan_sha256: hash, id: name.slice(0, -5) }), 'Transfer claim identity changed');
    const active = path.join(plan.control_directory, 'active-block.json');
    if (fs.existsSync(active)) need(names.includes('block-' + skill + '.json') && equal(json(active), { plan_sha256: hash, skill }), 'Transfer active block changed');
    for (const row of plan.runs.slice(slotNames.length)) require('./cs3-comparison-segment.cjs').pristine(plan, row);
    return { plan, file, hash, claimed: plan.runs.slice(0, slotNames.length), begun: names.includes('block-' + skill + '.json') };
  });
}
function controlInventory(manifest) {
  const entries = fs.readdirSync(manifest.directory).sort(), expected = ['manifest.json', 'slots', 'transitions', ...skills, ...(fs.existsSync(path.join(manifest.directory, 'active-skill.json')) ? ['active-skill.json'] : [])].sort();
  need(equal(entries, expected) && equal(fs.readdirSync(manifest.base.directory).sort(), manifest.base.runs.map(row => row.id).sort()), 'Exact transfer controls and ninety-slot inventory required');
  return prep.identity(manifest.directory, entries.filter(name => name !== 'slots'));
}
function owners(manifest, reference, all) {
  const names = fs.readdirSync(path.join(manifest.directory, 'transitions')).sort(); let owner = null;
  need(names.length <= 5 && equal(names, names.map((_, i) => i + '.json')), 'Exact serial transfer transitions required');
  for (const [ordinal, name] of names.entries()) {
    const group = all[ordinal], to = { skill: skills[ordinal], plan_sha256: group.hash, manifest_sha256: reference.sha256 };
    need(equal(json(path.join(manifest.directory, 'transitions', name)), { manifest_sha256: reference.sha256, ordinal, from: owner, to }), 'Transfer transition identity changed'); owner = to;
  }
  const active = path.join(manifest.directory, 'active-skill.json');
  need(equal(fs.existsSync(active) ? json(active) : null, owner) && all.every((group, i) => !group.begun || i < names.length), 'Shared transfer owner/claim differs');
}
function validate(plan, hash) {
  const manifest = load(plan), reference = plan.friendli_transfer.manifest;
  need(equal(manifest, describe(manifest.spec, manifest.directory, false)) && equal(plan, project(manifest, reference, plan.friendli_transfer.skill)), 'Frozen transferred source or prerequisites changed');
  const all = groups(manifest, reference); need(all.find(group => group.plan.friendli_transfer.skill === plan.friendli_transfer.skill)?.hash === hash, 'Exact transfer plan hash required');
  owners(manifest, reference, all); controlInventory(manifest);
  for (const row of manifest.base.runs) { const base = path.join(manifest.base.directory, row.id);
    need(sha(read(path.join(base, 'profile.json'))) === row.profile_sha256 && sha(read(path.join(base, 'prompt.txt'))) === row.prompt_sha256
      && equal(core().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path))), 'Frozen transfer input bytes changed'); }
  for (const group of all) for (const [index, row] of group.claimed.entries()) {
    const file = core().reportFile(group.plan, row.id);
    if (fs.existsSync(file)) isolation.safeReport(group.plan, row, json(file));
    else need(index === group.claimed.length - 1 && equal(json(path.join(group.plan.control_directory, 'active-block.json')), { plan_sha256: group.hash, skill: group.plan.friendli_transfer.skill }), 'Incomplete transfer lacks exact active owner');
  }
  return plan;
}
const totals = () => ({ known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 });
function add(sum, money) { for (const key of Object.keys(sum)) sum[key] += key === 'observed_attempts' ? money.attempts.length : money[key]; }
function admission(plan) {
  const manifest = load(plan), sum = totals();
  for (const group of groups(manifest, plan.friendli_transfer.manifest)) for (const row of group.claimed) add(sum, isolation.safeReport(group.plan, row, core().slotReport(group.plan, row.id)));
  need(Object.values(sum).every(value => Number.isSafeInteger(value) && value >= 0) && sum.conservative_debit_micros + 600000 <= 54000000 && sum.observed_attempts + 16 <= 1440
    && 44913737 + sum.conservative_debit_micros + 600000 <= 98913737, 'Shared transferred allocation cannot reserve another slot');
  return { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros, reserved_micros: 600000, reserved_requests: 16 };
}
function disposition(group) {
  const skill = group.plan.friendli_transfer.skill, review = require('./cs3-comparison-review.cjs');
  return fs.existsSync(path.join(group.plan.control_directory, 'terminal-disposition-' + skill + '.json'))
    ? review.validateTerminalDisposition(group.file, group.hash) : review.validateDisposition(group.file, group.hash, skill);
}
function begin(plan, hash) {
  validate(plan, hash); const manifest = load(plan), reference = plan.friendli_transfer.manifest, all = groups(manifest, reference), ordinal = all.filter(group => group.begun).length;
  need(all[ordinal]?.plan.friendli_transfer.skill === plan.friendli_transfer.skill, 'Fixed transfer skill order or one-shot claim consumed');
  for (const group of all.slice(0, ordinal)) disposition(group);
  const file = path.join(manifest.directory, 'active-skill.json'), from = fs.existsSync(file) ? json(file) : null, to = { skill: plan.friendli_transfer.skill, plan_sha256: hash, manifest_sha256: reference.sha256 };
  write(path.join(manifest.directory, 'transitions', ordinal + '.json'), { manifest_sha256: reference.sha256, ordinal, from, to });
  need(equal(fs.existsSync(file) ? json(file) : null, from), 'Shared transfer owner changed during exclusive handoff');
  if (from) fs.unlinkSync(file); write(file, to);
}
function assertActive(plan, hash) { const manifest = load(plan); need(equal(json(path.join(manifest.directory, 'active-skill.json')), { skill: plan.friendli_transfer.skill, plan_sha256: hash, manifest_sha256: plan.friendli_transfer.manifest.sha256 }), 'Shared active transfer ownership changed'); }
function globalHalt(plan, hash, reason) {
  const manifest = bound(plan.friendli_transfer.manifest), file = path.join(manifest.directory, 'global-halt.json');
  if (!fs.existsSync(file)) write(file, { schema: 'cs3-friendli-transfer-halt/1', manifest_sha256: plan.friendli_transfer.manifest.sha256, plan_sha256: hash, skill: plan.friendli_transfer.skill, reason, action: 'All transfer dispatch stopped; no replay.' });
}
function canary(plan, row) {
  const task = tasks(plan.spec).find(task => task.id === row.case_id), literals = [...(task.forbidden_literals || []), `CS3_${task.id.split('-')[0]}_PRIVATE_CANARY_82D4`].filter(literal => Object.values(task.files).some(content => content.includes(literal)));
  if (!literals.length) return false;
  const base = path.join(plan.directory, row.id);
  return json(path.join(base, 'outputs.json')).flatMap(page => page.items).filter(item => item.collection === 'artifact' && item.record?.spec?.channel === 'response').some(item => {
    const bytes = read(path.join(base, 'response-' + sha(item.id) + '.sse'), 1024 * 1024);
    need(sha(bytes) === item.record.sha256 && bytes.length === Number(item.record.length), 'Canonical canary response changed');
    return isolation.responseCanary(bytes.toString('utf8'), literals);
  });
}
function failure(plan, hash, row, report, error) {
  try {
    need(error.message === 'Synthetic canary disclosed in canonical output' && report.accounted === true, 'Not a supplied-canary observation');
    validate(plan, hash); assertActive(plan, hash); core().validateExecution(plan, false); isolation.safeReport(plan, row, report, false); need(canary(plan, row), 'No authenticated explicitly supplied canary');
    report.evidence_sha256 = capture.runEvidence(path.join(plan.directory, row.id)); report.local_failure = 'supplied_synthetic_canary';
    return { failure_scope: 'skill', failure_kind: 'supplied_synthetic_canary', failing_arm: row.arm };
  } catch (error2) { globalHalt(plan, hash, error.message + '; ' + error2.message); return { failure_scope: 'global', failure_kind: 'execution_integrity' }; }
}
function terminalProof(plan, hash, partial) {
  validate(plan, hash); const skill = plan.friendli_transfer.skill, resultFile = path.join(plan.control_directory, 'result-' + skill + '.json'), result = json(resultFile), count = result.runs.length;
  need(result.plan_sha256 === hash && result.skill === skill && count > 0 && count <= 18 && (partial || count === 18 && !result.stopped), 'Authenticated transfer terminal inventory required');
  const group = groups(load(plan), plan.friendli_transfer.manifest).find(group => group.plan.friendli_transfer.skill === skill), sum = totals(), reports = [];
  need(equal(group.claimed.map(row => row.id), result.runs.map(row => row.id)), 'Terminal reports differ from exact claimed prefix');
  for (const [index, report] of result.runs.entries()) { const row = plan.runs[index]; need(equal(core().slotReport(plan, row.id), report), 'Transfer aggregate report changed'); add(sum, isolation.safeReport(plan, row, report)); reports.push({ id: row.id, result_sha256: sha(read(core().reportFile(plan, row.id))), evidence_sha256: report.evidence_sha256 }); }
  need(result.actual_cost_micros === (sum.unresolved_attempts ? null : sum.known_settled_micros) && Object.keys(sum).every(key => result[key] === sum[key]), 'Transfer terminal totals differ');
  let haltHash = null;
  if (partial) { const file = path.join(plan.control_directory, 'halt.json'), halt = json(file), last = result.runs.at(-1);
    need(result.stopped && halt.plan_sha256 === hash && halt.slot === last.id && halt.failure_scope === 'skill' && halt.failure_kind === 'supplied_synthetic_canary'
      && halt.failing_arm === last.arm && halt.reason === 'Synthetic canary disclosed in canonical output' && last.status === 'failed' && last.local_failure === 'supplied_synthetic_canary' && canary(plan, plan.runs[count - 1]), 'Only authenticated supplied-canary failure is locally terminal'); haltHash = sha(read(file)); }
  return { skill, plan_sha256: hash, claimed_ids: group.claimed.map(row => row.id), halt_sha256: haltHash, block_result_sha256: sha(read(resultFile)), reports,
    accounting: { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros }, reason: partial ? 'supplied_synthetic_canary' : 'complete_safe_native_observations', no_unresolved_execution_effects: true };
}
function validateTerminal(plan, hash) { return terminalProof(plan, hash, true); }
function validateReaderTerminal(plan, hash) { return terminalProof(plan, hash, false); }
module.exports = { baseSpec, decision, tasks, candidateRegistry, skillTerminal, validateSpec, protectedDestination, describe, project, prepare, load, groups, controlInventory, validate,
  admission, begin, assertActive, failure, globalHalt, validateTerminal, validateReaderTerminal, claimFile, skills, limits };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); need(['prepare', 'dry-run'].includes(command), 'Usage: prepare|dry-run SPEC NEW_PRIVATE_ROOT'); process.stdout.write(JSON.stringify(prepare(...args, command === 'dry-run'), null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
