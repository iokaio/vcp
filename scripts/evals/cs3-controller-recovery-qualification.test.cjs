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
  for (const target of [root, old]) put(path.join(target, 'scripts/evals/fixtures/webapp/manifest.json'), { synthetic_frozen_web: true });
  put(path.join(root, 'scripts/evals/cs3-controller-recovery-qualification.cjs'), fs.readFileSync(helperFile));
  // These synthetic historical modules are not qualification evidence. The VM
  // substitutes only the archive digest/identity pins and strict native graders;
  // real module dependency authentication and read-only review invocation run.
  put(path.join(old, 'scripts/evals/cs3-comparison.cjs'), "module.exports={controlDirectory:p=>p.control_directory,arms:['none','nearest','candidate'],prompt:t=>t.request+'\\nexact synthetic suffix\\n'};");
  put(path.join(old, 'scripts/evals/historical-dependency.cjs'), 'module.exports={authenticated:true};');
  put(path.join(old, 'scripts/evals/cs3-comparison-review.cjs'), `const fs=require('node:fs'),path=require('node:path');require('./historical-dependency.cjs');
module.exports={validateDisposition:(file,hash,skill)=>JSON.parse(fs.readFileSync(path.join(JSON.parse(fs.readFileSync(file)).control_directory,'disposition-'+skill+'.json'))),
validateTerminalDisposition:file=>JSON.parse(fs.readFileSync(path.join(JSON.parse(fs.readFileSync(file)).control_directory,'terminal-disposition-skill-authoring.json'))),
block:file=>({result:JSON.parse(fs.readFileSync(file)).test_result})};`);
  put(path.join(old, 'scripts/evals/cs3-document-remediation.cjs'), `module.exports={priorTerminal:(ref,decision)=>({audit_sha256:ref.sha256,synthetic:true}),
tasks:()=>Array.from({length:6},(_,i)=>({id:'DOC-test-'+i,skill:'document-authoring',request:'Exact task '+i,nearest:['writing'],files:{'source.txt':'preserved '+i}})),
candidateRegistry:{inspect:()=>({entries:[{id:'document-authoring',version:'1.0.5'}]}),qualified:id=>'synthetic-doc::'+id},claimFile:()=>require('node:path').join(__dirname,'synthetic-doc-claim.json')};`);
  put(path.join(old, 'scripts/evals/cs3-runtime-amendment.cjs'), `const fs=require('node:fs'),path=require('node:path');module.exports={load:p=>JSON.parse(fs.readFileSync(p.isolated.manifest.path)),tasks:s=>s.test_tasks,project:(m,r,s)=>{if(m.test_add_transition)fs.writeFileSync(path.join(m.directory,'transitions','1.json'),JSON.stringify(m.test_add_transition));return m.test_groups[s];}};`);
  put(path.join(old, 'src/evals/skills/cs3-document-remediation/decision.json'), { synthetic: true });
  fs.cpSync(old, archive, { recursive: true });
  const scope = ['scripts', 'src/tests/support/windows/webapp', 'src/evals/skills/cs3-document-remediation'];
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
function mixedFixture(t) {
  const f = finalFixture(t), future = path.join(f.directory, 'future-skl'), archive = path.join(f.directory, 'future-skl-archive');
  fs.cpSync(f.old, future, { recursive: true });
  // This synthetic reviewed module deliberately depends on the separately
  // authenticated old reviewer, as real SKL admission does for its predecessor.
  f.put(path.join(future, 'scripts/evals/cs3-comparison-review.cjs'), `const fs=require('node:fs');const old=require(${JSON.stringify(path.join(f.old, 'scripts/evals/cs3-comparison-review.cjs'))});
module.exports={...old,validateDisposition:(file,hash,skill)=>{const plan=JSON.parse(fs.readFileSync(file));const value=old.validateDisposition(file,hash,skill);if(plan.test_mutate)fs.writeFileSync(plan.test_mutate,'mutated');return value;}};`);
  const frozenDecision = { ...structuredClone(f.pins), skill_remediation_lineage: { ...structuredClone(f.pins.skill_remediation_lineage) } };
  f.put(path.join(future, 'src/evals/skills/cs3-controller-recovery/qualification-decision.json'), frozenDecision);
  const scope = ['scripts', 'src/evals', 'src/tests/support/windows/webapp'], source = realPrep.identity(future, scope);
  fs.cpSync(future, archive, { recursive: true });
  const commit = '1'.repeat(40), archiveRef = f.put(path.join(f.directory, 'future-archive-receipt.json'), { schema: 'cs3-frozen-skill-remediation-source-archive/1', commit, archive, source,
    source_before_after_equal: true, archive_source_equal: true, model_calls: 0, claims_created: 0 });
  const selected = f.input.plans[0], plan = JSON.parse(f.bound(selected.plan));
  plan.source = source; plan.skill_remediation = { synthetic: true }; plan.spec.build_receipt.sha256 = '2'.repeat(64);
  plan.candidate_assets = { entries: [{ id: 'skill-authoring', version: '1.0.3' }], files: [{ path: 'skill-authoring/skill.json', sha256: '3'.repeat(64) }] };
  plan.spec.gates.ui_qualification.sha256 = f.pins.new_ui_matrix_sha256; plan.spec.gates.web_oracles.sha256 = f.pins.new_web_sha256;
  const skillPlan = f.put(path.join(f.directory, 'new-skl-plan.json'), plan);
  f.pins.skill_remediation_lineage = { archive_schema: 'cs3-frozen-skill-remediation-source-archive/1', candidate_version: '1.0.3', root: future, commit,
    source_sha256: source.content_sha256, archive_receipt_sha256: archiveRef.sha256, build_receipt_sha256: plan.spec.build_receipt.sha256,
    executable_sha256: f.pins.executable_sha256, candidate_inventory_sha256: sha(JSON.stringify(plan.candidate_assets)) };
  const input = { historical_archive: f.input.archive, skill_archive: archiveRef, plans: [{ skill: 'skill-authoring', plan: skillPlan }, ...f.input.plans.slice(1)] };
  return { ...f, future, futureArchive: archive, mixedInput: input, newPlan: plan, skillPlan, frozenDecision };
}
function freshFixture(t) {
  const f = mixedFixture(t), oldRow = f.input.plans[0], oldPlan = JSON.parse(f.bound(oldRow.plan));
  oldPlan.runtime_amendment = { synthetic: true }; oldRow.plan = f.put(oldRow.plan.path, oldPlan);
  const oldDisposition = f.put(path.join(oldPlan.control_directory, 'disposition-skill-authoring.json'), { status: 'unqualified' });
  const history = f.put(path.join(f.directory, 'old-skl-history.json'), { archive: f.input.archive,
    retirement: { path: path.join(f.directory, 'retirement.json'), sha256: '729cd66e7d190349960fbe5fc9d7adfac5dd431ca6310a632101691455929fb7' }, terminal_plan: oldRow.plan, terminal_disposition: oldDisposition });
  const shared = { history, allocation: { sha256: '4'.repeat(64) }, qualification: { sha256: '5'.repeat(64) }, runtime_preflight: { sha256: '6'.repeat(64) } };
  const plans = f.mixedInput.plans.map((row, index) => {
    const plan = JSON.parse(f.bound(row.plan));
    if (index === 0) {
      const base = path.join(f.directory, 'new-skl-evidence'); fs.cpSync(plan.directory, base, { recursive: true });
      plan.directory = base; plan.control_directory = path.join(base, 'control');
      f.put(path.join(plan.control_directory, 'disposition-skill-authoring.json'), { status: 'qualified', candidate_hard_gates: true, independent_blind_readers: 2, common_normal_wins: ['normal'], review_directory: path.join(base, 'review') });
    } else {
      plan.schema = 'cs3-friendli-transfer-plan/1'; plan.friendli_transfer = { skill: row.skill, manifest: { sha256: '7'.repeat(64) } };
      plan.candidate_assets = { entries: [{ id: row.skill, version: row.skill === 'document-authoring' ? '1.0.5' : '1.0.1' }] };
      f.pins.friendli_transfer_candidate_inventory_sha256[row.skill] = sha(JSON.stringify(plan.candidate_assets));
    }
    plan.source = f.newPlan.source; plan.spec.build_receipt.sha256 = f.pins.skill_remediation_lineage.build_receipt_sha256;
    plan.spec.gates.ui_qualification.sha256 = f.pins.new_ui_matrix_sha256; plan.spec.gates.web_oracles.sha256 = f.pins.new_web_sha256;
    plan.spec.skill_remediation = shared;
    return { skill: row.skill, plan: f.put(path.join(f.directory, 'fresh-' + row.skill + '-plan.json'), plan) };
  });
  return { ...f, freshInput: { ...f.mixedInput, plans }, shared };
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

test('SKL prerequisite projection authenticates actual preserved terminal reviewer, not a passed summary', t => {
  const f = fixture(t), decision = f.saveDecision(), selected = f.input.plans[0], value = JSON.parse(f.bound(selected.plan));
  value.runtime_amendment = { synthetic: true }; selected.plan = f.put(selected.plan.path, value);
  const disposition = f.put(path.join(value.control_directory, 'disposition-skill-authoring.json'), { status: 'unqualified' });
  const input = { archive: f.input.archive, retirement: { path: path.join(f.directory, 'retirement.json'), sha256: '729cd66e7d190349960fbe5fc9d7adfac5dd431ca6310a632101691455929fb7' }, terminal_plan: selected.plan, terminal_disposition: disposition };
  const result = f.helper.skillRemediationPrerequisites(input, decision); assert.equal(result.status, 'unqualified'); assert.equal(result.model_calls, 0);
  input.terminal_disposition = f.put(disposition.path, { status: 'qualified' }); assert.throws(() => f.helper.skillRemediationPrerequisites(input, decision), /authentic terminal failure/);
  input.terminal_disposition = f.put(path.join(value.control_directory, 'terminal-disposition-skill-authoring.json'), { status: 'terminal_unqualified' });
  assert.equal(f.helper.skillRemediationPrerequisites(input, decision).status, 'terminal_unqualified');
  const source = path.join(f.old, 'scripts/evals/cs3-document-remediation.cjs'); f.put(source, 'module.exports={priorTerminal:()=>({forged:true})}');
  assert.throws(() => f.helper.skillRemediationPrerequisites(input, decision), /source differs/);
});

test('native-only prerequisite projection retains exact lifecycle gates without claiming six-skill completion', t => {
  const f = finalFixture(t), { historical_proof, fresh_ui, ...spec } = f.spec;
  const result = f.helper.nativePrerequisites(spec); assert.equal(result.ui_qualification.controls, 23); assert.equal(result.six_skills_qualified, undefined);
  const native = JSON.parse(f.bound(spec.cancel.receipt)); native.events = []; spec.cancel.receipt = f.put(spec.cancel.receipt.path, native);
  f.pins.new_cancel_sha256 = spec.cancel.receipt.sha256; spec.decision = f.saveDecision(); assert.throws(() => f.helper.nativePrerequisites(spec), /Cut token/);
});

test('Friendli retirement projection retains literal old tasks and DOC arm order without replay or summary trust', t => {
  const f = fixture(t), selected = f.input.plans[0], terminal = JSON.parse(f.bound(selected.plan)), groups = f.input.plans.slice(1, 5);
  const tasks = groups.flatMap(group => Array.from({ length: 6 }, (_, i) => ({ id: group.skill + '-task-' + i, skill: group.skill, request: 'Literal old ' + i, files: { 'input.txt': 'literal\r\nbytes\n' } })));
  const plans = Object.fromEntries(groups.map(group => {
    const plan = JSON.parse(f.bound(group.plan)); plan.runs.forEach((run, i) => { run.case_id = group.skill + '-task-' + Math.floor(i / 3); });
    group.plan = f.put(path.join(plan.control_directory, 'plan.json'), plan); return [group.skill, plan];
  }));
  const sharedRoot = path.join(f.directory, 'runtime'); fs.mkdirSync(path.join(sharedRoot, 'transitions'), { recursive: true });
  const owner = { skill: 'skill-authoring', plan_sha256: selected.plan.sha256, manifest_sha256: 'b'.repeat(64) };
  f.put(path.join(sharedRoot, 'transitions/0.json'), { manifest_sha256: owner.manifest_sha256, ordinal: 0, from: null, to: owner });
  f.put(path.join(sharedRoot, 'active-skill.json'), owner);
  const manifest = { directory: sharedRoot, base: { source: terminal.source }, spec: { ...terminal.spec, remediation: { allocation: { path: 'synthetic', sha256: 'a'.repeat(64) } }, test_tasks: tasks }, test_groups: plans };
  const manifestRef = f.put(path.join(sharedRoot, 'manifest.json'), manifest);
  terminal.runtime_amendment = { synthetic: true }; terminal.isolated = { manifest: manifestRef }; selected.plan = f.put(selected.plan.path, terminal);
  const history = { archive: f.input.archive, retirement: { path: path.join(f.directory, 'retirement.json'), sha256: '729cd66e7d190349960fbe5fc9d7adfac5dd431ca6310a632101691455929fb7' }, terminal_plan: selected.plan,
    terminal_disposition: f.put(path.join(terminal.control_directory, 'disposition-skill-authoring.json'), { status: 'unqualified' }) };
  const decision = f.saveDecision(), input = { history, manifest: manifestRef };
  const result = f.helper.friendliRetirementPrerequisites(input, decision);
  assert.deepEqual(result.shared_controls.inventory.files.map(row => row.path).sort(), ['active-skill.json', 'manifest.json', 'transitions/0.json']);
  assert.deepEqual(result.groups.map(group => group.skill), groups.map(group => group.skill));
  assert.equal(result.groups[3].tasks[0].files['input.txt'], 'literal\r\nbytes\n');
  assert.equal(result.document.runs.length, 18); assert.deepEqual(result.document.runs.slice(3, 6).map(run => run.arm), ['nearest', 'candidate', 'none']);
  assert.equal(result.document.runs[0].prompt, 'Exact task 0\nexact synthetic suffix\n');
  assert.equal(result.document.tasks[0].files['source.txt'], 'preserved 0'); assert.equal(fs.existsSync(result.document.claim_path), false);
  const localHalt = path.join(plans['frontend-design'].control_directory, 'halt.json'); f.put(localHalt, { terminal: 'retired_undispatched' });
  assert.deepEqual(f.helper.friendliRetirementPrerequisites(input, decision), result, 'Group-local barriers do not invent a global halt or change inputs');
  const foreign = f.put(path.join(f.directory, 'foreign-manifest.json'), manifest);
  assert.throws(() => f.helper.friendliRetirementPrerequisites({ ...input, manifest: foreign }, decision), /shared runtime manifest location/);
  f.put(groups[0].plan.path, { ...plans['frontend-design'], runs: [] });
  assert.throws(() => f.helper.friendliRetirementPrerequisites(input, decision), /Retained transfer plan/);
  f.put(groups[0].plan.path, plans['frontend-design']);
  // Even a structurally valid transition cannot be silently appended while
  // projecting a previously authenticated terminal owner's evidence.
  manifest.test_add_transition = { manifest_sha256: owner.manifest_sha256, ordinal: 1, from: owner, to: { ...owner, skill: 'frontend-design' } };
  input.manifest = f.put(manifestRef.path, manifest); terminal.isolated.manifest = input.manifest;
  history.terminal_plan = f.put(selected.plan.path, terminal);
  assert.throws(() => f.helper.friendliRetirementPrerequisites(input, decision), /Shared runtime control evidence changed/);
});

test('mixed final proof authenticates five old reviews and one new SKL without a pin cycle', t => {
  const f = mixedFixture(t), before = fs.readFileSync(path.join(f.future, 'src/evals/skills/cs3-controller-recovery/qualification-decision.json'));
  const proof = f.helper.mixedHistoricalProjection(f.mixedInput, f.saveDecision());
  assert.equal(proof.dispositions.length, 6); assert.equal(proof.ui_outputs.length, 6);
  assert.notEqual(proof.lineages.historical.source_sha256, proof.lineages.skill_remediation.source_sha256);
  const ref = f.put(path.join(f.directory, 'mixed-proof.json'), proof); f.pins.historical_comparison_proof_sha256 = ref.sha256;
  f.spec.historical_proof = ref; f.spec.decision = f.saveDecision();
  assert.equal(f.helper.validate(f.spec).six_skills_qualified, true);
  assert.deepEqual(fs.readFileSync(path.join(f.future, 'src/evals/skills/cs3-controller-recovery/qualification-decision.json')), before);
  assert.equal(JSON.parse(before).skill_remediation_lineage.source_sha256, null, 'Frozen paid source never needs its future final-acceptance pins');
});

test('mixed lineage cannot substitute old SKL, another archive or mutable cached reviewer', t => {
  const f = mixedFixture(t), decision = f.saveDecision();
  const wrong = structuredClone(f.mixedInput); wrong.plans[0] = f.input.plans[0];
  assert.throws(() => f.helper.mixedHistoricalProjection(wrong, decision), /comparison identity/);
  wrong.plans[0] = f.mixedInput.plans[0]; wrong.skill_archive = f.input.archive;
  assert.throws(() => f.helper.mixedHistoricalProjection(wrong, decision), /skill_remediation archive/);
  const reviewer = path.join(f.future, 'scripts/evals/cs3-comparison-review.cjs'); let invoked = false;
  require.cache[reviewer] = { exports: { validateDisposition() { invoked = true; return { status: 'qualified' }; } } };
  assert.throws(() => f.helper.mixedHistoricalProjection(f.mixedInput, decision), /cached historical/); assert.equal(invoked, false); delete require.cache[reviewer];
  f.pins.skill_remediation_lineage.source_sha256 = null; assert.throws(() => f.helper.mixedHistoricalProjection(f.mixedInput, f.saveDecision()), /lineage pins/);
});

test('a new-lineage reviewer cannot mutate earlier old evidence during the mixed projection', t => {
  const f = mixedFixture(t), oldUi = JSON.parse(f.bound(f.mixedInput.plans[1].plan));
  f.newPlan.test_mutate = path.join(oldUi.directory, oldUi.runs[0].id, 'result.json');
  f.mixedInput.plans[0].plan = f.put(f.skillPlan.path, f.newPlan);
  assert.throws(() => f.helper.mixedHistoricalProjection(f.mixedInput, f.saveDecision()), /Mixed-lineage evidence changed/);
});

test('all-fresh final join authenticates six new reviews, original failure and unchanged frozen WEB bytes', t => {
  const f = freshFixture(t), proof = f.helper.freshHistoricalProjection(f.freshInput, f.saveDecision());
  assert.equal(proof.dispositions.length, 6); assert.equal(proof.historical.status, 'unqualified'); assert.equal(proof.ui_outputs.length, 6);
  assert.equal(proof.shared_prerequisites.qualification.sha256, f.shared.qualification.sha256);
  const ref = f.put(path.join(f.directory, 'fresh-proof.json'), proof); f.pins.historical_comparison_proof_sha256 = ref.sha256;
  f.spec.historical_proof = ref; f.spec.decision = f.saveDecision(); assert.equal(f.helper.validate(f.spec).six_skills_qualified, true);
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.future, 'src/evals/skills/cs3-controller-recovery/qualification-decision.json'))).skill_remediation_lineage.source_sha256, null);
});

test('all-fresh consumers cannot substitute qualification, old plans, roles, candidate pins or WEB inputs', t => {
  const f = freshFixture(t), decision = f.saveDecision(), row = f.freshInput.plans[1], original = JSON.parse(f.bound(row.plan));
  for (const field of ['qualification', 'runtime_preflight', 'allocation', 'history']) {
    const changed = structuredClone(original); changed.spec.skill_remediation[field] = { sha256: 'f'.repeat(64) };
    row.plan = f.put(row.plan.path, changed); assert.throws(() => f.helper.freshHistoricalProjection(f.freshInput, decision), /share exact funded/);
  }
  row.plan = f.put(row.plan.path, original); const changed = structuredClone(original); changed.friendli_transfer.skill = 'mcp-development';
  row.plan = f.put(row.plan.path, changed); assert.throws(() => f.helper.freshHistoricalProjection(f.freshInput, decision), /fixed Friendli transfer/);
  row.plan = f.put(row.plan.path, original); f.pins.friendli_transfer_candidate_inventory_sha256['frontend-design'] = null;
  assert.throws(() => f.helper.freshHistoricalProjection(f.freshInput, f.saveDecision()), /candidate inventory pin/);
  f.pins.friendli_transfer_candidate_inventory_sha256['frontend-design'] = sha(JSON.stringify(original.candidate_assets));
  const currentWeb = path.join(f.root, 'scripts/evals/fixtures/webapp/manifest.json'); f.put(currentWeb, { substituted: true });
  assert.throws(() => f.helper.freshHistoricalProjection(f.freshInput, f.saveDecision()), /WEB inputs changed/);
});

test('all-fresh qualification fails for a failed new disposition or predecessor mutation', t => {
  const f = freshFixture(t), row = f.freshInput.plans[5], plan = JSON.parse(f.bound(row.plan)), file = path.join(plan.control_directory, 'disposition-document-authoring.json');
  const original = fs.readFileSync(file); f.put(file, { ...JSON.parse(original), status: 'unqualified' });
  assert.throws(() => f.helper.freshHistoricalProjection(f.freshInput, f.saveDecision()), /genuinely qualify/); f.put(file, original);
  const history = JSON.parse(f.bound(f.shared.history)), terminal = JSON.parse(f.bound(history.terminal_plan));
  plan.test_mutate = path.join(terminal.directory, terminal.runs[0].id, 'result.json'); row.plan = f.put(row.plan.path, plan);
  assert.throws(() => f.helper.freshHistoricalProjection(f.freshInput, f.saveDecision()), /predecessor evidence changed/);
});
