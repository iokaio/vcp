// SPDX-License-Identifier: Apache-2.0
'use strict';
// Final, unpaid qualification join. Historical reviews execute only from the
// authenticated preserved source; no old plan, claim, result or receipt is edited.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), Module = require('node:module');
const { isDeepStrictEqual: equal } = require('node:util');
const prep = require('./authoring-prepare.cjs'), ui = require('./cs3-ui-artifact.cjs');
const gates = require('./cs3-comparison-gates.cjs');
const { plain, read, within, noParentInstructions, privateDirectory, write } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../..'), decisionFile = path.join(root, 'src/evals/skills/cs3-controller-recovery/qualification-decision.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const skills = ['skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing', 'document-authoring'];
const changedSources = ['Controller-helpers.ps1', 'Invoke-NativeProbe.ps1', 'Test-Contracts.ps1'];
function need(value, reason) { if (!value) throw Error(reason); }
function reference(file) { return { path: plain(path.resolve(file)), sha256: sha(read(file, 1024 * 1024 * 1024)) }; }
const bound = gates.bound, json = ref => JSON.parse(bound(ref));
function decision(ref) {
  const value = json(ref);
  need(equal(value, JSON.parse(read(decisionFile))) && value.schema === 'cs3-controller-recovery-qualification-decision/1'
    && value.historical_source_sha256 === '84d6b90185d7ea16f11eb548f843e78a97daa218bda0d4a55bc759b099727320'
    && value.historical_archive_receipt_sha256 === 'd69dcff6805cdd8accc31d0ade8aa4bd0417112c8a13f035851453f28c50095d'
    && value.historical_build_receipt_sha256 === 'aa1ab692434783358d419069dc88b17e708eed05b7ebaf7b285701a7e09aebb9'
    && value.executable_sha256 === '8650c1670b1c5079a3777f11321cd92b76aeaec4130616ee4df960bfae6bdafc'
    && value.historical_ui_matrix_sha256 === '818074b97799d7d1bf52852dc16ba704de49f6f7f763924b3b6d909d7fc25163'
    && value.historical_web_sha256 === 'a5bb97a72a07650d2682808de02d5f9fd71d445758167685652ab2ad6956dab5'
    && equal(value.allowed_native_source_changes, changedSources) && value.model_calls === 0
    && value.historical_results_modified === false && value.qualification_waiver === false, 'Exact source-bound recovery decision required');
  need(path.isAbsolute(value.historical_root) && plain(value.historical_root) !== root && !within(value.historical_root, root), 'Separate preserved historical worktree required');
  return value;
}
function pin(value, actual, label) { need(/^[a-f0-9]{64}$/.test(value || '') && value === actual, 'Missing or changed ' + label + ' pin'); }
function sourceClosure() { return ui.sourceNames.map(name => ({ path: name, sha256: sha(read(path.join(root, 'src/tests/support/windows/webapp', name))) })); }
function uiDecision(ref) {
  const approved = decision(ref), historical = JSON.parse(read(path.join(root, 'src/evals/skills/cs3-comparison/acceptance-decision.json')));
  pin(approved.historical_ui_matrix_sha256, historical.qualification_sha256, 'historical UI decision');
  pin(approved.new_native_source_sha256, sha(JSON.stringify(sourceClosure())), 'corrected native source');
  need(/^[a-f0-9]{64}$/.test(approved.new_ui_matrix_sha256 || ''), 'New UI matrix remains unqualified');
  return { ...historical, decision_id: approved.decision_id, qualification_sha256: approved.new_ui_matrix_sha256 };
}
function archivedSource(approved, archiveRef) {
  pin(approved.historical_archive_receipt_sha256, archiveRef?.sha256, 'historical archive');
  const archive = json(archiveRef), oldRoot = plain(approved.historical_root);
  need(archive.schema === 'cs3-frozen-preflight-supplement-source-archive/1' && archive.commit.startsWith(approved.historical_commit)
    && archive.source.content_sha256 === approved.historical_source_sha256 && archive.source_before_after_equal === true
    && archive.archive_source_equal === true && archive.model_calls === 0 && archive.claims_created === 0, 'Historical archive provenance differs');
  need(equal(prep.identity(plain(archive.archive), archive.source.scope), archive.source)
    && equal(prep.identity(oldRoot, archive.source.scope), archive.source), 'Preserved historical source differs');
  const inventory = prep.identity(archive.archive, ['.']), directories = new Set(archive.source.directories);
  for (const relative of [...directories, ...archive.source.files.map(item => item.path)])
    for (let parent = path.posix.dirname(relative); parent !== '.'; parent = path.posix.dirname(parent)) directories.add(parent);
  need(equal(inventory.files.map(item => item.path.slice(2)).sort(), archive.source.files.map(item => item.path).sort())
    && equal(inventory.directories.filter(item => item !== '.').map(item => item.slice(2)).sort(), [...directories].sort()), 'Historical archive has unbound entries');
  return { archive, oldRoot };
}
function historicalCall(approved, archiveRef, action) {
  const before = archivedSource(approved, archiveRef), allowed = new Map(before.archive.source.files.map(item => [path.resolve(before.oldRoot, item.path), item]));
  const loaded = new Map(), loading = new Set(), originalLoad = Module._load;
  // Each non-builtin dependency must belong to the hash-pinned source closure.
  // Previously injected/cached exports cannot stand in for reviewed source.
  Module._load = function (request, parent, isMain) {
    if (Module.isBuiltin(request)) return originalLoad.apply(this, arguments);
    const file = Module._resolveFilename(request, parent, isMain), entry = allowed.get(file);
    need(entry && plain(file) === file && fs.lstatSync(file).nlink === 1 && sha(read(file)) === entry.sha256, 'Historical dependency is outside authenticated source');
    need(!require.cache[file] || loading.has(file) || loaded.get(file) === require.cache[file], 'Unverified cached historical module');
    const alreadyLoading = loading.has(file);
    loading.add(file);
    try {
      const result = originalLoad.apply(this, arguments);
      if (require.cache[file]) loaded.set(file, require.cache[file]);
      return result;
    } finally { if (!alreadyLoading) loading.delete(file); }
  };
  try {
    const oldRequire = Module.createRequire(path.join(before.oldRoot, 'scripts/evals/cs3-comparison-review.cjs'));
    const result = action(oldRequire('./cs3-comparison-review.cjs'), oldRequire('./cs3-comparison.cjs'));
    for (const file of loaded.keys()) need(sha(read(file)) === allowed.get(file).sha256, 'Historical dependency changed during review');
    need(equal(archivedSource(approved, archiveRef), before), 'Historical source changed during review');
    return result;
  } finally {
    Module._load = originalLoad;
    // No exports from this authenticated invocation survive for adoption by a
    // later call. Caller-injected caches are rejected, never silently evicted.
    for (const [file, value] of loaded) if (require.cache[file] === value) delete require.cache[file];
  }
}
function historicalProjection(input, decisionRef) {
  const approved = decision(decisionRef);
  need(equal(Object.keys(input).sort(), ['archive', 'plans']) && Array.isArray(input.plans)
    && equal(input.plans.map(row => row.skill), skills), 'Exact six historical plans required');
  return historicalCall(approved, input.archive, (review, campaign) => {
    const parsed = input.plans.map(row => ({ ...row, value: json(row.plan) })), protectedRoots = new Set();
    for (const row of parsed) {
      need(row.value.source.content_sha256 === approved.historical_source_sha256 && row.value.spec.build_receipt.sha256 === approved.historical_build_receipt_sha256
        && row.value.spec.executable.sha256 === approved.executable_sha256 && row.value.runs.length === 18
        && row.value.runs.every(run => run.skill === row.skill), 'Historical comparison identity differs');
      pin(approved.historical_ui_matrix_sha256, row.value.spec.gates.ui_qualification.sha256, 'historical UI gate');
      pin(approved.historical_web_sha256, row.value.spec.gates.web_oracles.sha256, 'historical WEB gate');
      protectedRoots.add(plain(campaign.controlDirectory(row.value)));
      for (const run of row.value.runs) { need(/^[A-Za-z0-9_-]+$/.test(run.id), 'Unsafe historical slot'); protectedRoots.add(plain(path.join(row.value.directory, run.id))); }
      const retained = json(reference(path.join(campaign.controlDirectory(row.value), 'disposition-' + row.skill + '.json')));
      protectedRoots.add(plain(retained.review_directory));
    }
    const roots = [...protectedRoots].filter(item => ![...protectedRoots].some(parent => parent !== item && within(parent, item))).sort();
    const snapshot = () => roots.map(directory => ({ directory, inventory: prep.identity(directory, ['.']) }));
    const before = snapshot(), uiOutputs = [];
    const dispositions = parsed.map(row => {
      const disposition = review.validateDisposition(row.plan.path, row.plan.sha256, row.skill);
      need(disposition.status === 'qualified' && disposition.candidate_hard_gates === true && disposition.independent_blind_readers === 2
        && disposition.common_normal_wins.length > 0 && !disposition.block_security_failure, 'All six historical comparisons must genuinely qualify');
      const { result } = review.block(row.plan.path, row.plan.sha256, row.skill);
      const browser = row.skill === 'frontend-design' ? json(disposition.browser_grades) : null;
      if (row.skill === 'frontend-design') for (const run of result.runs.filter(run => ui.cases.includes(run.case_id))) {
        const base = path.join(row.value.directory, run.id), file = path.join(base, 'materialized-files.json');
        const record = { run_id: run.id, case_id: run.case_id, arm: run.arm, canonical_result: reference(path.join(base, 'result.json')) };
        if (fs.existsSync(file)) {
          record.materialized = reference(file);
          const grade = browser.runs.find(item => item.run_id === run.id), observed = json(grade.receipt);
          need(grade.artifact_sha256 === record.materialized.sha256 && observed.artifact_sha256 === record.materialized.sha256, 'Historical browser artifact differs');
          record.browser = { receipt: grade.receipt, status: observed.status, assertions: observed.assertions };
        }
        else { need(run.status === 'failed' && !!run.output_error, 'Missing historical UI artifact is not a canonical failure'); record.status = 'not_run_output_invalid'; }
        uiOutputs.push(record);
      }
      return { skill: row.skill, plan: row.plan, candidate_assets: row.value.candidate_assets,
        disposition: reference(path.join(campaign.controlDirectory(row.value), 'disposition-' + row.skill + '.json')), observed: disposition };
    });
    need(uiOutputs.length === 6 && new Set(uiOutputs.map(row => row.run_id)).size === 6, 'Six historical UI normal slots required');
    need(equal(snapshot(), before), 'Historical comparison evidence changed during review');
    for (const row of parsed) need(equal(json(row.plan), row.value), 'Historical plan changed during review');
    return { schema: 'cs3-controller-recovery-historical-proof/1', input, source_sha256: approved.historical_source_sha256,
      executable_sha256: approved.executable_sha256, build_receipt_sha256: approved.historical_build_receipt_sha256,
      dispositions, ui_outputs: uiOutputs, protected_inventories: before, model_calls: 0, historical_results_modified: false };
  });
}
function native(ref) {
  const receipt = json(ref.receipt), bytes = bound(ref.build), build = JSON.parse(bytes), directory = path.dirname(ref.build.path), sources = sourceClosure();
  need(receipt.schema === 'cs3-native-probe-receipt/1' && receipt.inputs_sha256 === ref.build.sha256 && build.ui_artifact?.enabled !== true
    && equal(build.sources, sources), 'Corrected non-UI lifecycle source identity differs');
  for (const item of sources) need(sha(read(path.join(directory, item.path))) === item.sha256, 'Staged lifecycle source differs');
  need(receipt.provider_calls === 0 && Array.isArray(receipt.cleanup_errors) && !receipt.cleanup_errors.length
    && receipt.primary_controller_failure === null && receipt.processes_drained === true && !fs.existsSync(plain(receipt.root)), 'Lifecycle cleanup is not independently complete');
  return { receipt, build };
}
function cutIdentity(receipt, suspended) {
  need(receipt.mode === 'webview2-dom' && receipt.profile_created === true && receipt.worker_launch_attempted === true
    && receipt.worker_stderr_truncated === false && /^iokaio\.vcp\.cs3\.[a-f0-9]{32}$/.test(receipt.name)
    && /^S-1-15-2-(?:\d+-){6}\d+$/.test(receipt.sid) && path.basename(receipt.root) === 'AC'
    && path.basename(path.dirname(receipt.root)) === receipt.name && Number.isInteger(receipt.controller_pid) && receipt.controller_pid > 0
    && Number.isInteger(receipt.controller_creation_filetime) && receipt.controller_creation_filetime > 0, 'Exact native cut identity required');
  need(Array.isArray(receipt.events) && !receipt.events.some(event => ['primary_failure', 'host_observation_rejected', 'cleanup_failure'].includes(event.type)), 'Failed cut event present');
  const owned = receipt.events.filter(event => event.type === 'owned_process'), tokens = receipt.events.filter(event => event.type === 'token');
  need(owned.length > 0 && owned.length <= 128 && tokens.length > 0 && tokens.every(event => event.appcontainer === true && event.capabilities === 0
    && event.sid === receipt.sid && event.owned_job === true && owned.some(item => item.pid === event.pid)), 'Cut token/job evidence incomplete');
  need(owned.every(event => Number.isInteger(event.pid) && event.pid > 0 && Number.isInteger(event.creation_filetime) && event.creation_filetime > 0
    && event.token_verified === true && path.isAbsolute(event.image) && path.resolve(event.image) === path.join(receipt.root, 'host', 'WebViewHost.exe')
    && tokens.some(item => item.pid === event.pid)), 'Cut held process identity incomplete');
  const server = receipt.events.filter(event => event.type === 'owned_server_started');
  need(server.length === 1 && /^http:\/\/127\.0\.0\.1:[1-9]\d{0,4}$/.test(server[0].origin)
    && Number(new URL(server[0].origin).port) <= 65535 && server[0].network_scope === 'exact_ipv4_loopback_endpoint'
    && server[0].browser_network_capabilities === 0, 'Cut owned server identity differs');
  if (suspended) {
    const created = receipt.events.filter(event => event.type === 'created_suspended');
    need(created.length === 1 && created[0].atomic_job_assignment === true && owned.some(event => event.pid === created[0].pid
      && event.creation_filetime === created[0].creation_filetime), 'Cancellation atomic process identity missing');
  }
}
function lifecycle(spec, approved) {
  pin(approved.new_pause_sha256, spec.pause.receipt.sha256, 'pause'); pin(approved.new_cancel_sha256, spec.cancel.receipt.sha256, 'cancellation');
  pin(approved.new_owner_loss_sha256, spec.owner_loss.result.sha256, 'owner loss');
  need(equal(spec.pause.receipt, spec.web.native) && equal(spec.pause.build, spec.web.build), 'Pause must be the exact completed WEB run');
  const pause = native(spec.pause).receipt, cancel = native(spec.cancel).receipt, lost = native(spec.owner_loss).receipt, outer = json(spec.owner_loss.result);
  cutIdentity(cancel, true); cutIdentity(lost, false);
  need(new Set([spec.pause.receipt.sha256, spec.cancel.receipt.sha256, spec.owner_loss.receipt.sha256]).size === 3, 'Distinct lifecycle observations required');
  need(pause.pause_before_resume_milliseconds === 1000 && pause.cancel_after_resume === false && pause.outcome === 'dom_observed'
    && pause.events.filter(event => event.type === 'controller_pause_observed' && event.milliseconds === 1000 && event.created_process_still_suspended === true).length === 1, 'Bounded pre-resume pause missing');
  need(cancel.cancel_after_resume === true && cancel.status === 'cleaned' && cancel.outcome === 'cancelled_clean'
    && cancel.runtime_unchanged === true && cancel.host_unchanged === true && cancel.policy_unchanged === true, 'Clean explicit cancellation required');
  need(outer.schema === 'cs3-web-owner-loss/1' && outer.inputs_sha256 === spec.owner_loss.build.sha256
    && outer.receipt === spec.owner_loss.receipt.path && outer.receipt_sha256 === spec.owner_loss.receipt.sha256
    && outer.controller_pid === lost.controller_pid && outer.controller_creation_filetime === lost.controller_creation_filetime
    && outer.outcome === 'owner_loss_recovered' && outer.processes_drained === true && lost.status === 'owner_loss_recovered'
    && lost.outcome === 'owner_loss_recovered' && lost.owner_loss_recovered === true
    && lost.pause_before_resume_milliseconds === 5000 && lost.cancel_after_resume === false
    && lost.recovery_basis === 'Exact controller identity absent; nested kill-on-close job handles closed; exact AppContainer profile deletion succeeded.'
    && lost.events.some(event => event.type === 'owned_process' && event.token_verified === true), 'Exact corrected owner-loss observation required');
  return { pause: spec.pause, cancel: spec.cancel, owner_loss: spec.owner_loss };
}
function validate(spec) {
  need(equal(Object.keys(spec).sort(), ['boundary', 'cancel', 'decision', 'fresh_ui', 'historical_proof', 'node', 'node_fixture', 'owner_loss', 'pause', 'ui_matrix', 'web', 'web_evidence']), 'Exact final recovery qualification inputs required');
  const verifierScope = ['scripts/evals', 'scripts/skills/builtin-assets.cjs', 'src/evals/skills/cs3-controller-recovery'];
  const verifierBefore = prep.identity(root, verifierScope), nativeBefore = sourceClosure();
  const approved = decision(spec.decision); uiDecision(spec.decision);
  pin(approved.historical_comparison_proof_sha256, spec.historical_proof.sha256, 'historical comparison proof');
  pin(approved.new_web_sha256, spec.web.sha256, 'new WEB'); pin(approved.new_ui_matrix_sha256, spec.ui_matrix.sha256, 'new UI matrix');
  const historical = json(spec.historical_proof);
  need(equal(historicalProjection(historical.input, spec.decision), historical), 'Historical proof does not derive from immutable source and actual reviews');
  const archived = archivedSource(approved, historical.input.archive), oldFiles = new Map(archived.archive.source.files.map(item => [item.path, item.sha256]));
  const changes = sourceClosure().filter(item => oldFiles.get('src/tests/support/windows/webapp/' + item.path) !== item.sha256).map(item => item.path).sort();
  need(equal(changes, changedSources), 'Recovery qualification changed unrelated native source');
  bound(spec.node, 128 * 1024 * 1024);
  const denial = gates.denial(json(spec.boundary), spec.boundary, spec.node), nodeControls = gates.nodeControls(json(spec.node_fixture), spec.node);
  const web = gates.web(json(spec.web), spec.web, spec.web_evidence), matrix = gates.recoveryUiQualification(json(spec.ui_matrix), spec.ui_matrix, spec.decision);
  const clean = lifecycle(spec, approved);
  need(Array.isArray(spec.fresh_ui) && equal(spec.fresh_ui.map(row => row.run_id), historical.ui_outputs.map(row => row.run_id)), 'Exact retained UI normal inventory required');
  const fresh = spec.fresh_ui.map((row, index) => {
    const original = historical.ui_outputs[index];
    if (!original.materialized) { need(equal(row, { run_id: original.run_id, status: 'not_run_output_invalid', canonical_result: original.canonical_result }), 'Invalid historical output cannot acquire an invented browser grade'); return row; }
    const files = json(original.materialized), receipt = json(row.receipt);
    need(equal(Object.keys(row).sort(), ['receipt', 'run_id']) && receipt.run_id === original.run_id
      && receipt.artifact_sha256 === original.materialized.sha256, 'Fresh browser grade changed historical artifact identity');
    const measured = ui.validateUiArtifact(receipt, files, original.case_id);
    need(measured.status === original.browser.status && equal(measured.assertions, original.browser.assertions), 'Fresh UI semantics changed the historical reader evidence');
    need(original.arm !== 'candidate' || measured.status === 'passed', 'Corrected browser gate reopened the candidate');
    return { run_id: original.run_id, materialized: original.materialized, receipt: row.receipt, measured };
  });
  need(equal(historicalProjection(historical.input, spec.decision), historical), 'Historical comparison evidence changed during final qualification');
  need(equal(sourceClosure(), nativeBefore) && equal(prep.identity(root, verifierScope), verifierBefore), 'Recovery verifier or native source changed during qualification');
  return { schema: 'cs3-controller-recovery-qualification/1', status: 'passed', decision: spec.decision, historical_proof: spec.historical_proof,
    verifier_source: verifierBefore, source_closure_sha256: approved.new_native_source_sha256, six_skills_qualified: true, denial, node: nodeControls, web, matrix, lifecycle: clean, fresh_ui: fresh,
    model_calls: 0, historical_results_modified: false, visual_review: 'not_run' };
}
function project(inputFile, outputFile) {
  const input = JSON.parse(read(inputFile)), approved = decision(input.decision), output = plain(path.resolve(outputFile));
  need(!within(root, output) && !within(approved.historical_root, output), 'Historical proof must be a new private external file');
  privateDirectory(path.dirname(output)); noParentInstructions(path.dirname(output));
  need(equal(Object.keys(input).sort(), ['archive', 'decision', 'plans']), 'Exact historical projection inputs required');
  const proof = historicalProjection({ archive: input.archive, plans: input.plans }, input.decision);
  need(!proof.protected_inventories.some(item => within(item.directory, output)), 'Proof cannot mutate protected comparison inventory');
  write(output, proof); return reference(output);
}
module.exports = { decision, uiDecision, sourceClosure, historicalProjection, validate, project };
if (require.main === module) {
  try { const [command, first, second] = process.argv.slice(2); const result = command === 'project-history' ? project(first, second) : command === 'validate' ? validate(JSON.parse(read(first))) : (() => { throw Error('Usage: project-history INPUT NEW_PRIVATE_FILE | validate SPEC'); })(); process.stdout.write(JSON.stringify(result, null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
