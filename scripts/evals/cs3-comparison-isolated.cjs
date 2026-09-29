// SPDX-License-Identifier: Apache-2.0
'use strict';
// Five independently terminal skill groups share one immutable ownership and budget.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prep = require('./authoring-prepare.cjs'), policy = require('./cs3-comparison-policy.cjs'), capture = require('./developer-runner.cjs');
const prior = require('./p6-live-runner.cjs');
const { read, write, plain, within, privateDirectory, noParentInstructions, frames } = prior.boundaries;
const root = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const skills = ['skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
const allowedChanges = ['scripts/evals/cs3-comparison.cjs', 'scripts/evals/cs3-comparison-review.cjs', 'scripts/evals/cs3-comparison-host.test.cjs',
  'scripts/evals/cs3-comparison-isolated.cjs', 'scripts/evals/cs3-comparison-isolated.test.cjs', 'scripts/evals/cs3-comparison-review-terminal.test.cjs',
  'src/evals/skills/cs3-comparison/isolation-decision.json'];
const campaign = () => require('./cs3-comparison.cjs');
const json = file => JSON.parse(read(file));
function bound(ref) {
  if (!ref || !path.isAbsolute(ref.path || '') || !/^[a-f0-9]{64}$/.test(ref.sha256 || '')) throw Error('Exact isolated evidence reference required');
  const bytes = read(ref.path, 32 * 1024 * 1024);
  if (sha(bytes) !== ref.sha256) throw Error('Isolated bound evidence changed');
  return JSON.parse(bytes);
}
function claimFile() { return path.join(path.dirname(campaign().claimFile(true)), 'vcp-cs3-deepseek-20260928-isolation1-claim.json'); }
function totals() { return { known_settled_micros: 0, conservative_debit_micros: 0, unresolved_liability_micros: 0, unresolved_attempts: 0, observed_attempts: 0 }; }
function add(total, money) {
  for (const key of Object.keys(total)) total[key] += key === 'observed_attempts' ? money.attempts.length : money[key];
  if (Object.values(total).some(value => !Number.isSafeInteger(value) || value < 0)) throw Error('Shared accounting bound');
}
function historical(spec) {
  if (!spec || !equal(Object.keys(spec).sort(), ['audit', 'decision', 'predecessor'])) throw Error('Exact isolation decision/predecessor/audit required');
  const decision = bound(spec.decision), tracked = require('../../src/evals/skills/cs3-comparison/isolation-decision.json');
  if (!equal(decision, tracked) || decision.schema !== 'cs3-comparison-isolation-decision/1'
    || decision.authority !== 'owner_explicit_isolated_continuation_and_cs3_completion' || !equal(decision.skills, skills)
    || decision.retained_consumed_slots !== 11 || decision.excluded_untouched_terminal_skill_slots !== 7 || decision.untouched_slots !== 90
    || decision.existing_campaign_cap_micros !== 64800000 || decision.existing_campaign_requests !== 1728 || decision.outer_cap_micros !== 100000000
    || decision.additional_allocation_micros !== 0 || decision.completion_or_promotion_waiver !== false
    || decision.model_output_security_failure !== 'terminal_unqualified_for_own_skill_never_replayed_with_failing_arm_recorded') throw Error('Source-pinned isolation approval differs');
  if (spec.predecessor.sha256 !== decision.predecessor_plan_sha256 || spec.audit.sha256 !== decision.predecessor_audit_sha256) throw Error('Exact isolated predecessor commitments required');
  const previous = bound(spec.predecessor), audit = bound(spec.audit), control = previous.control_directory;
  if (previous.schema !== 'cs3-comparison-segment-plan/1' || previous.runs.length !== 108 || path.resolve(path.dirname(spec.predecessor.path)) !== path.resolve(control)
    || previous.source.content_sha256 !== decision.predecessor_source_identity || !path.isAbsolute(spec.predecessor.source_archive || '')
    || !equal(prep.identity(spec.predecessor.source_archive, previous.source.scope), previous.source)) throw Error('Archived terminal segment source differs');
  if (audit.schema !== 'cs3-comparison-segment-terminal-audit/1' || audit.plan_sha256 !== spec.predecessor.sha256
    || !equal(audit.consumed.map(r => r.id), previous.runs.slice(0, 11).map(r => r.id)) || !equal(audit.remaining_ids, previous.runs.slice(11).map(r => r.id))) throw Error('Exact eleven-consumed/97-untouched audit required');
  const oldClaim = require('./cs3-comparison-segment.cjs').claimFile();
  if (sha(read(oldClaim)) !== audit.claim_sha256 || !equal(json(oldClaim), { control_directory: control, original_plan_sha256: previous.segment.spec.origin.sha256, plan_sha256: spec.predecessor.sha256 })) throw Error('Terminal segment global claim differs');
  for (const [name, hash] of Object.entries({ 'halt.json': audit.halt_sha256, 'result-document-authoring.json': audit.block_sha256, 'active-block.json': audit.active_block_sha256 })) {
    if (sha(read(path.join(control, name))) !== hash) throw Error('Terminal segment control changed');
  }
  if (!equal(json(path.join(control, 'halt.json')), audit.halt) || audit.halt.reason !== 'Synthetic canary disclosed in canonical output') throw Error('Exact historical synthetic-canary halt required');
  if (!equal(fs.readdirSync(path.join(control, 'claims')).sort(), audit.claims.map(c => c.name).sort())) throw Error('Terminal segment claim inventory changed');
  for (const row of audit.claims) if (sha(read(path.join(control, 'claims', row.name))) !== row.sha256) throw Error('Terminal segment claim changed');
  // Reauthenticate the earlier two-slot control chain, not just the latest summary.
  const origin = bound(previous.segment.spec.origin), originAudit = bound(previous.segment.spec.audit);
  if (sha(read(campaign().claimFile(true))) !== originAudit.global_claim_sha256) throw Error('Original successor global claim changed');
  for (const [name, hash] of Object.entries({ 'halt.json': originAudit.halt_sha256, 'result-document-authoring.json': originAudit.block_result_sha256, 'active-block.json': originAudit.active_block_sha256 })) {
    if (sha(read(path.join(origin.directory, name))) !== hash) throw Error('Original successor control changed');
  }
  if (!equal(fs.readdirSync(path.join(origin.directory, 'claims')).sort(), originAudit.original_claims.map(c => path.basename(c.path)).sort())) throw Error('Original successor claim inventory changed');
  for (const row of originAudit.original_claims) if (sha(read(path.join(origin.directory, row.path))) !== row.sha256) throw Error('Original successor claim changed');
  if (!equal(prep.identity(previous.segment.spec.origin.source_archive, origin.source.scope), origin.source)) throw Error('Original successor source archive changed');
  const prefix = totals();
  for (const [index, audited] of audit.consumed.entries()) {
    const row = previous.runs[index], base = path.join(previous.directory, row.id), report = campaign().slotReport(previous, row.id), money = policy.reread(base, row.cap_micros);
    if (!equal(prep.identity(base, ['.']), audited.inventory) || sha(read(campaign().reportFile(previous, row.id))) !== audited.result_sha256
      || capture.runEvidence(base) !== audited.evidence_sha256 || !equal(policy.fields(money), policy.fields(audited)) || !equal(policy.fields(report), policy.fields(money))
      || money.attempts.length !== audited.observed_attempts || report.observed_attempts !== money.attempts.length || report.id !== row.id || report.status !== audited.status) throw Error('Immutable consumed historical evidence changed');
    add(prefix, money);
  }
  if (!equal(prefix, audit.totals)) throw Error('Historical shared accounting differs');
  const current = campaign().sourceIdentity();
  for (const name of new Set([...previous.source.files, ...current.files].map(f => f.path))) if (!equal(previous.source.files.find(f => f.path === name), current.files.find(f => f.path === name)) && !allowedChanges.includes(name)) throw Error('Isolation changed non-verifier source: ' + name);
  if (!equal(previous.source.directories, current.directories)) throw Error('Isolation source directory closure changed');
  const build = bound(previous.spec.build_receipt); campaign().buildProvenance(build, previous.spec.executable);
  const nativeScope = build.source_inputs.scope.filter(p => p.startsWith('src/crates') || p.startsWith('src/skills/builtin') || p.startsWith('src/third_party'));
  const native = prep.identity(root, nativeScope), selected = p => nativeScope.some(prefix => p === prefix || p.startsWith(prefix + '/'));
  if (!nativeScope.length || !equal(native.files, build.source_inputs.files.filter(f => selected(f.path))) || !equal(native.directories, build.source_inputs.directories.filter(selected))) throw Error('Unchanged native build source required');
  for (const row of previous.runs) {
    const base = path.join(previous.directory, row.id);
    if (sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256 || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256
      || !equal(campaign().workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path)))) throw Error('Frozen isolated slot inputs changed');
  }
  for (const row of previous.runs.slice(11, 18)) require('./cs3-comparison-segment.cjs').pristine(previous, row);
  return { previous, audit, decision, prefix, current };
}
function project(manifest, reference, skill, previous) {
  const { segment, ...base } = previous;
  return { ...base, schema: 'cs3-comparison-isolated-plan/1', source: manifest.source, control_directory: path.join(manifest.directory, skill),
    runs: previous.runs.filter(row => row.skill === skill), isolated: { manifest: reference, skill } };
}
function describe(spec, destination) {
  const directory = plain(path.resolve(destination));
  if (within(root, directory) || within(directory, root) || fs.existsSync(directory) || fs.existsSync(claimFile())) throw Error('New private isolation directory and one unclaimed envelope required');
  privateDirectory(directory); noParentInstructions(path.dirname(directory));
  const { previous, prefix, current } = historical(spec);
  if (within(previous.directory, directory) || within(directory, previous.directory) || within(previous.control_directory, directory) || within(directory, previous.control_directory)) throw Error('Isolation controls must be separate from historical evidence');
  for (const row of previous.runs.slice(18)) require('./cs3-comparison-segment.cjs').pristine(previous, row);
  const groups = skills.map(skill => ({ skill, ids: previous.runs.filter(row => row.skill === skill).map(row => row.id) }));
  if (groups.some(group => group.ids.length !== 18) || new Set(groups.flatMap(g => g.ids)).size !== 90) throw Error('Five disjoint exact eighteen-slot groups required');
  return { schema: 'cs3-comparison-isolation/1', directory, spec, source: current, groups, prefix, excluded_ids: previous.runs.slice(11, 18).map(row => row.id), model_calls: 0 };
}
function prepare(specFile, destination, dryRun = false) {
  const manifest = describe(json(specFile), destination), bytes = JSON.stringify(manifest, null, 2) + '\n', reference = { path: path.join(manifest.directory, 'manifest.json'), sha256: sha(bytes) };
  const previous = bound(manifest.spec.predecessor), plans = skills.map(skill => project(manifest, reference, skill, previous));
  // Projection changes only frozen row membership and control/skill identity.
  // Every field consumed by these execution prerequisites is shared exactly.
  campaign().validateExecution(plans[0]); campaign().qualificationWindow(bound(plans[0].spec.profile));
  if (dryRun) return { status: 'validated_not_claimed', manifest_sha256: reference.sha256, consumed: 11, excluded: 7, remaining: 90, groups: 5, model_calls: 0 };
  write(claimFile(), { directory: manifest.directory, manifest_sha256: reference.sha256 });
  fs.mkdirSync(manifest.directory, { mode: 0o700 }); fs.mkdirSync(path.join(manifest.directory, 'transitions')); write(reference.path, bytes);
  const prepared = [];
  for (const plan of plans) {
    fs.mkdirSync(plan.control_directory); fs.mkdirSync(path.join(plan.control_directory, 'claims'));
    const file = path.join(plan.control_directory, 'plan.json'); write(file, plan); prepared.push({ skill: plan.isolated.skill, path: file, sha256: sha(read(file)) });
  }
  return { manifest: reference, plans: prepared, model_calls: 0 };
}
function load(plan) {
  const reference = plan.isolated?.manifest, manifest = bound(reference);
  if (manifest.schema !== 'cs3-comparison-isolation/1' || path.dirname(reference.path) !== manifest.directory
    || !equal(json(claimFile()), { directory: manifest.directory, manifest_sha256: reference.sha256 })) throw Error('Exact shared isolation ownership required');
  privateDirectory(manifest.directory); noParentInstructions(manifest.directory);
  if (fs.existsSync(path.join(manifest.directory, 'global-halt.json'))) throw Error('Shared isolation execution integrity halted');
  return manifest;
}
function groups(manifest, reference, previous) {
  return skills.map(skill => { const plan = project(manifest, reference, skill, previous), file = path.join(plan.control_directory, 'plan.json');
    if (!equal(json(file), plan)) throw Error('Isolated plan changed original assignment');
    const hash = sha(read(file)), names = fs.readdirSync(path.join(plan.control_directory, 'claims')).sort(), claimed = names.filter(n => !n.startsWith('block-'));
    if (!equal(claimed, plan.runs.slice(0, claimed.length).map(r => r.id + '.json').sort()) || names.some(n => n.startsWith('block-') && n !== 'block-' + skill + '.json')
      || claimed.length && !names.includes('block-' + skill + '.json')) throw Error('Isolated claims are not a one-shot frozen prefix');
    for (const name of names) if (!equal(json(path.join(plan.control_directory, 'claims', name)), name.startsWith('block-') ? { plan_sha256: hash, skill } : { plan_sha256: hash, id: name.slice(0, -5) })) throw Error('Isolated claim identity changed');
    for (const row of plan.runs.slice(claimed.length)) require('./cs3-comparison-segment.cjs').pristine(plan, row);
    return { plan, hash, file, claimed: plan.runs.slice(0, claimed.length) };
  });
}
function validate(plan, hash) {
  const manifest = load(plan), historicalState = historical(manifest.spec), { previous, current, prefix } = historicalState;
  const expected = { schema: 'cs3-comparison-isolation/1', directory: manifest.directory, spec: manifest.spec, source: current,
    groups: skills.map(skill => ({ skill, ids: previous.runs.filter(r => r.skill === skill).map(r => r.id) })), prefix, excluded_ids: previous.runs.slice(11, 18).map(r => r.id), model_calls: 0 };
  if (!equal(manifest, expected) || !skills.includes(plan.isolated.skill) || !equal(plan, project(manifest, plan.isolated.manifest, plan.isolated.skill, previous))) throw Error('Frozen isolation/source/assignments changed');
  if (within(previous.directory, manifest.directory) || within(manifest.directory, previous.directory) || within(previous.control_directory, manifest.directory) || within(manifest.directory, previous.control_directory)) throw Error('Isolation overlaps historical data');
  const all = groups(manifest, plan.isolated.manifest, previous), currentGroup = all.find(g => g.plan.isolated.skill === plan.isolated.skill);
  if (currentGroup.hash !== hash) throw Error('Exact isolated plan hash required');
  const activeFile = path.join(manifest.directory, 'active-skill.json');
  if (fs.existsSync(activeFile)) { const active = json(activeFile), group = all.find(g => g.plan.isolated.skill === active.skill);
    if (!group || !equal(active, { skill: group.plan.isolated.skill, plan_sha256: group.hash, manifest_sha256: plan.isolated.manifest.sha256 })) throw Error('Shared active skill ownership changed'); }
  const transitions = fs.readdirSync(path.join(manifest.directory, 'transitions')).sort(), seen = new Set(); let owner = null;
  if (transitions.length > 5 || !equal(transitions, transitions.map((_, i) => i + '.json'))) throw Error('Exclusive skill transition inventory changed');
  for (const [ordinal, name] of transitions.entries()) {
    const transition = json(path.join(manifest.directory, 'transitions', name)), group = all.find(g => g.plan.isolated.skill === transition.to?.skill);
    if (!group || seen.has(group.plan.isolated.skill) || !equal(transition, { manifest_sha256: plan.isolated.manifest.sha256, ordinal, from: owner,
      to: { skill: group.plan.isolated.skill, plan_sha256: group.hash, manifest_sha256: plan.isolated.manifest.sha256 } })) throw Error('Exclusive skill transition provenance changed');
    owner = transition.to; seen.add(owner.skill);
  }
  if (!equal(fs.existsSync(activeFile) ? json(activeFile) : null, owner)
    || all.some(g => fs.existsSync(path.join(g.plan.control_directory, 'claims', 'block-' + g.plan.isolated.skill + '.json')) && !seen.has(g.plan.isolated.skill))) throw Error('Shared claim/transition ownership differs');
  return plan;
}
function safeReport(plan, row, report, requireDigest = true) {
  const base = path.join(plan.directory, row.id), money = policy.reread(base, row.cap_micros);
  if (report.id !== row.id || report.case_id !== row.case_id || report.arm !== row.arm || !equal(report.scope, money.ledger_scope) || !equal(policy.fields(report), policy.fields(money))
    || report.observed_attempts !== money.attempts.length || report.preserved !== true || requireDigest && report.evidence_sha256 !== capture.runEvidence(base)) throw Error('Isolated report/canonical accounting identity differs');
  const profile = json(path.join(base, 'profile.json'));
  if (profile.maximum_autonomy !== 'plan' || !equal(profile.automatic_effects, []) || ['processes', 'checks', 'mcp', 'mcp_http'].some(k => profile[k] !== undefined && !equal(profile[k], []))
    || profile.canonical_tools.some(t => !['vcp_list', 'vcp_read', 'vcp_search', 'vcp_verify'].includes(t))) throw Error('Isolated execution exceeds read-only authority');
  const tools = json(path.join(base, 'tools.json'));
  if (!tools.length || tools.some(p => p.gaps.length)) throw Error('Isolated tool/effect evidence incomplete');
  for (const item of tools.flatMap(p => p.items).filter(i => i.collection !== 'artifact')) {
    if (item.collection !== 'effect' || item.visibility !== 'available' || item.record.state !== 'succeeded' || !equal(item.record.scope, money.ledger_scope)
      || item.record.exit_code !== null || item.record.reason !== 'broker observed bounded file results; no automatic replay') throw Error('Isolated native effect is ambiguous or violates authority');
  }
  const outputs = json(path.join(base, 'outputs.json'));
  if (!outputs.length || outputs.some(p => p.gaps.some(g => !prior.privacyGap(g, g.artifact)))) throw Error('Isolated response evidence incomplete');
  const responses = outputs.flatMap(p => p.items).filter(i => i.collection === 'artifact' && i.record?.spec?.channel === 'response'), sent = money.attempts.filter(a => a.send_intent || a.phase === 'settled');
  if (responses.length !== sent.length) throw Error('Isolated retained responses do not cover dispatched attempts');
  if (!equal(fs.readdirSync(base).filter(name => /^response-.*\.sse$/.test(name)).sort(), responses.map(item => 'response-' + sha(item.id) + '.sse').sort())) throw Error('Unbound isolated response file inventory');
  for (const attempt of sent) {
    const selected = responses.filter(i => i.record.spec.source === 'retained-codex-attempt:' + attempt.id);
    if (selected.length !== 1 || !equal(selected[0].record.spec.scope, money.ledger_scope)) throw Error('Isolated response scope/source mismatch');
    const item = selected[0], bytes = read(path.join(base, 'response-' + sha(item.id) + '.sse'), 1024 * 1024);
    if (sha(bytes) !== item.record.sha256 || bytes.length !== Number(item.record.length) || !['complete', 'aborted'].includes(item.record.state)) throw Error('Isolated raw response bytes changed');
  }
  return money;
}
function admission(plan) {
  const manifest = load(plan), previous = bound(manifest.spec.predecessor), sum = { ...manifest.prefix };
  for (const group of groups(manifest, plan.isolated.manifest, previous)) for (const row of group.claimed) add(sum, safeReport(group.plan, row, campaign().slotReport(group.plan, row.id)));
  if (sum.conservative_debit_micros + 600000 > 64800000 || sum.observed_attempts + 16 > 1728
    || plan.successor.fixed_conservative_micros + sum.conservative_debit_micros + 600000 > 100000000) throw Error('Shared original allocation cannot reserve another slot');
  return { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros, reserved_micros: 600000, reserved_requests: 16 };
}
function begin(plan, hash) {
  validate(plan, hash);
  const manifest = load(plan), activeFile = path.join(manifest.directory, 'active-skill.json'), previous = bound(manifest.spec.predecessor), all = groups(manifest, plan.isolated.manifest, previous);
  if (fs.existsSync(path.join(plan.control_directory, 'claims', 'block-' + plan.isolated.skill + '.json'))) throw Error('Isolated skill already consumed');
  const previousOwner = fs.existsSync(activeFile) ? json(activeFile) : null;
  if (previousOwner) {
    const active = previousOwner, owner = all.find(g => g.plan.isolated.skill === active.skill);
    if (!owner || !fs.existsSync(path.join(owner.plan.control_directory, 'claims', 'block-' + active.skill + '.json'))) throw Error('Interrupted shared ownership requires reconciliation');
  }
  for (const group of all.filter(g => g.claimed.length || fs.existsSync(path.join(g.plan.control_directory, 'claims', 'block-' + g.plan.isolated.skill + '.json')))) {
    const review = require('./cs3-comparison-review.cjs');
    if (fs.existsSync(path.join(group.plan.control_directory, 'terminal-disposition-' + group.plan.isolated.skill + '.json'))) review.validateTerminalDisposition(group.file, group.hash);
    else review.validateDisposition(group.file, group.hash, group.plan.isolated.skill);
  }
  const nextOwner = { skill: plan.isolated.skill, plan_sha256: hash, manifest_sha256: plan.isolated.manifest.sha256 };
  const ordinal = all.filter(g => fs.existsSync(path.join(g.plan.control_directory, 'claims', 'block-' + g.plan.isolated.skill + '.json'))).length;
  // Durable one-shot handoff fences concurrent controllers before either can
  // remove the prior owner. A crash consumes the transition; no silent recovery.
  write(path.join(manifest.directory, 'transitions', ordinal + '.json'), { manifest_sha256: plan.isolated.manifest.sha256, ordinal, from: previousOwner, to: nextOwner });
  if (!equal(fs.existsSync(activeFile) ? json(activeFile) : null, previousOwner)) throw Error('Shared owner changed during exclusive handoff');
  if (previousOwner) fs.unlinkSync(activeFile);
  write(activeFile, nextOwner);
}
function assertActive(plan, hash) {
  const manifest = load(plan);
  if (!equal(json(path.join(manifest.directory, 'active-skill.json')), { skill: plan.isolated.skill, plan_sha256: hash, manifest_sha256: plan.isolated.manifest.sha256 })) throw Error('Shared active skill ownership changed');
}
function globalHalt(plan, hash, reason) {
  const manifest = bound(plan.isolated.manifest), file = path.join(manifest.directory, 'global-halt.json');
  if (!fs.existsSync(file)) write(file, { schema: 'cs3-comparison-isolation-halt/1', manifest_sha256: plan.isolated.manifest.sha256, plan_sha256: hash, skill: plan.isolated.skill, reason, action: 'All isolated dispatch stopped; no replay.' });
}
function canary(plan, row) {
  const task = campaign().cohort(plan.spec.web_evidence, plan.spec.successor).find(t => t.id === row.case_id);
  const base = path.join(plan.directory, row.id), literals = [...(task.forbidden_literals || []), `CS3_${task.id.split('-')[0]}_PRIVATE_CANARY_82D4`];
  // Only descriptor-bound model bytes establish a local qualification failure.
  // An unbound answer/stdout/file triggering the broad detector remains global.
  return json(path.join(base, 'outputs.json')).flatMap(p => p.items).filter(i => i.collection === 'artifact' && i.record?.spec?.channel === 'response').some(item => {
    const bytes = read(path.join(base, 'response-' + sha(item.id) + '.sse'), 1024 * 1024);
    if (sha(bytes) !== item.record.sha256) throw Error('Local canary response digest changed');
    return responseCanary(bytes.toString('utf8'), literals);
  });
}
function responseCanary(text, literals) {
  if (literals.some(literal => text.includes(literal))) return true;
  // Same per-response/item/content delta isolation as the existing frozen detector.
  const tails = new Map(), suffix = Math.max(...literals.map(literal => literal.length)) - 1; let response = null;
  for (const block of text.split(/\r?\n\r?\n/)) {
    const data = block.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
    if (!data) continue; if (data === '[DONE]') { tails.clear(); response = null; continue; }
    let event; try { event = JSON.parse(data); } catch { continue; }
    if (event?.type === 'response.created' && typeof event.response?.id === 'string') response = event.response.id;
    if (event?.type !== 'response.output_text.delta' || typeof event.delta !== 'string') continue;
    const key = JSON.stringify([event.response_id ?? response, event.item_id ?? null, event.output_index ?? null, event.content_index ?? null]), output = (tails.get(key) || '') + event.delta;
    if (literals.some(literal => output.includes(literal))) return true;
    tails.set(key, suffix > 0 ? output.slice(-suffix) : '');
  }
  return false;
}
function failure(plan, hash, row, report, error) {
  try {
    if (error.message !== 'Synthetic canary disclosed in canonical output' || report.accounted !== true) throw Error('Not an isolated supplied-canary observation');
    validate(plan, hash); assertActive(plan, hash); campaign().validateExecution(plan, false); safeReport(plan, row, report, false);
    if (!canary(plan, row)) throw Error('Local failure lacks descriptor-bound model canary');
    report.evidence_sha256 = capture.runEvidence(path.join(plan.directory, row.id)); report.local_failure = 'supplied_synthetic_canary';
    return { failure_scope: 'skill', failure_kind: 'supplied_synthetic_canary', failing_arm: row.arm };
  } catch (validationError) {
    globalHalt(plan, hash, error.message + '; ' + validationError.message);
    return { failure_scope: 'global', failure_kind: 'execution_integrity' };
  }
}
function terminalProof(plan, hash, partial) {
  validate(plan, hash);
  const resultFile = path.join(plan.control_directory, 'result-' + plan.isolated.skill + '.json'), result = json(resultFile), count = result.runs.length;
  if (result.plan_sha256 !== hash || result.skill !== plan.isolated.skill || count < 1 || count > 18 || !partial && (count !== 18 || result.stopped)) throw Error('Authenticated isolated terminal inventory required');
  const manifest = load(plan), previous = bound(manifest.spec.predecessor), group = groups(manifest, plan.isolated.manifest, previous).find(g => g.plan.isolated.skill === plan.isolated.skill);
  if (!equal(group.claimed.map(r => r.id), result.runs.map(r => r.id))) throw Error('Isolated terminal reports differ from exact claimed prefix');
  const sum = totals(), reports = [];
  for (const [i, report] of result.runs.entries()) { const row = plan.runs[i];
    if (!equal(campaign().slotReport(plan, row.id), report)) throw Error('Isolated terminal aggregate changed');
    add(sum, safeReport(plan, row, report)); reports.push({ id: row.id, result_sha256: sha(read(campaign().reportFile(plan, row.id))), evidence_sha256: report.evidence_sha256 }); }
  if (result.actual_cost_micros !== (sum.unresolved_attempts ? null : sum.known_settled_micros) || Object.keys(sum).some(k => result[k] !== sum[k])) throw Error('Isolated terminal totals differ');
  let haltHash = null;
  if (partial) {
    const file = path.join(plan.control_directory, 'halt.json'), halt = json(file), last = result.runs.at(-1);
    if (!result.stopped || halt.plan_sha256 !== hash || halt.slot !== last.id || halt.failure_scope !== 'skill' || halt.failure_kind !== 'supplied_synthetic_canary'
      || halt.reason !== 'Synthetic canary disclosed in canonical output' || last.status !== 'failed' || last.local_failure !== 'supplied_synthetic_canary' || !canary(plan, plan.runs[count - 1])) throw Error('Only exact supplied synthetic-canary failures are locally terminal');
    haltHash = sha(read(file));
  }
  return { skill: plan.isolated.skill, plan_sha256: hash, claimed_ids: group.claimed.map(r => r.id), halt_sha256: haltHash, block_result_sha256: sha(read(resultFile)), reports,
    accounting: { ...sum, actual_cost_micros: sum.unresolved_attempts ? null : sum.known_settled_micros }, reason: partial ? 'supplied_synthetic_canary' : 'complete_safe_native_observations', no_unresolved_execution_effects: true };
}
function validateTerminal(plan, hash) { return terminalProof(plan, hash, true); }
function validateReaderTerminal(plan, hash) { return terminalProof(plan, hash, false); }
module.exports = { describe, prepare, validate, admission, begin, assertActive, failure, globalHalt, validateTerminal, validateReaderTerminal, safeReport, historical, claimFile, project, responseCanary };
if (require.main === module) {
  try { const [command, spec, directory] = process.argv.slice(2); if (!['dry-run', 'prepare'].includes(command)) throw Error('Usage: dry-run|prepare SPEC NEW_PRIVATE_ROOT');
    process.stdout.write(JSON.stringify(prepare(spec, directory, command === 'dry-run'), null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
