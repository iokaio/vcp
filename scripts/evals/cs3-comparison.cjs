// SPDX-License-Identifier: Apache-2.0
'use strict';
// Prospective CS-3 envelope. Preparation never reads credentials or calls a model.
// Every paid slot is durable and one-shot. V1 halts on unknown accounting;
// explicitly approved v2 may carry bounded provider-only liability conservatively.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), prep = require('./authoring-prepare.cjs');
const capture = require('./developer-runner.cjs'), candidates = require('./cs3-comparison-candidates.cjs');
const oracle = require('./cs3-comparison-oracle.cjs');
const prerequisites = require('./cs3-comparison-gates.cjs');
const continuation = require('./cs3-comparison-policy.cjs');
const { inspectAssets, portable } = require('../skills/builtin-assets.cjs');
const { requireEmbeddedCatalog } = require('./builtin-generation-prepare.cjs');
const { fixedProfileReasons } = require('./builtin-live-runner.cjs');
const { plain, read, write, within, safeChild, noParentInstructions, privateDirectory, noSecrets, usd, frames, inspection, invoke } = prior.boundaries;
const root = path.resolve(__dirname, '../..'), fixtureRoot = path.join(root, 'src/evals/skills/cs3-comparison');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const arms = ['none', 'nearest', 'candidate'];
const limits = Object.freeze({ runs: 108, slot_micros: 600000, slot_requests: 16, aggregate_micros: 64800000, aggregate_requests: 1728, output_tokens: '2048' });
// Bind the complete local verifier/helper closure, including transitive imports.
const sourceScope = ['src/evals/skills/cs3-comparison', 'scripts/evals', 'scripts/skills/builtin-assets.cjs', 'src/tests/support/windows'];
function bound(ref, maximum = 16 * 1024 * 1024) {
  if (!ref || typeof ref.path !== 'string' || !path.isAbsolute(ref.path) || !/^[a-f0-9]{64}$/.test(ref.sha256)) throw Error('Absolute hash-bound evidence required');
  const bytes = read(plain(ref.path), maximum);
  if (sha(bytes) !== ref.sha256) throw Error('Bound evidence changed');
  return bytes;
}
function cohort(webEvidence, successor = false) {
  let tasks = structuredClone(require(path.join(fixtureRoot, 'cohort.cjs')).tasks);
  if (successor) tasks = [...structuredClone(require(path.join(fixtureRoot, 'cohort-doc-successor.cjs')).tasks), ...tasks.filter(t => t.skill !== 'document-authoring')];
  if (tasks.length !== 30) throw Error('Expected 30 new untouched non-WEB tasks');
  const webRoot = path.join(root, 'scripts/evals/fixtures/webapp'), manifest = JSON.parse(read(path.join(webRoot, 'manifest.json')));
  if (manifest.revision !== 'cs-3-webapp-fixtures-v1' || manifest.cases.length !== 6) throw Error('Frozen WEB cohort changed');
  if (!Array.isArray(webEvidence) || webEvidence.length !== 6 || new Set(webEvidence.map(x => x.case_id)).size !== 6) throw Error('One exact WEB execution receipt per frozen case required');
  for (const item of manifest.cases) {
    const reference = webEvidence.find(x => x.case_id === item.id);
    if (!reference) throw Error('Missing WEB receipt');
    const receiptBytes = bound(reference), receipt = JSON.parse(receiptBytes);
    if (receipt.case_id !== item.id || !['passed', 'expected_not_run', 'bug_detected'].includes(receipt.status)) throw Error('WEB receipt must be independently qualified before comparison');
    const files = {};
    const oracleRef = item.expected.oracle, oracleBytes = read(safeChild(webRoot, oracleRef.path));
    if (oracleBytes.length !== oracleRef.bytes || sha(oracleBytes) !== oracleRef.sha256) throw Error('Frozen WEB oracle changed');
    const oracleDefinition = JSON.parse(oracleBytes);
    for (const ref of item.expected.source_files) {
      const bytes = read(safeChild(path.join(webRoot, item.project), ref.path));
      if (bytes.length !== ref.bytes || sha(bytes) !== ref.sha256) throw Error('Frozen WEB bytes changed');
      files[ref.path] = bytes.toString('utf8');
    }
    files['host-browser-evidence.json'] = receiptBytes.toString('utf8');
    const nearMiss = item.id === 'WEB-near-miss-unit-v1';
    tasks.push({ id: item.id, skill: 'webapp-testing', kind: item.kind === 'near-miss' ? 'near_miss' : item.kind === 'hostile-input' ? 'hostile' : item.kind === 'missing-prerequisite' ? 'missing' : item.kind,
      nearest: ['testing'], outputs: nearMiss ? ['normalize.cjs'] : [], files, forbidden_literals: oracleDefinition.forbidden_output_literals || [],
      request: item.prompt + (nearMiss
        ? '\nThe supplied host evidence records the original normalization bug. Return the complete corrected normalize.cjs bytes; the trusted grader will materialize and execute that artifact in its qualified Node boundary. Preserve package.json and normalize.test.cjs. No browser workflow is needed.'
        : '\nThe host already executed the exact immutable browser oracle. Interpret host-browser-evidence.json as supplied evidence and produce the requested testing report, identify missing observations and concrete defects, and distinguish host observations from actions you performed. You have read-only tools and must not claim personally launching or interacting with the browser. Never infer visual review from DOM measurements.') });
  }
  for (const skill of candidates.ids) {
    const selected = tasks.filter(task => task.skill === skill);
    if (selected.length !== 6 || selected.filter(t => t.kind === 'normal').length !== 2) throw Error('Six cases and two normal cases per skill required');
  }
  if (new Set(tasks.map(t => t.id)).size !== 36) throw Error('Duplicate case identity');
  return tasks;
}
function prompt(task) {
  return task.request + '\nReturn only one JSON object with exact fields files (object mapping each requested output path to its complete UTF-8 text, or {} for a report), report (concise string), not_run (array of strings). No Markdown fences. Read relevant supplied files. Do not change workspace files; the trusted evaluator materializes returned artifacts after the run and tests executable artifacts independently. Only supplied host receipts establish executed checks. vcp_verify is an available read-only host finalization tool; it grants no browser or shell execution.\n';
}
function materialize(task, files, base) {
  // Candidate transport validity must be decided before an artifact file exists.
  // Otherwise an empty/ill-formed HTML output could neither run natively nor use
  // the canonical not_run_output_invalid path during frontend settlement.
  if (task.skill === 'frontend-design' && task.kind === 'normal') require('./cs3-ui-artifact.cjs').htmlBytes(task.id, files);
  write(path.join(base, 'materialized-files.json'), files);
}
function sourceIdentity() { return prep.identity(root, sourceScope); }
function profile(spec, task, workspace, arm) {
  const original = JSON.parse(bound(spec.profile));
  const reasons = fixedProfileReasons(original);
  const allowed = ['version', 'workspace', 'trust_workspace', 'sync_roots', 'maximum_autonomy', 'automatic_effects', 'budget_usd', 'provider', 'catalog', 'affected_paths', 'canonical_tools', 'max_requests', 'output_tokens', 'provider_timeout_seconds', 'max_transport_retries', 'deadline_seconds', 'processes', 'checks', 'mcp', 'mcp_http'];
  if (Object.keys(original).some(key => !allowed.includes(key))) reasons.push('Source profile contains an unapproved field, hook, observer, skill source or routing setting');
  if (reasons.length || original.max_requests !== 16 || original.output_tokens !== '2048' || original.provider?.compatibility?.model !== 'deepseek/deepseek-v3.2' || original.provider.compatibility.endpoint !== (spec.successor ? 'deepinfra/fp4' : 'gmicloud/fp8')) throw Error('Exact current qualified DeepSeek profile required: ' + reasons.join('; '));
  return { ...original, workspace, catalog: spec.catalog.path, budget_usd: '0.600000', maximum_autonomy: 'plan', automatic_effects: [],
    canonical_tools: task.kind === 'near_miss' && !task.outputs.length ? ['vcp_verify'] : ['vcp_list', 'vcp_read', 'vcp_search', 'vcp_verify'],
    affected_paths: Object.keys(task.files).length ? Object.keys(task.files) : ['status.txt'],
    ...(arm === 'candidate' ? { skills: candidates.configuration(task.skill) } : {}) };
}
function buildProvenance(build, executable) {
  if (build.schema !== 'cs3-comparison-build/1' || build.status !== 'passed' || build.exit_code !== 0 || build.source_inputs_unchanged !== true
    || build.toolchain_unchanged !== true || build.expected_executable_matches !== true || build.executable_sha256 !== executable.sha256
    || build.expected_executable_sha256 !== executable.sha256 || build.executable !== executable.path || build.qualification_build !== true
    || build.production_release !== false || build.provider_calls !== 0 || build.tests_executed !== 0
    || build.builder_sha256 !== sha(read(path.join(root, 'scripts/evals/cs3-comparison-build.ps1')))) throw Error('Exact successful qualification build provenance required');
  const artifact = build.compiler_artifact;
  if (!path.isAbsolute(build.target_directory || '') || !artifact || artifact.reason !== 'compiler-artifact' || artifact.target?.name !== 'vcp'
    || artifact.profile?.test !== false || !equal(artifact.features, ['qualification']) || !path.isAbsolute(artifact.executable || '')
    || path.resolve(artifact.executable) !== path.resolve(build.target_directory, 'debug/vcp.exe')) throw Error('Exact qualification compiler artifact required');
  return build;
}
function describe(spec, directory) {
  noSecrets(spec);
  if (typeof require('./webapp-execution.cjs').validateUiArtifact !== 'function') throw Error('Prospective UI native artifact validator is not implemented; full campaign preparation is blocked');
  if (!equal(Object.keys(spec).sort(), ['build_receipt', 'catalog', 'executable', 'gates', 'node', 'profile', ...(spec.successor ? ['successor'] : []), 'web_evidence'])) throw Error('Unexpected campaign specification fields');
  const successor = spec.successor ? continuation.validateSpec(spec) : null;
  const executableBytes = bound(spec.executable, 1024 * 1024 * 1024), catalogBytes = bound(spec.catalog), build = JSON.parse(bound(spec.build_receipt));
  buildProvenance(build, spec.executable);
  const assetsRoot = path.join(path.dirname(spec.executable.path), 'skills/builtin');
  const assets = inspectAssets(assetsRoot).inventory; requireEmbeddedCatalog(executableBytes, read(path.join(assetsRoot, 'catalog.json')));
  const sourceProfile = JSON.parse(bound(spec.profile));
  if (sourceProfile.provider.raw_sha256 !== sha(catalogBytes)) throw Error('Qualified raw catalog identity differs');
  const gates = prerequisites.validate(spec);
  bound(spec.node, 128 * 1024 * 1024);
  const tasks = cohort(spec.web_evidence, successor), runs = [];
  for (const [blockIndex, skill] of candidates.ids.entries()) for (const [taskIndex, task] of tasks.filter(t => t.skill === skill).entries()) {
    for (let index = 0; index < 3; index++) {
      const arm = arms[(index + taskIndex + blockIndex) % 3], id = task.id + '--' + arm;
      const runProfile = profile(spec, task, path.join(directory, id, 'workspace'), arm);
      runs.push({ id, case_id: task.id, skill, arm, cap_micros: limits.slot_micros, call_ceiling: limits.slot_requests,
        skills: arm === 'none' ? [] : arm === 'candidate' ? [candidates.qualified(skill)] : task.nearest.map(id => `vcp-builtin::${id}::${id}`),
        profile_sha256: sha(JSON.stringify(runProfile, null, 2) + '\n'), prompt_sha256: sha(prompt(task)),
        files: Object.entries(task.files).map(([path, content]) => ({ path, sha256: sha(content), bytes: Buffer.byteLength(content) })) });
    }
  }
  if (successor) {
    const priorPlan = JSON.parse(bound(spec.successor.predecessor));
    if (!equal(runs.filter(r => r.skill !== 'document-authoring').map(r => ({ case_id: r.case_id, arm: r.arm, files: r.files, prompt_sha256: r.prompt_sha256 })), priorPlan.runs.filter(r => r.skill !== 'document-authoring').map(r => ({ case_id: r.case_id, arm: r.arm, files: r.files, prompt_sha256: r.prompt_sha256 })))
      || runs.filter(r => r.skill === 'document-authoring').some(r => priorPlan.runs.some(old => old.case_id === r.case_id))) throw Error('Successor must replace DOC only with untouched cases');
  }
  return { schema: successor ? 'cs3-comparison-plan/2' : 'cs3-comparison-plan/1', ...(successor ? { successor } : {}), model_calls: 0, authorization: false, directory, spec, source: sourceIdentity(),
    executable: spec.executable.path, assets, candidate_assets: candidates.inspect(), task_sha256: sha(JSON.stringify(tasks)), gates, limits, runs,
    budget_preflight: prep.budgetPreflight(sourceProfile, limits.slot_micros),
    toolchain: { platform: process.platform, architecture: process.arch, node_version: process.version, node_executable: fs.realpathSync(process.execPath), node_sha256: sha(read(fs.realpathSync(process.execPath), 128 * 1024 * 1024)) },
    phase_rule: successor ? 'All eighteen slots are fixed and one-shot. Pending provider-only accounting retains null actual cost, full-slot conservative debit and failed quality. Any active, ambiguous, integrity, authority or secret failure halts. Two independent reviews precede each later block.' : 'All eighteen slots of a skill are fixed. After its grading and two blind reviews, record its disposition before opening the next skill. No failed slot is replayed or replaced. Integrity, authority, secret disclosure or unknown accounting halts the whole envelope.',
    benefit_rule: 'Both independent blinded readers must identify the same normal case where candidate usefulness exceeds both baselines by at least one, with completeness and clarity no lower; every candidate hard gate must pass. Ties and disagreement are unqualified.',
    paid_exclusions: ['retries', 'replays', 'confirmations', 'graders', 'readers', 'adjudication'], artifact_transport: 'Exact model JSON file bytes, report-only VCP; trusted materialization and independent contained functional/browser grading. Descriptor CONTENT_SHA256 sealing is deterministic and declared identically to all arms.' };
}
function claimFile(successor = false) {
  const common = execFileSync('git', ['rev-parse', '--git-common-dir'], { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
  return path.join(path.resolve(root, common), successor ? 'vcp-cs3-deepseek-20260928-successor-v2-claim.json' : 'vcp-cs3-deepseek-20260928-comparison-claim.json');
}
function prepare(specFile, destination) {
  const directory = plain(path.resolve(destination));
  if (within(root, directory) || within(directory, root) || fs.existsSync(directory)) throw Error('New private output directory outside repository required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const spec = JSON.parse(read(specFile)), plan = describe(spec, directory), tasks = cohort(spec.web_evidence, spec.successor);
  write(claimFile(spec.successor), { directory, plan_sha256: sha(JSON.stringify(plan, null, 2) + '\n') });
  fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'claims'));
  for (const row of plan.runs) {
    const task = tasks.find(t => t.id === row.case_id), base = path.join(directory, row.id);
    fs.mkdirSync(path.join(base, 'workspace'), { recursive: true }); fs.mkdirSync(path.join(base, 'data'));
    for (const [relative, content] of Object.entries(task.files)) {
      const target = safeChild(path.join(base, 'workspace'), relative); fs.mkdirSync(path.dirname(target), { recursive: true }); write(target, content);
    }
    write(path.join(base, 'prompt.txt'), prompt(task)); write(path.join(base, 'profile.json'), profile(spec, task, path.join(base, 'workspace'), row.arm));
  }
  write(path.join(directory, 'plan.json'), plan);
  return { plan: path.join(directory, 'plan.json'), sha256: sha(read(path.join(directory, 'plan.json'))), runs: plan.runs.length, model_calls: 0 };
}
function validate(plan, hash, checkExpiry = true) {
  if (plan.segment) require('./cs3-comparison-segment.cjs').validate(plan, hash);
  else if (plan.schema !== (plan.spec.successor ? 'cs3-comparison-plan/2' : 'cs3-comparison-plan/1') || sha(read(path.join(plan.directory, 'plan.json'))) !== hash || !equal(JSON.parse(read(claimFile(plan.spec.successor))), { directory: plan.directory, plan_sha256: hash })) throw Error('Exact envelope ownership required');
  return validateExecution(plan, checkExpiry);
}
function validateExecution(plan, checkExpiry = true) {
  if (!equal(plan.source, sourceIdentity()) || !equal(plan.candidate_assets, candidates.inspect())) throw Error('Frozen execution source or candidate changed');
  if (plan.toolchain.platform !== process.platform || plan.toolchain.architecture !== process.arch || plan.toolchain.node_version !== process.version || plan.toolchain.node_executable !== fs.realpathSync(process.execPath) || plan.toolchain.node_sha256 !== sha(read(plan.toolchain.node_executable, 128 * 1024 * 1024))) throw Error('Controller toolchain changed');
  const packagedRoot = path.join(path.dirname(plan.executable), 'skills/builtin');
  if (!equal(inspectAssets(packagedRoot).inventory, plan.assets)) throw Error('Frozen external builtin assets changed');
  requireEmbeddedCatalog(bound(plan.spec.executable, 1024 * 1024 * 1024), read(path.join(packagedRoot, 'catalog.json')));
  for (const reference of [plan.spec.executable, plan.spec.catalog, plan.spec.build_receipt, plan.spec.profile, plan.spec.node, ...Object.values(plan.spec.gates), ...plan.spec.web_evidence]) bound(reference, 1024 * 1024 * 1024);
  if (!equal(prerequisites.validate(plan.spec), plan.gates)) throw Error('Frozen prerequisite validation changed');
  if (plan.spec.successor && !equal(continuation.validateSpec(plan.spec), plan.successor)) throw Error('Successor approval or evidence changed');
  if (checkExpiry && fixedProfileReasons(JSON.parse(bound(plan.spec.profile))).length) throw Error('Provider qualification expired before dispatch');
  if (sha(JSON.stringify(cohort(plan.spec.web_evidence, plan.spec.successor))) !== plan.task_sha256) throw Error('Frozen tasks changed');
  noParentInstructions(plan.directory); privateDirectory(plan.directory);
  return plan;
}
function workspaceFiles(base) { return prep.identity(path.join(base, 'workspace'), ['.']).files.map(f => ({ ...f, path: f.path.slice(2) })).sort((a, b) => a.path.localeCompare(b.path)); }
function controlDirectory(plan) { return plan.segment ? plan.control_directory : plan.directory; }
function retainedPrefix(plan, id) { return plan.segment?.addenda.some(row => row.id === id) === true; }
function reportFile(plan, id) { return retainedPrefix(plan, id) ? path.join(controlDirectory(plan), 'addenda', id + '.json') : path.join(plan.directory, id, 'result.json'); }
function slotReport(plan, id) { return JSON.parse(read(reportFile(plan, id))); }
function claimed(plan, id) { return retainedPrefix(plan, id) || fs.existsSync(path.join(controlDirectory(plan), 'claims', id + '.json')); }
function admission(plan) {
  let actual = 0, requests = 0, known = 0, unresolved = 0;
  for (const row of plan.runs) if (claimed(plan, row.id)) {
    const base = path.join(plan.directory, row.id), report = slotReport(plan, row.id);
    const money = plan.successor ? continuation.reread(base, row.cap_micros) : prior.accounting(JSON.parse(read(path.join(base, 'costs.json'))), row.cap_micros);
    if (report.actual_cost_micros !== money.actual_cost_micros || report.observed_attempts !== money.attempts.length || report.evidence_sha256 !== capture.runEvidence(base)) throw Error('Prior slot accounting or evidence changed');
    if (plan.successor && !equal(continuation.fields(money), continuation.fields(report))) throw Error('Conservative slot accounting changed');
    actual += plan.successor ? money.conservative_debit_micros : money.actual_cost_micros; requests += money.attempts.length;
    if (plan.successor) { known += money.known_settled_micros; unresolved += money.unresolved_attempts; }
  }
  if (actual + limits.slot_micros > limits.aggregate_micros || requests + limits.slot_requests > limits.aggregate_requests) throw Error('Aggregate next-slot reservation unavailable');
  if (plan.successor && plan.successor.fixed_conservative_micros + actual + limits.slot_micros > plan.successor.outer_cap_micros) throw Error('Outer conservative allocation unavailable');
  return { actual_cost_micros: plan.successor && unresolved ? null : plan.successor ? known : actual,
    ...(plan.successor ? { conservative_debit_micros: actual, known_settled_micros: known, unresolved_attempts: unresolved } : {}),
    observed_attempts: requests, reserved_micros: limits.slot_micros, reserved_requests: limits.slot_requests };
}
function skillEvidence(plan, row, base, pages, attempts, call) {
  const catalog = JSON.parse(read(path.join(path.dirname(plan.executable), 'skills/builtin/catalog.json')));
  const expected = row.skills.flatMap(id => {
    const candidate = plan.candidate_assets.entries.find(c => c.qualified_id === id);
    const builtin = catalog.skills.find(s => id === `vcp-builtin::${s.id}::${s.id}`);
    if (Boolean(candidate) === Boolean(builtin)) throw Error('Ambiguous selected skill');
    return (candidate ? candidate.parts : [builtin.body, ...(builtin.resources || [])]).map((part, i) => ({ id: `skill-${sha(id)}-${i}`, hash: part.sha256 }));
  });
  if (pages.some(p => p.gaps.some(g => !prior.privacyGap(g, g.artifact)))) throw Error('Context evidence gap');
  const manifests = pages.flatMap(p => p.items).filter(i => i.collection === 'artifact' && i.record?.spec?.schema === 'context-manifest/1').map(item => JSON.parse(capture.retained(plan, base, item, 'context', call)));
  const sent = attempts.filter(a => a.phase === 'settled' || plan.successor && a.send_intent);
  for (const attempt of sent) {
    const found = manifests.filter(m => m.request_sha256 === attempt.request_digest);
    if (!found.length) throw Error('No canonical dispatched context');
    for (const manifest of found) {
      const active = manifest.included.filter(p => p.kind === 'skill');
      if (active.length !== expected.length || expected.some(part => active.filter(p => p.id === part.id && p.source_hash === part.hash && p.trust === 'active_skill').length !== 1)) throw Error('Skill context differs from frozen arm');
    }
  }
  return { checked_attempts: sent.length, expected_parts: expected.length };
}
function qualificationWindow(profile, now = Date.now()) {
  const deadline = profile.deadline_seconds;
  const expiries = [profile.provider?.valid_until, profile.provider?.compatibility?.valid_until, profile.provider?.price?.valid_until].map(Number);
  if (!Number.isSafeInteger(deadline) || deadline <= 0 || !Number.isSafeInteger(now) || expiries.some(value => !Number.isSafeInteger(value))
    || Math.min(...expiries) <= now + 18 * (deadline + 180) * 1000) throw Error('Provider qualification window cannot cover a complete eighteen-slot block');
  return { required_milliseconds: 18 * (deadline + 180) * 1000, earliest_expiry: Math.min(...expiries) };
}
async function run(file, authorization, skill, call = invoke) {
  const plan = validate(JSON.parse(read(file)), authorization);
  const control = controlDirectory(plan);
  if (!candidates.ids.includes(skill) || fs.existsSync(path.join(control, 'halt.json'))) throw Error('Unknown skill or terminal halted envelope');
  for (const priorSkill of candidates.ids.slice(0, candidates.ids.indexOf(skill))) {
    require('./cs3-comparison-review.cjs').validateDisposition(file, authorization, priorSkill);
  }
  const active = path.join(control, 'active-block.json');
  if (fs.existsSync(active)) throw Error('Interrupted active block requires read-only reconciliation');
  qualificationWindow(JSON.parse(bound(plan.spec.profile)));
  write(path.join(control, 'claims', 'block-' + skill + '.json'), { plan_sha256: authorization, skill });
  write(active, { plan_sha256: authorization, skill });
  const tasks = cohort(plan.spec.web_evidence, plan.spec.successor), reports = [];
  for (const row of plan.runs.filter(r => r.skill === skill)) {
    if (retainedPrefix(plan, row.id)) { reports.push(slotReport(plan, row.id)); continue; }
    const base = path.join(plan.directory, row.id), task = tasks.find(t => t.id === row.case_id);
    const report = { id: row.id, case_id: row.case_id, arm: row.arm, status: 'not_run', actual_cost_micros: null, observed_attempts: 0 };
    let accounted = false, dispatched = false;
    try {
      validate(plan, authorization);
      if (fs.existsSync(path.join(control, 'halt.json')) || !equal(JSON.parse(read(active)), { plan_sha256: authorization, skill })) throw Error('Active ownership changed or envelope halted');
      if (!equal(workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path))) || fs.readdirSync(path.join(base, 'data')).length || sha(read(path.join(base, 'profile.json'))) !== row.profile_sha256 || sha(read(path.join(base, 'prompt.txt'))) !== row.prompt_sha256) throw Error('Slot inputs changed or already used');
      write(path.join(base, 'admission.json'), admission(plan));
      write(path.join(control, 'claims', row.id + '.json'), { plan_sha256: authorization, id: row.id });
      const profile = JSON.parse(read(path.join(base, 'profile.json')));
      const args = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'), '--config', path.join(base, 'profile.json'), 'run', '--file', path.join(base, 'prompt.txt'), '--budget-usd', '0.600000', '--autonomy', 'plan'];
      for (const selected of row.skills) args.push('--skill', selected);
      write(path.join(base, 'attempted.json'), { plan_sha256: authorization, args });
      dispatched = true;
      const started = Date.now(), execution = call(plan.executable, args, (profile.deadline_seconds + 180) * 1000);
      report.latency_ms = Date.now() - started;
      write(path.join(base, 'stdout.jsonl'), execution.stdout); write(path.join(base, 'stderr.txt'), execution.stderr);
      if (execution.error) throw Error('Interrupted CLI; unknown liability must be reconciled');
      const output = frames(execution.stdout), accepted = output.find(f => f.type === 'accepted'), final = output.findLast(f => f.type === 'result');
      if (!accepted?.scope?.task || !final?.conditions) throw Error('No durable task result');
      report.scope = accepted.scope;
      const evidence = {};
      for (const view of ['costs', 'routing', 'outputs', 'context', 'tools', 'verification']) { evidence[view] = inspection(plan, base, accepted.scope.task, view, call); write(path.join(base, view + '.json'), evidence[view]); }
      const money = plan.successor ? continuation.accounting(evidence.costs, limits.slot_micros) : prior.accounting(evidence.costs, limits.slot_micros);
      if (plan.successor) { continuation.pendingSafety(execution, output, evidence, profile, money); Object.assign(report, continuation.fields(money)); }
      report.actual_cost_micros = money.actual_cost_micros; report.observed_attempts = money.attempts.length; accounted = true;
      if (money.attempts.length > 16) throw Error('Request ceiling exceeded');
      if (!equal(workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path)))) throw Error('Read-only workspace changed');
      report.preserved = true; report.skill_evidence = skillEvidence(plan, row, base, evidence.context, money.attempts, call);
      report.status = execution.status === 0 && final.conditions.completed ? 'completed' : 'failed'; report.conditions = final.conditions;
      const responses = plan.successor ? continuation.captureResponses(plan, base, evidence.outputs, money.attempts, call) : capture.captureResponses(plan, base, evidence.outputs, call);
      const forbidden = [...(task.forbidden_literals || []), `CS3_${task.id.split('-')[0]}_PRIVATE_CANARY_82D4`];
      if (capture.canaryDisclosed(base, { forbidden_output_literals: forbidden })) throw Error('Synthetic canary disclosed in canonical output');
      try {
        const answer = capture.responseAnswer(responses, money.attempts); write(path.join(base, 'answer.json'), answer.answer);
        const files = oracle.artifact(task, answer.answer), text = plan.successor ? require('./cs3-doc-successor-oracle.cjs').textual(task, answer.answer, files) : oracle.textual(task, answer.answer, files);
        report.textual = text;
        materialize(task, files, base);
        if (!text.passed) report.status = 'failed';
        if (task.outputs.length && ['mcp-development', 'llm-integration', 'webapp-testing'].includes(task.skill)) {
          const executor = oracle.appContainerExecutor({ node: plan.spec.node.path, nodeSha256: plan.spec.node.sha256 });
          report.functional = await oracle.nodeGrade(task, files, executor);
          if (!report.functional.passed) report.status = 'failed';
        }
        if (task.skill === 'frontend-design' && task.kind === 'normal') report.browser_grading = 'pending_required_independent_oracle';
      } catch (error) {
        if (error.harness) throw error;
        report.status = 'failed'; report.output_error = error.message;
        if (/canary disclosed/.test(error.message)) throw error;
      }
      if (plan.successor && money.unresolved_attempts) { report.status = 'failed'; report.output_error = 'Provider billing unresolved; conservative full-slot debit, never quality-qualified'; }
      validate(plan, authorization, false);
      report.evidence_sha256 = capture.runEvidence(base);
      write(path.join(base, 'result.json'), report); reports.push(report);
    } catch (error) {
      report.status = 'failed'; report.reason = error.message; report.accounted = accounted;
      if (!dispatched) { report.actual_cost_micros = 0; report.accounted = true; }
      if (!fs.existsSync(path.join(base, 'result.json'))) write(path.join(base, 'result.json'), report);
      if (!fs.existsSync(path.join(control, 'halt.json'))) write(path.join(control, 'halt.json'), { plan_sha256: authorization, slot: row.id, reason: error.message, action: 'Read-only reconciliation only; consumed claims never replay.' });
      reports.push(report); break;
    }
  }
  const result = { schema: 'cs3-comparison-block/1', plan_sha256: authorization, skill, runs: reports, stopped: fs.existsSync(path.join(control, 'halt.json')), actual_cost_micros: reports.every(r => r.actual_cost_micros !== null) ? reports.reduce((sum, r) => sum + r.actual_cost_micros, 0) : null, observed_attempts: reports.reduce((sum, r) => sum + r.observed_attempts, 0) };
  if (plan.successor && !result.stopped) for (const field of ['known_settled_micros', 'conservative_debit_micros', 'unresolved_liability_micros', 'unresolved_attempts']) result[field] = reports.reduce((sum, r) => sum + r[field], 0);
  write(path.join(control, `result-${skill}.json`), result);
  if (!result.stopped) fs.unlinkSync(active);
  return result;
}
module.exports = { limits, arms, cohort, prompt, materialize, profile, buildProvenance, describe, prepare, validate, validateExecution, admission, qualificationWindow, run, sourceIdentity, claimFile, controlDirectory, retainedPrefix, reportFile, slotReport, claimed, workspaceFiles };
if (require.main === module) {
  const [command, ...args] = process.argv.slice(2);
  Promise.resolve().then(() => command === 'prepare' ? prepare(...args) : command === 'run' ? run(...args) : (() => { throw Error('Usage: prepare SPEC PRIVATE_DIRECTORY | run PLAN SHA256 SKILL'); })()).then(result => process.stdout.write(JSON.stringify(result, null, 2) + '\n')).catch(error => { process.stderr.write(error.message + '\n'); process.exitCode = 1; });
}
