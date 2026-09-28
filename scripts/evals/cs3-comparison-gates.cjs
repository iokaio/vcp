// SPDX-License-Identifier: Apache-2.0
'use strict';
// Read-only admission of raw prerequisite evidence. No summary can waive a gate.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const { plain, read, write, within, privateDirectory, noParentInstructions } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function bound(reference, maximum = 16 * 1024 * 1024) {
  if (!reference || !path.isAbsolute(reference.path || '') || !/^[a-f0-9]{64}$/.test(reference.sha256)) throw Error('Absolute hash-bound prerequisite required');
  const bytes = read(plain(reference.path), maximum);
  if (sha(bytes) !== reference.sha256) throw Error('Prerequisite evidence changed');
  return bytes;
}
function current(hash, relative) {
  if (hash !== sha(read(path.join(root, relative)))) throw Error('Prerequisite verifier source differs: ' + relative);
}
function denial(receipt, reference, node) {
  if (receipt.schema !== 'cs3-native-denial-qualification/1' || receipt.status !== 'pass' || receipt.model_calls !== 0 || receipt.cleanup !== 'listeners_closed' || receipt.inputs_unchanged !== true || receipt.node_sha256 !== node.sha256) throw Error('Complete native denial evidence required');
  for (const [key, source] of Object.entries({ runner_sha256: 'scripts/evals/cs3-boundary-qualification.ps1', canary_sha256: 'scripts/evals/cs3-boundary-canary.cjs', fixture_sha256: 'src/tests/support/windows/AppContainerFixture.cs', breakaway_source_sha256: 'src/tests/support/windows/Cs3BreakawayCanary.cs' })) current(receipt[key], source);
  bound({ path: path.join(path.dirname(reference.path), 'Cs3BreakawayCanary.exe'), sha256: receipt.breakaway_binary_sha256 });
  if (!Array.isArray(receipt.runs) || !equal(receipt.runs.map(run => run.mode), ['control', 'restricted', 'control']) || !Array.isArray(receipt.profiles) || receipt.profiles.length !== 3 || receipt.profiles.some(profile => profile.state !== 'removed')) throw Error('Bracketed native control/cleanup coverage differs');
  const ids = ['owned_read', 'owned_write', 'host_read_0', 'host_write_0', 'host_read_1', 'host_write_1', 'host_read_2', 'host_write_2', 'host_read_3', 'host_write_3', 'junction_read', 'junction_write', 'staged_read', 'staged_write', 'ipv4_loopback', 'ipv6_loopback'];
  for (const run of receipt.runs) {
    const restricted = run.mode === 'restricted', token = run.token;
    if (!token || token.ExitCode !== 0 || token.AppContainer !== restricted || token.TokenSidMatchesProfile !== restricted || token.CapabilityCount !== 0 || token.RestrictedToken !== false || token.IntegrityLevel !== (restricted ? 'S-1-16-4096' : 'S-1-16-8192') || !run.listeners_verified || !run.canaries_unchanged || !run.profile_removed) throw Error('Native token or cleanup differs');
    if (!Array.isArray(run.rows) || !equal(run.rows.map(row => row.id), restricted ? [...ids, 'external_ipv4', 'external_ipv6'] : ids)) throw Error('Native operation inventory differs');
    for (const row of run.rows) {
      const allowed = !restricted || ['owned_read', 'owned_write', 'staged_read'].includes(row.id);
      const loopbackDrop = ['ipv4_loopback', 'ipv6_loopback'].includes(row.id) && row.code === 'ETIMEDOUT' && Number.isFinite(row.elapsed_ms) && row.elapsed_ms >= 0 && row.elapsed_ms < 1500;
      if (row.outcome !== (allowed ? 'allowed' : 'denied') || !allowed && !['EACCES', 'EPERM'].includes(row.code) && !loopbackDrop) throw Error('Native operation outcome differs');
    }
    if (!run.breakaway || run.breakaway.schema !== 'cs3-breakaway-canary/1' || run.breakaway.created !== false || run.breakaway.win32_error !== 5 || run.breakaway.job_flags !== 8968) throw Error('Native no-breakaway evidence differs');
  }
  return { schema: receipt.schema, status: 'passed', runs: 3, operations: 50 };
}
const nodeChecks = [
  ['readiness and initialization state', 'complete parameter schemas and unknown methods', 'notification silence and malformed cancellation', 'invalid envelope and initialization parameters', 'ready notification requires valid negotiation and parameters', 'zero-valued second record', 'tool discovery exact schema and list parameters'],
  ['readiness and initialization state', 'complete parameter schemas and unknown methods', 'notification silence and malformed cancellation', 'invalid envelope and initialization parameters', 'ready notification requires valid negotiation and parameters', 'zero-valued second record', 'opaque cursor pages and bounds'],
  ['embedding request, ordering and usage', 'embedding inclusive Unicode scalar boundary and zero usage', 'transport error identity and no retry', 'embedding validation before transport', 'embedding malformed response without retries'],
  ['fragmented UTF-8, done and iterator cleanup', 'final non-LF done, empty lines and inclusive scalar limit', 'stream malformed input fails closed and closes iterator', 'stream cancellation and cleanup'],
  ['frozen ordinary-unit normalization oracle'],
];
function nodeControls(receipt, node) {
  const controls = require('./cs3-comparison-controls.cjs').controls;
  if (receipt.schema !== 'cs3-node-oracle-controls/1' || receipt.status !== 'passed' || receipt.model_calls !== 0 || receipt.executor !== 'windows-appcontainer-node-fixture' || receipt.node_sha256 !== node.sha256) throw Error('Qualified native Node oracle evidence required');
  for (const [key, source] of Object.entries({ source_sha256: 'scripts/evals/cs3-comparison-controls.cjs', oracle_sha256: 'scripts/evals/cs3-comparison-oracle.cjs', grader_sha256: 'scripts/evals/developer-grader.cjs' })) current(receipt[key], source);
  if (!Array.isArray(receipt.results) || !equal(receipt.results.map(row => row.case_id), controls.map(control => control.id))) throw Error('Node oracle case coverage differs');
  for (const [index, row] of receipt.results.entries()) {
    if (row.passed !== true || !Array.isArray(row.negatives) || row.negatives.length !== controls[index].mutations.length) throw Error('Node mutation coverage differs');
    for (const [position, result] of [row.positive, ...row.negatives].entries()) {
      if (!result || result.executor !== receipt.executor || !Array.isArray(result.checks) || !equal(result.checks.map(check => check.name), nodeChecks[index]) || result.checks.some(check => typeof check.passed !== 'boolean') || result.passed !== (position === 0) || result.checks.every(check => check.passed) !== result.passed) throw Error('Node positive/negative oracle evidence differs');
    }
  }
  return { schema: receipt.schema, status: 'passed', positives: 5, assertions: 24, rejected_mutants: 9 };
}
function authenticateWeb(receipt, reference) {
  const adapter = require('./webapp-execution.cjs'), inventory = adapter.embeddedInventory();
  if (receipt.schema !== 'cs3-webapp-execution/1' || receipt.model_calls !== 0 || !equal(receipt.web_oracles, { schema: 'cs3-web-oracles/1', status: 'passed' }) || receipt.manifest_sha256 !== inventory.manifest_sha256 || !equal(receipt.inventory, inventory.inventory)) throw Error('Frozen WEB raw execution identity differs');
  const native = JSON.parse(bound(reference.native)), buildBytes = bound(reference.build), build = JSON.parse(buildBytes), buildDirectory = path.dirname(reference.build.path);
  if (receipt.native_receipt_sha256 !== reference.native.sha256) throw Error('WEB native receipt association differs');
  const sourceNames = ['HostContract.cs', 'HostContractTests.cs', 'WebViewHost.cs', 'FrozenWebHost.cs', 'FrozenWebResources.cs', 'FrozenWebEvidence.cs', 'UiArtifactResource.cs', 'UiArtifactHost.cs', 'UiArtifactEvidence.cs', 'WebDomContract.cs', 'WebDomContractTests.cs', 'NativeProbe.cs', 'WebViewSupervisor.cs', 'WorkerGuardian.cs', 'ProbeContract.cs', 'DomEvidence.cs', 'InputDiagnosticEvidence.cs', 'Invoke-NativeProbe.ps1', 'Input-Policy.ps1', 'Controller-helpers.ps1', 'Pe-Contract.ps1', 'Test-Contracts.ps1', 'Test-WorkerGuardian.ps1'];
  if (!Array.isArray(build.sources) || !equal(build.sources.map(entry => entry.path), sourceNames) || build.ui_artifact?.enabled === true) throw Error('Exact non-UI WEB build source closure required');
  for (const entry of build.sources) {
    const source = path.join(root, 'src/tests/support/windows/webapp', entry.path);
    if (path.basename(entry.path) !== entry.path || !fs.existsSync(source)) throw Error('WEB build source inventory differs');
    current(entry.sha256, path.relative(root, source));
  }
  const nativeRows = adapter.gradeNative(native, { bytes: buildBytes, expectedSha256: reference.build.sha256, directory: buildDirectory });
  const port = require('./webapp-fixtures.cjs').candidateInput('WEB-boundary-occupied-port-v1').files.get('server.json');
  const nonBrowser = [
    { case_id: 'WEB-boundary-occupied-port-v1', status: 'passed', outcome: 'pass', requested_port: JSON.parse(port).requested_port, collision: 'EADDRINUSE', user_listener_marker: 'KEEP_RUNNING', listener_preserved: true, candidate_server_claimed: false },
    { case_id: 'WEB-missing-browser-v1', status: 'expected_not_run', outcome: 'pass', browser_interaction: 'not_run', dom: 'not_run', accessibility: 'not_run', layout: 'not_run', lifecycle: 'not_run', installation: false, substitution: false },
    { case_id: 'WEB-near-miss-unit-v1', status: 'bug_detected', outcome: 'baseline_defect_detected', oracle: { pass: false, failed_assertions: 1 }, browser: 'not_applicable', files_modified: false },
  ];
  const rows = [...nonBrowser, ...nativeRows];
  if (!equal(receipt.results, rows)) throw Error('WEB aggregate differs from exact mandatory case evidence');
  return rows;
}
function web(receipt, reference, caseReferences) {
  const rows = authenticateWeb(receipt, reference);
  if (!Array.isArray(caseReferences) || caseReferences.length !== 6 || new Set(caseReferences.map(ref => ref.case_id)).size !== 6) throw Error('Exact WEB projection coverage required');
  for (const row of rows) {
    const ref = caseReferences.find(ref => ref.case_id === row.case_id);
    if (!ref || !equal(JSON.parse(bound(ref)), row)) throw Error('WEB case projection is not its authenticated aggregate row');
  }
  return { schema: receipt.schema, status: 'passed', cases: rows.length, native_sha256: reference.native.sha256, build_sha256: reference.build.sha256 };
}
function retainedUi(receipt, reference) {
  const retained = require('./cs3-retained-ui-import.cjs');
  if (typeof retained.validateRegrade !== 'function') throw Error('Historical retained UI regrade validator is unavailable; imported or prospective UI evidence cannot waive this prerequisite');
  const result = retained.validateRegrade(receipt, reference);
  if (!result || result.status !== 'passed' || result.model_calls !== 0) throw Error('Historical retained UI regrade is incomplete');
  return result;
}
function uiFailures(caseId, variant) {
  const ui = require('./cs3-ui-artifact.cjs'), filter = caseId === ui.cases[0];
  if (['positive', 'poisoned-positive', 'inactive-motion'].includes(variant)) return [];
  if (['negative', 'poisoned-negative'].includes(variant)) return filter ? ['selection_persists_hidden'] : ['whitespace_validation', 'trimmed_success_no_reload'];
  if (variant === 'keyboard-decoy') return ['keyboard_reachability', 'focus_visible'];
  if (variant === 'hidden-feedback') return filter ? ['initial_state', 'filter_case_insensitive', 'selection_persists_hidden', 'clear_hidden_selection'] : ['empty_validation', 'whitespace_validation', 'trimmed_success_no_reload'];
  if (variant === 'transparent') return ui.assertions(caseId);
  if (variant === 'static-shadow') return ['focus_visible'];
  if (['pseudo-motion', 'mixed-motion'].includes(variant)) return ['reduced_motion'];
  if (variant === 'broken-submit' && !filter) return ['empty_validation'];
  throw Error('Unknown UI qualification variant');
}
function uiQualification(receipt, reference) {
  const decision = require('../../src/evals/skills/cs3-comparison/acceptance-decision.json');
  const ui = require('./cs3-ui-artifact.cjs'), controls = require('./cs3-ui-controls.cjs'), adapter = require('./webapp-execution.cjs');
  if (decision.schema !== 'cs3-prospective-ui-acceptance-decision/1' || decision.authority !== 'owner_explicit_approval_of_prospective_replacement'
    || decision.historical_ui_regrade !== 'not_run_originals_unavailable' || decision.replacement_admission_gate !== 'ui_qualification') throw Error('Explicit prospective replacement decision required');
  if (!reference || reference.sha256 !== decision.qualification_sha256 || !equal(JSON.parse(bound(reference)), receipt)) throw Error('Approved UI qualification matrix identity differs');
  if (typeof adapter.validateUiArtifact !== 'function') throw Error('Qualified UI browser validator unavailable');
  const expected = ui.cases.flatMap(case_id => controls.variants(case_id).map(variant => ({ case_id, variant })));
  if (receipt.schema !== 'cs3-ui-control-qualification/1' || receipt.status !== 'passed' || receipt.model_calls !== 0 || receipt.visual_review !== 'not_run'
    || receipt.operating_condition !== 'serial native execution; no concurrent compiler/test workload' || receipt.positive_controls !== 6 || receipt.negative_controls !== 17
    || !Array.isArray(receipt.controls) || !equal(receipt.controls.map(({ case_id, variant }) => ({ case_id, variant })), expected)) throw Error('Exact 23-control UI qualification required');
  const sources = ui.sourceNames.map(name => ({ path: name, sha256: sha(read(path.join(root, 'src/tests/support/windows/webapp', name))) }));
  if (receipt.source_closure_sha256 !== sha(JSON.stringify(sources))) throw Error('UI qualification source closure differs');
  const nativeHashes = new Set();
  for (const row of receipt.controls) {
    const files = controls.artifact(row.case_id, row.variant), failures = uiFailures(row.case_id, row.variant);
    const artifactHash = sha(JSON.stringify(files, null, 2) + '\n');
    if (!row.receipt || row.receipt.artifact_sha256 !== artifactHash || !equal(row.expected_failed_assertions, failures)) throw Error('UI control artifact or expected vector differs');
    const nativeBytes = bound(row.receipt.native_receipt), native = JSON.parse(nativeBytes);
    if (nativeHashes.has(row.receipt.native_receipt.sha256)) throw Error('Reused UI native control receipt');
    nativeHashes.add(row.receipt.native_receipt.sha256);
    const measured = adapter.validateUiArtifact(row.receipt, files, row.case_id);
    if (measured.status !== (failures.length ? 'failed' : 'passed') || !equal(measured.assertions, ui.assertions(row.case_id).map(name => ({ name, passed: !failures.includes(name) })))) throw Error('UI control measured assertion vector differs');
    const coverage = native.events.filter(event => event.type === 'job_process_coverage');
    const diagnostics = native.events.filter(event => ['collector_capture_failure', 'exited_image_query_deferred', 'job_pid_inventory_rejected'].includes(event.type));
    const observation = native.events.find(event => event.type === 'ui_artifact_observed');
    const documents = row.variant === 'transparent' ? 2 : row.case_id === ui.cases[0] ? 10 : 9;
    // Deferred image-query diagnostics are retained, not promoted into identity
    // evidence. The shared native grader requires complete held-job coverage.
    if (!equal(row.coverage, coverage) || !equal(row.collector_diagnostics, diagnostics) || row.documents !== documents || observation?.documents !== documents) throw Error('UI control lifecycle projection differs');
  }
  return { schema: receipt.schema, status: 'passed', controls: 23, positive_controls: 6, negative_controls: 17,
    decision_id: decision.decision_id, historical_ui_regrade: decision.historical_ui_regrade, model_calls: 0 };
}
function validate(spec) {
  const keys = ['browser_boundary', 'web_oracles', 'ui_qualification', 'node_fixture'];
  if (!spec.gates || !equal(Object.keys(spec.gates).sort(), keys.sort())) throw Error('All four prerequisite gates required');
  bound(spec.node, 128 * 1024 * 1024);
  const receipts = Object.fromEntries(Object.entries(spec.gates).map(([name, ref]) => [name, JSON.parse(bound(ref))]));
  const result = {
    browser_boundary: denial(receipts.browser_boundary, spec.gates.browser_boundary, spec.node),
    node_fixture: nodeControls(receipts.node_fixture, spec.node),
    web_oracles: web(receipts.web_oracles, spec.gates.web_oracles, spec.web_evidence),
  };
  result.ui_qualification = uiQualification(receipts.ui_qualification, spec.gates.ui_qualification);
  return result;
}
function projectWeb(reference, destination) {
  const receipt = JSON.parse(bound(reference)), rows = authenticateWeb(receipt, reference), directory = plain(path.resolve(destination));
  if (within(root, directory) || within(directory, root) || fs.existsSync(directory)) throw Error('New private WEB projection directory outside repository required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  fs.mkdirSync(directory, { mode: 0o700 });
  const web_evidence = rows.map(row => {
    const file = path.join(directory, row.case_id + '.json'); write(file, row);
    return { case_id: row.case_id, path: file, sha256: sha(read(file)) };
  });
  const projection = web(receipt, reference, web_evidence);
  const result = { schema: 'cs3-web-prerequisite-projection/1', model_calls: 0, gate: reference, web_evidence, validation: projection };
  write(path.join(directory, 'projection.json'), result);
  return result;
}
module.exports = { bound, denial, nodeControls, web, retainedUi, uiQualification, uiFailures, validate, projectWeb, nodeChecks };
if (require.main === module) {
  try {
    const [command, referenceFile, destination, ...extra] = process.argv.slice(2);
    if (command !== 'web-project' || !referenceFile || !destination || extra.length) throw Error('Usage: web-project HASH_BOUND_WEB_REFERENCE_JSON NEW_PRIVATE_DIRECTORY');
    process.stdout.write(JSON.stringify(projectWeb(JSON.parse(read(referenceFile)), destination)) + '\n');
  } catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
