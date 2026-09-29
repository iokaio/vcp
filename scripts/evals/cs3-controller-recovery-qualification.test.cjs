// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const helperFile = path.join(__dirname, 'cs3-controller-recovery-qualification.cjs');
const actual = createRequire(helperFile), realPrep = actual('./authoring-prepare.cjs'), realUi = actual('./cs3-ui-artifact.cjs');
const realBoundaries = actual('./p6-live-runner.cjs').boundaries;
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-recovery-join-')), root = path.join(directory, 'new'), old = path.join(directory, 'old'), archive = path.join(directory, 'archive');
  t.after(() => { for (const file of Object.keys(require.cache)) if (file.startsWith(directory + path.sep)) delete require.cache[file]; fs.rmSync(directory, { recursive: true, force: true }); });
  const ref = file => ({ path: file, sha256: sha(fs.readFileSync(file)) });
  const put = (file, value) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, Buffer.isBuffer(value) || typeof value === 'string' ? value : JSON.stringify(value)); return ref(file); };
  const pins = JSON.parse(fs.readFileSync(path.join(__dirname, '../../src/evals/skills/cs3-controller-recovery/qualification-decision.json')));
  pins.historical_root = old;
  const originalDecision = JSON.parse(fs.readFileSync(path.join(__dirname, '../../src/evals/skills/cs3-comparison/acceptance-decision.json')));
  put(path.join(root, 'src/evals/skills/cs3-comparison/acceptance-decision.json'), originalDecision);
  for (const name of realUi.sourceNames) {
    put(path.join(old, 'src/tests/support/windows/webapp', name), 'historical:' + name);
    put(path.join(root, 'src/tests/support/windows/webapp', name), (pins.allowed_native_source_changes.includes(name) ? 'corrected:' : 'historical:') + name);
  }
  put(path.join(root, 'scripts/skills/builtin-assets.cjs'), '// synthetic dependency');
  put(path.join(root, 'scripts/evals/cs3-controller-recovery-qualification.cjs'), fs.readFileSync(helperFile));
  // These synthetic historical modules are not qualification evidence. The VM
  // substitutes only the archive digest/identity pins and strict native graders;
  // real module dependency authentication and read-only review invocation run.
  put(path.join(old, 'scripts/evals/cs3-comparison.cjs'), "module.exports={controlDirectory:p=>p.control_directory};");
  put(path.join(old, 'scripts/evals/historical-dependency.cjs'), 'module.exports={authenticated:true};');
  put(path.join(old, 'scripts/evals/cs3-comparison-review.cjs'), `const fs=require('node:fs'),path=require('node:path');require('./historical-dependency.cjs');
module.exports={validateDisposition:(file,hash,skill)=>JSON.parse(fs.readFileSync(path.join(JSON.parse(fs.readFileSync(file)).control_directory,'disposition-'+skill+'.json'))),
block:file=>({result:JSON.parse(fs.readFileSync(file)).test_result})};`);
  fs.cpSync(old, archive, { recursive: true });
  const scope = ['scripts', 'src/tests/support/windows/webapp'];
  const identity = (target, selected) => { const value = realPrep.identity(target, selected); if ([old, archive].includes(target) && JSON.stringify(selected) === JSON.stringify(scope)) value.content_sha256 = pins.historical_source_sha256; return value; };
  const source = identity(old, scope), archiveFile = put(path.join(directory, 'archive-receipt.json'), { schema: 'cs3-frozen-preflight-supplement-source-archive/1', commit: '6de62de0' + '0'.repeat(32), archive, source,
    source_before_after_equal: true, archive_source_equal: true, model_calls: 0, claims_created: 0 });
  const pinnedArchive = { ...archiveFile, sha256: pins.historical_archive_receipt_sha256 };
  const bound = reference => {
    if (reference?.path === pinnedArchive.path && reference.sha256 === pinnedArchive.sha256) return fs.readFileSync(reference.path);
    const bytes = realBoundaries.read(reference.path, 1024 * 1024 * 1024); assert.equal(sha(bytes), reference.sha256, 'Exact bound bytes'); return bytes;
  };
  const ui = { ...realUi, validateUiArtifact: (receipt, files, caseId) => { assert.equal(receipt.case_id, caseId); assert.equal(receipt.test_html, files['index.html']); return { status: receipt.status, assertions: receipt.assertions }; } };
  const module = { exports: {} }, gates = { bound,
    denial: () => ({ passed: true }), nodeControls: () => ({ passed: true }), web: () => ({ passed: true }),
    recoveryUiQualification: (receipt, reference, decisionRef) => { assert.equal(reference.sha256, module.exports.uiDecision(decisionRef).qualification_sha256); return { controls: 23 }; } };
  const local = name => name === './authoring-prepare.cjs' ? { identity } : name === './cs3-ui-artifact.cjs' ? ui : name === './cs3-comparison-gates.cjs' ? gates : actual(name);
  local.cache = require.cache;
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(helperFile, 'utf8'))(local, module, module.exports, path.join(root, 'scripts/evals'));
  const helper = module.exports, decisionPath = path.join(root, 'src/evals/skills/cs3-controller-recovery/qualification-decision.json');
  const saveDecision = () => put(decisionPath, pins);
  const input = { archive: pinnedArchive, plans: [] }, outputs = [];
  for (const skill of ['skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing', 'document-authoring']) {
    const base = path.join(directory, 'evidence', skill), controls = path.join(base, 'control'), review = path.join(base, 'review');
    fs.mkdirSync(review, { recursive: true });
    const runs = Array.from({ length: 18 }, (_, i) => ({ id: skill + '-' + i, skill, arm: ['none', 'nearest', 'candidate'][i % 3], case_id: skill === 'frontend-design' && i < 6 ? realUi.cases[Math.floor(i / 3)] : skill + '-case-' + i }));
    for (const run of runs) {
      put(path.join(base, run.id, 'result.json'), { id: run.id, status: 'completed' });
      if (skill === 'frontend-design' && realUi.cases.includes(run.case_id)) {
        put(path.join(base, run.id, 'materialized-files.json'), { 'index.html': '<p>' + run.id + '</p>' }); outputs.push(run);
      }
    }
    const plan = { source, directory: base, control_directory: controls, candidate_assets: { inventory: skill }, runs,
      spec: { build_receipt: { sha256: pins.historical_build_receipt_sha256 }, executable: { sha256: pins.executable_sha256 }, gates: { ui_qualification: { sha256: pins.historical_ui_matrix_sha256 }, web_oracles: { sha256: pins.historical_web_sha256 } } }, test_result: { runs } };
    const planRef = put(path.join(base, 'plan.json'), plan);
    const browser = skill === 'frontend-design' ? put(path.join(review, 'browser.json'), { runs: outputs.map(run => {
      const artifact = ref(path.join(base, run.id, 'materialized-files.json'));
      return { run_id: run.id, artifact_sha256: artifact.sha256, receipt: put(path.join(review, run.id + '.json'), { artifact_sha256: artifact.sha256, status: 'passed', assertions: [{ name: 'synthetic', passed: true }] }) };
    }) }) : null;
    put(path.join(controls, 'disposition-' + skill + '.json'), { status: 'qualified', candidate_hard_gates: true, independent_blind_readers: 2, common_normal_wins: ['normal'], review_directory: review, browser_grades: browser });
    input.plans.push({ skill, plan: planRef });
  }
  pins.new_native_source_sha256 = sha(JSON.stringify(helper.sourceClosure()));
  return { directory, root, old, archive, put, ref, pins, bound, gates, helper, input, saveDecision, outputs };
}
function finalFixture(t) {
  const f = fixture(t), { helper, pins, put, directory } = f;
  let decision = f.saveDecision();
  const historical = helper.historicalProjection(f.input, decision), proof = put(path.join(directory, 'historical-proof.json'), historical);
  pins.historical_comparison_proof_sha256 = proof.sha256;
  const buildDir = path.join(directory, 'native-build');
  for (const entry of helper.sourceClosure()) put(path.join(buildDir, entry.path), fs.readFileSync(path.join(f.root, 'src/tests/support/windows/webapp', entry.path)));
  const build = put(path.join(buildDir, 'inputs.json'), { sources: helper.sourceClosure(), ui_artifact: { enabled: false } });
  const native = (label, extra) => {
    const name = 'iokaio.vcp.cs3.' + sha(label).slice(0, 32), sid = 'S-1-15-2-1-2-3-4-5-6-7', root = path.join(directory, name, 'AC');
    const events = [{ type: 'owned_server_started', origin: 'http://127.0.0.1:12345', network_scope: 'exact_ipv4_loopback_endpoint', browser_network_capabilities: 0 },
      { type: 'token', pid: 456, appcontainer: true, capabilities: 0, sid, owned_job: true },
      { type: 'owned_process', pid: 456, creation_filetime: 200, image: path.join(root, 'host/WebViewHost.exe'), token_verified: true },
      ...(label === 'cancel' ? [{ type: 'created_suspended', pid: 456, creation_filetime: 200, atomic_job_assignment: true }] : [])];
    return put(path.join(directory, label + '.json'), { schema: 'cs3-native-probe-receipt/1', inputs_sha256: build.sha256, name, sid, root,
      mode: 'webview2-dom', profile_created: true, worker_launch_attempted: true, worker_stderr_truncated: false, controller_pid: 123, controller_creation_filetime: 100, events,
      provider_calls: 0, cleanup_errors: [], primary_controller_failure: null, processes_drained: true, ...extra });
  };
  const pause = native('pause', { pause_before_resume_milliseconds: 1000, cancel_after_resume: false, outcome: 'dom_observed', events: [{ type: 'controller_pause_observed', milliseconds: 1000, created_process_still_suspended: true }] });
  const cancel = native('cancel', { cancel_after_resume: true, status: 'cleaned', outcome: 'cancelled_clean', runtime_unchanged: true, host_unchanged: true, policy_unchanged: true });
  const lost = native('lost', { status: 'owner_loss_recovered', outcome: 'owner_loss_recovered', owner_loss_recovered: true, pause_before_resume_milliseconds: 5000, cancel_after_resume: false,
    recovery_basis: 'Exact controller identity absent; nested kill-on-close job handles closed; exact AppContainer profile deletion succeeded.' });
  const owner = put(path.join(directory, 'owner.json'), { schema: 'cs3-web-owner-loss/1', inputs_sha256: build.sha256, receipt: lost.path, receipt_sha256: lost.sha256,
    controller_pid: 123, controller_creation_filetime: 100, outcome: 'owner_loss_recovered', processes_drained: true });
  const web = put(path.join(directory, 'web.json'), {}), matrix = put(path.join(directory, 'matrix.json'), {}), node = put(path.join(directory, 'node'), 'synthetic Node identity');
  Object.assign(pins, { new_ui_matrix_sha256: matrix.sha256, new_web_sha256: web.sha256, new_pause_sha256: pause.sha256, new_cancel_sha256: cancel.sha256, new_owner_loss_sha256: owner.sha256 });
  decision = f.saveDecision();
  const spec = { decision, historical_proof: proof, boundary: web, node_fixture: web, node, web: { ...web, native: pause, build }, web_evidence: [],
    pause: { receipt: pause, build }, cancel: { receipt: cancel, build }, owner_loss: { receipt: lost, build, result: owner }, ui_matrix: matrix,
    fresh_ui: historical.ui_outputs.map(row => ({ run_id: row.run_id, receipt: put(path.join(directory, row.run_id + '-browser.json'), { run_id: row.run_id,
      artifact_sha256: row.materialized.sha256, case_id: row.case_id, status: 'passed', assertions: row.browser.assertions, test_html: JSON.parse(f.bound(row.materialized))['index.html'] }) })) };
  return { ...f, historical, spec };
}
test('missing decision pins fail closed and legacy UI gate retains its original decision', t => {
  const helper = actual('./cs3-controller-recovery-qualification.cjs'), file = path.join(__dirname, '../../src/evals/skills/cs3-controller-recovery/qualification-decision.json');
  const production = () => helper.decision({ path: file, sha256: sha(fs.readFileSync(file)) });
  // The retained native evidence lives on Windows. Other hosts must reject its
  // drive path, while the synthetic absolute-root fixture below remains portable.
  if (process.platform === 'win32') assert.equal(production().schema, 'cs3-controller-recovery-qualification-decision/1');
  else assert.throws(production, /Separate preserved historical worktree/);
  const f = fixture(t); f.pins.new_native_source_sha256 = null; assert.throws(() => f.helper.uiDecision(f.saveDecision()), /pin/);
  assert.throws(() => actual('./cs3-comparison-gates.cjs').uiQualification({}, { path: file, sha256: sha(fs.readFileSync(file)) }), /matrix identity/);
});
test('historical proof invokes only source-authenticated modules and preserves every protected input', t => {
  const f = fixture(t), decision = f.saveDecision(), result = f.helper.historicalProjection(f.input, decision);
  assert.equal(result.dispositions.length, 6); assert.equal(result.ui_outputs.length, 6); assert.equal(result.model_calls, 0);
  assert.deepEqual(f.helper.historicalProjection(f.input, decision), result);
  const reviewFile = path.join(f.old, 'scripts/evals/cs3-comparison-review.cjs');
  assert.equal(require.cache[reviewFile], undefined, 'Authenticated exports must not survive to be mutated between reviews');
  let substituted = false;
  require.cache[reviewFile] = { exports: { validateDisposition: () => { substituted = true; return result.dispositions[0].observed; } } };
  assert.throws(() => f.helper.historicalProjection(f.input, decision), /cached historical/);
  assert.equal(substituted, false); delete require.cache[reviewFile];
  const disposition = result.dispositions[0].disposition;
  f.put(disposition.path, { ...JSON.parse(f.bound(disposition)), status: 'unqualified' });
  assert.throws(() => f.helper.historicalProjection(f.input, decision), /genuinely qualify/);
});
test('changed historical source, archive and unverified cached exports cannot replace genuine reviews', t => {
  let f = fixture(t); f.put(path.join(f.old, 'scripts/evals/cs3-comparison-review.cjs'), 'module.exports={}');
  assert.throws(() => f.helper.historicalProjection(f.input, f.saveDecision()), /source differs/);
  f = fixture(t); f.put(path.join(f.archive, 'extra'), 'undeclared'); assert.throws(() => f.helper.historicalProjection(f.input, f.saveDecision()), /unbound entries/);
  f = fixture(t); const file = path.join(f.old, 'scripts/evals/cs3-comparison-review.cjs'); require.cache[file] = { exports: { forged: true } };
  assert.throws(() => f.helper.historicalProjection(f.input, f.saveDecision()), /cached historical/);
  f = fixture(t); const dependency = path.join(f.old, 'scripts/evals/historical-dependency.cjs'); require.cache[dependency] = { exports: { forged: true } };
  assert.throws(() => f.helper.historicalProjection(f.input, f.saveDecision()), /cached historical/);
  f = fixture(t); f.pins.historical_root = f.root; assert.throws(() => f.helper.historicalProjection(f.input, f.saveDecision()), /Separate preserved/);
});
test('complete synthetic join binds new controls, historical qualification and exact retained UI bytes', t => {
  const f = finalFixture(t), result = f.helper.validate(f.spec);
  assert.equal(result.status, 'passed'); assert.equal(result.six_skills_qualified, true); assert.equal(result.fresh_ui.length, 6);
  assert.equal(result.historical_results_modified, false); assert.equal(result.model_calls, 0);
  // Pinning the already-produced proof cannot create a recursive decision digest.
  assert.deepEqual(f.helper.historicalProjection(f.input, f.spec.decision), f.historical);
});
test('null pins, substituted UI artifacts, failed new candidate and dirty native sources fail closed', t => {
  let f = finalFixture(t); f.pins.new_cancel_sha256 = null; f.spec.decision = f.saveDecision(); assert.throws(() => f.helper.validate(f.spec), /cancellation.*pin/);
  f = finalFixture(t); const row = f.spec.fresh_ui[0], receipt = JSON.parse(f.bound(row.receipt)); row.receipt = f.put(row.receipt.path, { ...receipt, artifact_sha256: 'a'.repeat(64) });
  assert.throws(() => f.helper.validate(f.spec), /historical artifact identity/);
  f = finalFixture(t); const candidate = f.spec.fresh_ui[2], value = JSON.parse(f.bound(candidate.receipt)); candidate.receipt = f.put(candidate.receipt.path, { ...value, status: 'failed' });
  assert.throws(() => f.helper.validate(f.spec), /reader evidence/);
  f = finalFixture(t); f.put(path.join(f.root, 'src/tests/support/windows/webapp/NativeProbe.cs'), 'changed native code'); assert.throws(() => f.helper.validate(f.spec), /native source.*pin/);
});
test('incomplete cancellation or substituted owner identity cannot qualify lifecycle recovery', t => {
  let f = finalFixture(t), value = JSON.parse(f.bound(f.spec.cancel.receipt)); f.spec.cancel.receipt = f.put(f.spec.cancel.receipt.path, { ...value, processes_drained: false });
  f.pins.new_cancel_sha256 = f.spec.cancel.receipt.sha256; f.spec.decision = f.saveDecision(); assert.throws(() => f.helper.validate(f.spec), /cleanup/);
  f = finalFixture(t); value = JSON.parse(f.bound(f.spec.owner_loss.result)); f.spec.owner_loss.result = f.put(f.spec.owner_loss.result.path, { ...value, controller_creation_filetime: 101 });
  f.pins.new_owner_loss_sha256 = f.spec.owner_loss.result.sha256; f.spec.decision = f.saveDecision(); assert.throws(() => f.helper.validate(f.spec), /owner-loss/);
});

test('summary-only cuts and changed reader-visible assertions cannot qualify', t => {
  const f = finalFixture(t), original = JSON.parse(f.bound(f.spec.cancel.receipt));
  for (const mutate of [v => { v.events = []; }, v => { v.events.find(e => e.type === 'token').capabilities = 1; },
    v => { v.events.find(e => e.type === 'created_suspended').creation_filetime++; }, v => { v.events.find(e => e.type === 'owned_process').image = path.join(f.directory, 'foreign.exe'); }]) {
    const value = structuredClone(original); mutate(value); f.spec.cancel.receipt = f.put(f.spec.cancel.receipt.path, value);
    f.pins.new_cancel_sha256 = f.spec.cancel.receipt.sha256; f.spec.decision = f.saveDecision();
    assert.throws(() => f.helper.validate(f.spec), /Cut token|atomic process|held process/);
  }
  f.spec.cancel.receipt = f.put(f.spec.cancel.receipt.path, original); f.pins.new_cancel_sha256 = f.spec.cancel.receipt.sha256; f.spec.decision = f.saveDecision();
  const row = f.spec.fresh_ui[0], value = JSON.parse(f.bound(row.receipt)); value.assertions[0].passed = false;
  row.receipt = f.put(row.receipt.path, value); assert.throws(() => f.helper.validate(f.spec), /reader evidence/);
});
