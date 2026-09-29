// SPDX-License-Identifier: Apache-2.0
'use strict';
// A separately allocated, one-shot compatibility observation for the new binary.
// The historical preflight implementation and its raw behavioral oracle stay intact.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), capture = require('./developer-runner.cjs');
const original = require('./cs3-read-preflight.cjs');
const { fixedProfileReasons } = require('./builtin-live-runner.cjs');
const { plain, read, write, within, filesUnder, privateDirectory, noParentInstructions, noSecrets, frames, inspection, invoke } = prior.boundaries;
const root = path.resolve(__dirname, '../..'), sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const views = ['costs', 'routing', 'outputs', 'context', 'tools', 'verification'];
const remediation = () => require('./cs3-document-remediation.cjs');
const campaign = () => require('./cs3-comparison.cjs');
const bound = original.bound, json = ref => JSON.parse(bound(ref));
const reference = file => ({ path: plain(path.resolve(file)), sha256: sha(read(file, 1024 * 1024 * 1024)) });
function requireThat(value, reason) { if (!value) throw Error(reason); }
function specIdentity(spec) {
  return { executable: spec.executable, build_receipt: spec.build_receipt, catalog: spec.catalog, node: spec.node, profile: spec.profile,
    remediation: Object.fromEntries(['decision', 'prior_terminal', 'allocation', 'qualification'].map(key => [key, spec.remediation?.[key]])) };
}
function sourceProfile(spec, current) {
  requireThat(equal(spec, specIdentity(spec)), 'Exact remediation preflight inputs required'); noSecrets(spec);
  remediation().validateAllocation(spec);
  original.validateQualification(spec.remediation.qualification, spec);
  requireThat(equal(spec.node, reference(fs.realpathSync(process.execPath))), 'Exact preflight controller required');
  const profile = json(spec.profile); bound(spec.catalog);
  const allowed = ['version', 'workspace', 'trust_workspace', 'sync_roots', 'maximum_autonomy', 'automatic_effects', 'budget_usd', 'provider', 'catalog', 'affected_paths', 'canonical_tools', 'max_requests', 'output_tokens', 'provider_timeout_seconds', 'max_transport_retries', 'deadline_seconds', 'processes', 'checks', 'mcp', 'mcp_http'];
  requireThat(Object.keys(profile).every(key => allowed.includes(key)) && !fixedProfileReasons(profile, current ? Date.now() : 0).length
    && profile.max_requests === 16 && profile.output_tokens === '2048' && profile.deadline_seconds === 180 && profile.provider_timeout_seconds === 60
    && profile.max_transport_retries === 0 && profile.maximum_autonomy === 'plan' && equal(profile.automatic_effects, [])
    && ['processes', 'checks', 'mcp', 'mcp_http'].every(key => profile[key] === undefined || equal(profile[key], []))
    && profile.provider?.raw_sha256 === spec.catalog.sha256 && profile.provider?.compatibility?.model === 'deepseek/deepseek-v3.2'
    && profile.provider?.compatibility?.endpoint === 'deepinfra/fp4', 'Fixed read-only remediation profile required');
  return profile;
}
function derived(spec, directory, current) {
  return { ...sourceProfile(spec, current), workspace: path.join(directory, 'workspace'), catalog: spec.catalog.path,
    budget_usd: '0.600000', affected_paths: ['status.txt'], canonical_tools: ['vcp_read', 'vcp_verify'] };
}
function claimFile() {
  const common = execFileSync('git', ['rev-parse', '--git-common-dir'], { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
  return path.join(path.resolve(root, common), 'vcp-cs3-document-remediation-preflight1.json');
}
function prepare(specFile, destination) {
  const spec = JSON.parse(read(specFile)), directory = plain(path.resolve(destination));
  requireThat(!within(root, directory) && !within(directory, root) && !fs.existsSync(directory) && !fs.existsSync(claimFile()), 'New private unconsumed remediation preflight required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const profile = derived(spec, directory, true);
  fs.mkdirSync(directory, { mode: 0o700 }); fs.mkdirSync(path.join(directory, 'workspace')); fs.mkdirSync(path.join(directory, 'data'));
  write(path.join(directory, 'workspace/status.txt'), original.CONTENT); write(path.join(directory, 'profile.json'), profile); write(path.join(directory, 'prompt.txt'), original.PROMPT);
  const plan = { schema: 'cs3-document-remediation-preflight-plan/1', directory, spec, source: campaign().sourceIdentity(),
    node: reference(fs.realpathSync(process.execPath)), profile_sha256: sha(read(path.join(directory, 'profile.json'))),
    prompt_sha256: sha(original.PROMPT), cap_micros: 600000, request_ceiling: 16 };
  write(path.join(directory, 'plan.json'), plan); return reference(path.join(directory, 'plan.json'));
}
function checkPlan(ref, current) {
  const plan = json(ref);
  requireThat(plan.schema === 'cs3-document-remediation-preflight-plan/1' && plain(path.dirname(ref.path)) === plan.directory
    && equal(plan.source, campaign().sourceIdentity()) && equal(plan.node, reference(fs.realpathSync(process.execPath))), 'Remediation preflight source/runtime changed');
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
  const claim = { schema: 'cs3-document-remediation-preflight-claim/1', plan: planRef, allocation: plan.spec.remediation.allocation, cap_micros: 600000, request_ceiling: 16 };
  write(claimFile(), claim); write(path.join(base, 'claim.json'), claim);
  const args = invocation(plan); write(path.join(base, 'attempted.json'), { executable: plan.spec.executable, args });
  const report = { schema: 'cs3-document-remediation-preflight/1', plan: planRef, status: 'failed', actual_cost_micros: null, raw: {}, artifacts: [] };
  try {
    const execution = call(plan.spec.executable.path, args, 360000);
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
  requireThat(report.schema === 'cs3-document-remediation-preflight/1' && plain(ref.path) === path.join(base, 'result.json')
    && equal(plan.spec, specIdentity(spec)), 'Preflight does not bind exact remediation inputs');
  const claim = { schema: 'cs3-document-remediation-preflight-claim/1', plan: report.plan, allocation: plan.spec.remediation.allocation, cap_micros: 600000, request_ceiling: 16 };
  requireThat(equal(json(reference(claimFile())), claim) && equal(json(reference(path.join(base, 'claim.json'))), claim), 'One-shot remediation preflight ownership differs');
  requireThat(equal(JSON.parse(read(path.join(base, 'attempted.json'))), { executable: plan.spec.executable, args: invocation(plan) }), 'Native preflight invocation differs');
  const raw = retained(report, base), observed = original.oracle(raw.evidence, raw.artifacts, raw.stdout, raw.exit);
  requireThat(Object.entries(observed).every(([key, value]) => equal(report[key], value)), 'Remediation preflight outcome differs from raw evidence');
  return observed;
}
module.exports = { prepare, run, validate, retained, specIdentity, checkPlan, invocation, claimFile };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); const result = command === 'prepare' ? prepare(...args) : command === 'run' ? run(...args) : (() => { throw Error('Usage: prepare SPEC NEW_PRIVATE_DIRECTORY | run PLAN SHA256'); })();
    process.stdout.write(JSON.stringify(result, null, 2) + '\n'); if (result.status === 'failed') process.exitCode = 1;
  } catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
