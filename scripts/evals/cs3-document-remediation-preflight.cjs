// SPDX-License-Identifier: Apache-2.0
'use strict';
// A separately allocated, one-shot compatibility observation for the new binary.
// The historical preflight implementation and its raw behavioral oracle stay intact.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), capture = require('./developer-runner.cjs');
const original = require('./cs3-read-preflight.cjs');
const prep = require('./authoring-prepare.cjs'), policy = require('./cs3-comparison-policy.cjs');
const { fixedProfileReasons } = require('./builtin-live-runner.cjs');
const { plain, read, write, within, filesUnder, privateDirectory, noParentInstructions, noSecrets, frames, inspection, invoke } = prior.boundaries;
const root = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const views = ['costs', 'routing', 'outputs', 'context', 'tools', 'verification'];
const remediation = () => require('./cs3-document-remediation.cjs');
const skillRemediation = () => require('./cs3-skill-remediation.cjs');
const campaign = () => require('./cs3-comparison.cjs');
const bound = original.bound, json = ref => JSON.parse(bound(ref));
const reference = file => ({ path: plain(path.resolve(file)), sha256: sha(read(file, 1024 * 1024 * 1024)) });
function requireThat(value, reason) { if (!value) throw Error(reason); }
function mode(spec) {
  if (Object.hasOwn(spec, 'skill_remediation')) {
    requireThat(spec.skill_remediation && typeof spec.skill_remediation === 'object' && !Array.isArray(spec.skill_remediation)
      && !Object.hasOwn(spec, 'remediation'), 'Exclusive fixed skill remediation selector required');
    return 'skill';
  }
  return 'document';
}
function schema(spec, suffix) { return `cs3-${mode(spec)}-remediation-preflight${suffix}`; }
function specIdentity(spec) {
  if (mode(spec) === 'skill') return { executable: spec.executable, build_receipt: spec.build_receipt, catalog: spec.catalog, node: spec.node, profile: spec.profile,
    skill_remediation: { ...Object.fromEntries(['decision', 'allocation', 'history', 'recovery_native', 'package_acceptance', 'qualification'].map(key => [key, spec.skill_remediation[key]])),
      ...(Object.hasOwn(spec.skill_remediation, 'qualification_supplement') ? { qualification_supplement: spec.skill_remediation.qualification_supplement } : {}) } };
  return { executable: spec.executable, build_receipt: spec.build_receipt, catalog: spec.catalog, node: spec.node, profile: spec.profile,
    remediation: { ...Object.fromEntries(['decision', 'prior_terminal', 'runtime_decision', 'allocation', 'qualification'].map(key => [key, spec.remediation?.[key]])),
      ...(spec.remediation?.preflight_supplement ? { preflight_supplement: spec.remediation.preflight_supplement } : {}) } };
}
function sourceProfile(spec, current) {
  requireThat(equal(spec, specIdentity(spec)), 'Exact remediation preflight inputs required'); noSecrets(spec);
  const skill = mode(spec) === 'skill';
  // SKL also joins the probe claim to its separately funded allocation. A valid
  // but foreign qualification pair cannot fund this fresh preflight.
  if (skill) skillRemediation().validateQualification(spec);
  else { remediation().validateAllocation(spec); original.validateQualification(spec.remediation.qualification, spec); }
  requireThat(equal(spec.node, reference(fs.realpathSync(process.execPath))), 'Exact preflight controller required');
  const profile = json(spec.profile); bound(spec.catalog);
  const allowed = ['version', 'workspace', 'trust_workspace', 'sync_roots', 'maximum_autonomy', 'automatic_effects', 'budget_usd', 'provider', 'catalog', 'affected_paths', 'canonical_tools', 'max_requests', 'output_tokens', 'provider_timeout_seconds', 'max_transport_retries', 'deadline_seconds', 'processes', 'checks', 'mcp', 'mcp_http'];
  requireThat(Object.keys(profile).every(key => allowed.includes(key)) && !fixedProfileReasons(profile, current ? Date.now() : 0).length
    && profile.max_requests === 16 && profile.output_tokens === '2048' && profile.deadline_seconds === 600 && profile.provider_timeout_seconds === 120
    && profile.max_transport_retries === 0 && profile.maximum_autonomy === 'plan' && equal(profile.automatic_effects, [])
    && ['processes', 'checks', 'mcp', 'mcp_http'].every(key => profile[key] === undefined || equal(profile[key], []))
    && profile.provider?.raw_sha256 === spec.catalog.sha256 && profile.provider?.compatibility?.model === 'deepseek/deepseek-v3.2'
    && profile.provider?.compatibility?.endpoint === (skill ? 'friendli' : 'deepinfra/fp4'), 'Fixed read-only remediation profile required');
  return profile;
}
function derived(spec, directory, current) {
  return { ...sourceProfile(spec, current), workspace: path.join(directory, 'workspace'), catalog: spec.catalog.path,
    budget_usd: '0.600000', affected_paths: ['status.txt'], canonical_tools: ['vcp_read', 'vcp_verify'] };
}
function claimFile(ordinal = 0, kind = 'document') {
  requireThat(kind === 'document' || kind === 'skill', 'Fixed remediation claim mode required');
  requireThat(Number.isInteger(ordinal) && ordinal >= 0 && ordinal <= 3, 'Fixed replacement ordinal required');
  requireThat(kind !== 'skill' || ordinal === 0, 'Skill preflight has no replacement allocation');
  const common = execFileSync('git', ['rev-parse', '--git-common-dir'], { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
  if (kind === 'skill') return path.join(path.resolve(root, common), 'vcp-cs3-skill-remediation-preflight1.json');
  return path.join(path.resolve(root, common), ordinal ? `vcp-cs3-document-remediation-preflight-replacement${ordinal}.json` : 'vcp-cs3-document-remediation-preflight1.json');
}
function ownership(plan, planRef) {
  return { schema: schema(plan.spec, '-claim/1'), plan: planRef, allocation: (mode(plan.spec) === 'skill' ? plan.spec.skill_remediation : plan.spec.remediation).allocation,
    ...(plan.replacement ? { replacement: plan.replacement, preflight_supplement: plan.spec.remediation.preflight_supplement } : {}), cap_micros: 600000, request_ceiling: 16 };
}
function prepare(specFile, destination, replacement = null) {
  const spec = JSON.parse(read(specFile)), directory = plain(path.resolve(destination));
  requireThat(mode(spec) !== 'skill' || replacement === null, 'Skill preflight has no replacement allocation');
  requireThat(replacement !== null || !spec.remediation?.preflight_supplement, 'Supplement requires explicit replacement preparation');
  if (replacement) validateReplacement(spec, replacement);
  requireThat(!within(root, directory) && !within(directory, root) && !fs.existsSync(directory) && !fs.existsSync(claimFile(replacement?.ordinal || 0, mode(spec))), 'New private unconsumed remediation preflight required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const profile = derived(spec, directory, true);
  fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'workspace')); fs.mkdirSync(path.join(directory, 'data'));
  write(path.join(directory, 'workspace/status.txt'), original.CONTENT); write(path.join(directory, 'profile.json'), profile); write(path.join(directory, 'prompt.txt'), original.PROMPT);
  const plan = { schema: schema(spec, replacement ? '-plan/2' : '-plan/1'), ...(replacement ? { replacement } : {}), directory, spec, source: campaign().sourceIdentity(),
    node: reference(fs.realpathSync(process.execPath)), profile_sha256: sha(read(path.join(directory, 'profile.json'))),
    prompt_sha256: sha(original.PROMPT), cap_micros: 600000, request_ceiling: 16 };
  write(path.join(directory, 'plan.json'), plan); return reference(path.join(directory, 'plan.json'));
}
function checkPlan(ref, current) {
  const plan = json(ref);
  requireThat(plan.schema === schema(plan.spec, plan.replacement ? '-plan/2' : '-plan/1') && plain(path.dirname(ref.path)) === plan.directory
    && equal(plan.source, campaign().sourceIdentity()) && equal(plan.node, reference(fs.realpathSync(process.execPath))), 'Remediation preflight source/runtime changed');
  requireThat(!!plan.replacement === !!plan.spec.remediation?.preflight_supplement, 'Replacement allocation/plan mismatch');
  requireThat(mode(plan.spec) !== 'skill' || !plan.replacement, 'Skill preflight has no replacement allocation');
  if (plan.replacement) validateReplacement(plan.spec, plan.replacement);
  privateDirectory(plan.directory); noParentInstructions(plan.directory);
  requireThat(equal(JSON.parse(read(path.join(plan.directory, 'profile.json'))), derived(plan.spec, plan.directory, current))
    && sha(read(path.join(plan.directory, 'profile.json'))) === plan.profile_sha256
    && read(path.join(plan.directory, 'prompt.txt')).toString() === original.PROMPT && plan.prompt_sha256 === sha(original.PROMPT)
    && plan.cap_micros === 600000 && plan.request_ceiling === 16, 'Remediation preflight inputs changed');
  requireThat(equal(filesUnder(path.join(plan.directory, 'workspace')), ['status.txt'])
    && read(path.join(plan.directory, 'workspace/status.txt')).toString() === original.CONTENT, 'Read-only preflight workspace changed');
  return plan;
}
function invocation(plan) {
  const base = plan.directory;
  return ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'),
    '--config', path.join(base, 'profile.json'), 'run', '--file', path.join(base, 'prompt.txt'), '--budget-usd', '0.600000', '--autonomy', 'plan'];
}
function run(planFile, authorization, call = invoke) {
  const planRef = { path: plain(path.resolve(planFile)), sha256: authorization }, plan = checkPlan(planRef, true), base = plan.directory;
  requireThat(fs.readdirSync(path.join(base, 'data')).length === 0, 'Preflight native data already used');
  const claim = ownership(plan, planRef);
  write(claimFile(plan.replacement?.ordinal || 0, mode(plan.spec)), claim); write(path.join(base, 'claim.json'), claim);
  const args = invocation(plan); write(path.join(base, 'attempted.json'), { executable: plan.spec.executable, args });
  const report = { schema: schema(plan.spec, '/1'), plan: planRef, status: 'failed', actual_cost_micros: null, raw: {}, artifacts: [] };
  try {
    const execution = call(plan.spec.executable.path, args, 780000);
    write(path.join(base, 'stdout.jsonl'), execution.stdout); write(path.join(base, 'stderr.txt'), execution.stderr); write(path.join(base, 'exit.json'), { status: execution.status, error: execution.error });
    for (const name of ['stdout.jsonl', 'stderr.txt', 'exit.json']) report.raw[name] = reference(path.join(base, name));
    const accepted = frames(execution.stdout).find(frame => frame.type === 'accepted'); requireThat(accepted?.scope?.task, 'No durable remediation preflight task');
    const evidence = {}, artifacts = [], seen = new Map(), native = { executable: plan.spec.executable.path };
    for (const view of views) {
      evidence[view] = inspection(native, base, accepted.scope.task, view, call); write(path.join(base, view + '.json'), evidence[view]); report.raw[view] = reference(path.join(base, view + '.json'));
      requireThat(!evidence[view].some(page => page.gaps.some(gap => !original.gapAllowed(view, gap))), 'Incomplete raw preflight inspection');
      if (view === 'costs') { const money = prior.accounting(evidence.costs, 600000); report.actual_cost_micros = money.actual_cost_micros; report.observed_attempts = money.attempts.length; }
      if (view === 'routing') continue;
      for (const item of evidence[view].flatMap(page => page.items).filter(item => item.collection === 'artifact')) {
        if (seen.has(item.id)) { requireThat(equal(seen.get(item.id), item), 'Repeated preflight artifact descriptor differs'); continue; }
        seen.set(item.id, item);
        const bytes = capture.retained(native, base, item, view, call), file = path.join(base, 'artifact-' + sha(item.id) + '.bin');
        fs.writeFileSync(file, bytes, { flag: 'wx', mode: 0o600 }); report.artifacts.push({ id: item.id, view, ref: reference(file) }); artifacts.push({ item, bytes });
      }
    }
    checkPlan(planRef, false); Object.assign(report, original.oracle(evidence, artifacts, execution.stdout, execution));
  } catch (error) { report.error = error.message; }
  write(path.join(base, 'result.json'), report);
  return { ...reference(path.join(base, 'result.json')), status: report.status, actual_cost_micros: report.actual_cost_micros };
}
function retained(report, base) {
  requireThat(equal(Object.keys(report.raw).sort(), [...views, 'stdout.jsonl', 'stderr.txt', 'exit.json'].sort()), 'Exact raw preflight inventory required');
  for (const [name, ref] of Object.entries(report.raw)) {
    const filename = ['stdout.jsonl', 'stderr.txt', 'exit.json'].includes(name) ? name : name + '.json';
    requireThat(plain(ref.path) === path.join(base, filename), 'Raw preflight evidence escaped its directory'); bound(ref);
  }
  const evidence = Object.fromEntries(views.map(view => [view, json(report.raw[view])])), descriptors = new Map();
  for (const [view, pages] of Object.entries(evidence)) {
    requireThat(Array.isArray(pages) && pages.length > 0 && !pages.some(page => !Array.isArray(page.gaps) || !Array.isArray(page.items)
      || page.gaps.some(gap => !original.gapAllowed(view, gap))), 'Incomplete retained preflight inspection');
    if (view === 'routing') continue;
    for (const item of pages.flatMap(page => page.items).filter(item => item.collection === 'artifact')) {
      requireThat(!descriptors.has(item.id) || equal(descriptors.get(item.id), item), 'Repeated preflight artifact descriptor differs'); descriptors.set(item.id, item);
    }
  }
  requireThat(Array.isArray(report.artifacts) && report.artifacts.length === descriptors.size
    && new Set(report.artifacts.map(row => row.id)).size === descriptors.size, 'Exact retained preflight artifact coverage required');
  const artifacts = report.artifacts.map(row => {
    const item = descriptors.get(row.id);
    requireThat(item && views.includes(row.view) && row.view !== 'routing' && evidence[row.view].some(page => page.items.some(candidate => equal(candidate, item)))
      && plain(row.ref.path) === path.join(base, 'artifact-' + sha(row.id) + '.bin'), 'Preflight artifact identity/path differs');
    const bytes = bound(row.ref);
    requireThat(item.record.state === 'complete' && Number(item.record.length) === bytes.length && item.record.sha256 === sha(bytes), 'Retained preflight descriptor differs');
    return { item, bytes };
  });
  requireThat(equal(fs.readdirSync(base).filter(name => /^artifact-.*\.bin$/.test(name)).sort(), report.artifacts.map(row => path.basename(row.ref.path)).sort()), 'Unbound preflight artifact file');
  return { evidence, artifacts, stdout: bound(report.raw['stdout.jsonl']).toString(), exit: json(report.raw['exit.json']) };
}
function validate(ref, spec) {
  const report = json(ref), plan = checkPlan(report.plan, false), base = plan.directory;
  requireThat(report.schema === schema(plan.spec, '/1') && plain(ref.path) === path.join(base, 'result.json')
    && equal(plan.spec, specIdentity(spec)), 'Preflight does not bind exact remediation inputs');
  const claim = ownership(plan, report.plan);
  requireThat(equal(json(reference(claimFile(plan.replacement?.ordinal || 0, mode(plan.spec)))), claim) && equal(json(reference(path.join(base, 'claim.json'))), claim), 'One-shot remediation preflight ownership differs');
  requireThat(equal(JSON.parse(read(path.join(base, 'attempted.json'))), { executable: plan.spec.executable, args: invocation(plan) }), 'Native preflight invocation differs');
  const raw = retained(report, base), observed = original.oracle(raw.evidence, raw.artifacts, raw.stdout, raw.exit);
  requireThat(Object.entries(observed).every(([key, value]) => equal(report[key], value)), 'Remediation preflight outcome differs from raw evidence');
  return observed;
}
function archivedSource(archive, source) {
  archive = plain(archive); requireThat(path.isAbsolute(archive) && equal(prep.identity(archive, source.scope), source), 'Historical preflight source archive changed');
  const actual = prep.identity(archive, ['.']), directories = new Set(source.directories);
  for (const relative of [...source.directories, ...source.files.map(item => item.path)]) for (let p = path.posix.dirname(relative); p !== '.'; p = path.posix.dirname(p)) directories.add(p);
  requireThat(equal(actual.files.map(item => item.path.slice(2)).sort(), source.files.map(item => item.path).sort())
    && equal(actual.directories.filter(item => item !== '.').map(item => item.slice(2)).sort(), [...directories].sort()), 'Historical archive has undeclared entries');
}
function failedObservation(stdout, costs, exit, profile, terminal) {
  const output = frames(stdout), accepted = output.filter(item => item.type === 'accepted'), final = output.filter(item => item.type === 'result');
  requireThat(accepted.length === 1 && final.length === 1, 'Exact failed native task required');
  const money = policy.accounting(costs, 600000);
  requireThat(money.unresolved_attempts === 1 && money.actual_cost_micros === null, 'One bounded provider liability required');
  const facts = output.flatMap(item => item.event?.event?.data?.facts || []), effects = new Map(), descriptors = new Map();
  for (const fact of facts) {
    if (fact.collection === 'effect') { requireThat(fact.id === fact.value?.id, 'Effect fact identity differs'); effects.set(fact.id, fact.value); }
    if (fact.collection === 'artifact') { requireThat(fact.id === fact.value?.spec?.id, 'Artifact fact identity differs'); descriptors.set(fact.id, fact.value); }
  }
  // Project authenticated canonical final effect facts, not invented inspection receipts.
  policy.pendingSafety(exit, output, { tools: [{ gaps: [], items: [...effects.values()].map(record => ({ collection: 'effect', visibility: 'available', record })) }] }, profile, money);
  const pending = money.attempts.find(item => item.phase === 'reconciliation_pending');
  requireThat(pending.uncertain === 'retained response did not complete', 'Unsupported provider failure');
  for (const descriptor of [...descriptors.values()].filter(item => item.spec?.channel === 'response')) {
    const attempts = money.attempts.filter(item => (item.send_intent || item.phase === 'settled') && descriptor.spec.source === 'retained-codex-attempt:' + item.id);
    requireThat(attempts.length === 1 && equal(descriptor.spec.scope, money.ledger_scope), 'Unbound or foreign preflight response');
  }
  for (const attempt of money.attempts.filter(item => item.send_intent || item.phase === 'settled')) {
    const responses = [...descriptors.values()].filter(item => item.spec?.channel === 'response' && item.spec.source === 'retained-codex-attempt:' + attempt.id);
    requireThat(responses.length === 1 && equal(responses[0].spec.scope, money.ledger_scope)
      && responses[0].state === (attempt === pending ? 'aborted' : 'complete'), 'Dispatched preflight response membership differs');
  }
  const descriptor = [...descriptors.values()].find(item => item.spec?.channel === 'response' && item.spec.source === 'retained-codex-attempt:' + pending.id);
  requireThat(descriptor && Number(descriptor.length) === terminal.length && sha(terminal) === descriptor.sha256, 'Failed provider response bytes differ');
  const error = JSON.parse(terminal).error;
  requireThat(error?.code === 429 && error.metadata?.provider_name === 'DeepInfra' && error.metadata.is_byok === false
    && error.metadata.provider_error_code === 'engine_overloaded' && error.metadata.limit_source === 'upstream_provider_shared_pool', 'Only exact retained upstream429 qualifies a replacement');
  return { status: 'conservative_failed_preflight_preserved', conservative_debit_micros: 600000, reserved_requests: 16,
    actual_cost_micros: null, known_settled_micros: money.known_settled_micros, unresolved_micros: money.unresolved_liability_micros,
    observed_attempts: money.attempts.length, scope: money.ledger_scope };
}
function validateFailedPredecessor(ref, spec, expected = { ordinal: 0, predecessors: [] }) {
  requireThat(ref && equal(Object.keys(ref).sort(), ['inventory', 'result', 'source_archive', 'terminal_response']) && path.isAbsolute(ref.source_archive || ''), 'Exact failed predecessor reference required');
  const report = json(ref.result), plan = json(report.plan), base = plain(path.dirname(ref.result.path)), ordinal = expected.ordinal;
  requireThat(Number.isInteger(ordinal) && ordinal >= 0 && ordinal <= 2 && Array.isArray(expected.predecessors) && expected.predecessors.length === ordinal, 'Fixed predecessor position required');
  if (!ordinal) requireThat(ref.result.sha256 === '7750247906d72b676161760a04fc3e572d41469b2c8e514cffb1defc66ab1e88'
    && report.plan.sha256 === '819fda929dda9c2d658f80ee7c66fc7c659e8dea8f4076ef1bbebe1db3fb3b14' && !plan.replacement && !plan.spec.remediation?.preflight_supplement, 'Exact original failed preflight required');
  else requireThat(equal(plan.replacement, expected) && equal(plan.spec.remediation?.preflight_supplement, spec.remediation?.preflight_supplement), 'Replacement predecessor order/allocation differs');
  requireThat(report.schema === 'cs3-document-remediation-preflight/1' && report.status === 'failed' && report.actual_cost_micros === null
    && equal(report.artifacts, []) && plan.schema === `cs3-document-remediation-preflight-plan/${ordinal ? 2 : 1}` && plan.directory === base
    && plain(ref.result.path) === path.join(base, 'result.json') && plain(report.plan.path) === path.join(base, 'plan.json')
    && plan.cap_micros === 600000 && plan.request_ceiling === 16, 'Failed preflight plan/result differs');
  const comparable = value => { const result = specIdentity(value); result.executable = { sha256: result.executable.sha256 }; delete result.build_receipt; delete result.remediation.preflight_supplement; return result; };
  requireThat(equal(comparable(plan.spec), comparable(spec)), 'Failed/current compatibility identities differ');
  bound(plan.spec.executable, 1024 * 1024 * 1024); bound(spec.executable, 1024 * 1024 * 1024); bound(plan.spec.build_receipt); bound(plan.spec.catalog); bound(plan.spec.profile);
  requireThat(equal(plan.node, reference(fs.realpathSync(process.execPath))), 'Historical preflight controller changed');
  requireThat(!within(base, plain(ref.source_archive)) && !within(plain(ref.source_archive), base), 'Historical source archive must be separate');
  archivedSource(ref.source_archive, plan.source);
  requireThat(sha(read(path.join(ref.source_archive, 'scripts/evals/cs3-read-preflight.cjs'))) === sha(read(path.join(__dirname, 'cs3-read-preflight.cjs'))), 'Original behavioral oracle changed');
  const profile = JSON.parse(read(path.join(base, 'profile.json'))), inputProfile = json(plan.spec.profile);
  requireThat(equal(profile, { ...inputProfile, workspace: path.join(base, 'workspace'), catalog: plan.spec.catalog.path, budget_usd: '0.600000', affected_paths: ['status.txt'], canonical_tools: ['vcp_read', 'vcp_verify'] })
    && inputProfile.maximum_autonomy === 'plan' && equal(inputProfile.automatic_effects, []) && inputProfile.deadline_seconds === 600 && inputProfile.provider_timeout_seconds === 120
    && inputProfile.max_requests === 16 && inputProfile.output_tokens === '2048' && inputProfile.max_transport_retries === 0
    && ['processes', 'checks', 'mcp', 'mcp_http'].every(key => inputProfile[key] === undefined || equal(inputProfile[key], [])), 'Historical read-only preflight profile differs');
  requireThat(sha(read(path.join(base, 'profile.json'))) === plan.profile_sha256 && read(path.join(base, 'prompt.txt')).toString() === original.PROMPT && plan.prompt_sha256 === sha(original.PROMPT)
    && equal(filesUnder(path.join(base, 'workspace')), ['status.txt']) && read(path.join(base, 'workspace/status.txt')).toString() === original.CONTENT
    && equal(JSON.parse(read(path.join(base, 'attempted.json'))), { executable: plan.spec.executable, args: invocation(plan) }), 'Historical invocation/workspace changed');
  const claim = ownership(plan, report.plan);
  requireThat(equal(json(reference(claimFile(ordinal))), claim) && equal(json(reference(path.join(base, 'claim.json'))), claim), 'Failed predecessor one-shot ownership differs');
  requireThat(equal(prep.identity(base, ['.']), ref.inventory), 'Failed predecessor full inventory changed');
  requireThat(equal(Object.keys(report.raw).sort(), ['costs', 'exit.json', 'stderr.txt', 'stdout.jsonl']), 'Failed raw inventory differs');
  for (const [name, value] of Object.entries(report.raw)) { requireThat(plain(value.path) === path.join(base, name === 'costs' ? 'costs.json' : name), 'Failed raw path differs'); bound(value); }
  const result = failedObservation(bound(report.raw['stdout.jsonl']).toString(), json(report.raw.costs), json(report.raw['exit.json']), profile, bound(ref.terminal_response));
  requireThat(equal(prep.identity(base, ['.']), ref.inventory), 'Failed predecessor changed during validation');
  return result;
}
function validateReplacement(spec, replacement) {
  requireThat(mode(spec) === 'document', 'Skill preflight has no replacement allocation');
  requireThat(replacement && equal(Object.keys(replacement).sort(), ['ordinal', 'predecessors']) && Number.isInteger(replacement.ordinal)
    && replacement.ordinal >= 1 && replacement.ordinal <= 3 && Array.isArray(replacement.predecessors) && replacement.predecessors.length === replacement.ordinal
    && new Set(replacement.predecessors.map(item => item.result?.path)).size === replacement.ordinal, 'Fixed unique ordered replacement chain required');
  const approved = require('./cs3-preflight-supplement.cjs').validate(spec.remediation?.preflight_supplement, spec);
  requireThat(approved.slots === 3 && approved.slot_cap_micros === 600000 && approved.slot_requests === 16 && approved.additional_cap_micros === 1800000
    && approved.original_failure_sha256 === replacement.predecessors[0].result.sha256, 'Replacement supplement differs');
  replacement.predecessors.forEach((ref, ordinal) => validateFailedPredecessor(ref, spec, { ordinal, predecessors: replacement.predecessors.slice(0, ordinal) }));
}
function prepareReplacement(specFile, destination, ordinal, predecessorsFile) {
  return prepare(specFile, destination, { ordinal: Number(ordinal), predecessors: JSON.parse(read(predecessorsFile, 16 * 1024 * 1024)) });
}
function runReplacement(planFile, authorization, call = invoke) { requireThat(json({ path: plain(path.resolve(planFile)), sha256: authorization }).replacement, 'Replacement plan required'); return run(planFile, authorization, call); }
function validateReplacementResult(ref, spec) { requireThat(json(json(ref).plan).replacement, 'Replacement result required'); return validate(ref, spec); }
module.exports = { prepare, run, validate, retained, specIdentity, checkPlan, invocation, claimFile, prepareReplacement, runReplacement, validateReplacementResult, validateFailedPredecessor, failedObservation };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); const result = command === 'prepare' ? prepare(...args) : command === 'run' ? run(...args) : command === 'prepare-replacement' ? prepareReplacement(...args) : command === 'run-replacement' ? runReplacement(...args) : (() => { throw Error('Usage: prepare SPEC NEW_PRIVATE_DIRECTORY | run PLAN SHA256 | prepare-replacement SPEC NEW_PRIVATE_DIRECTORY ORDINAL PREDECESSORS | run-replacement PLAN SHA256'); })();
    process.stdout.write(JSON.stringify(result, null, 2) + '\n'); if (result.status === 'failed') process.exitCode = 1;
  } catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
