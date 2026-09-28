// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const gates = require('./cs3-comparison-gates.cjs');
const root = path.resolve(__dirname, '../..'), sha = value => crypto.createHash('sha256').update(value).digest('hex');
const hashFile = relative => sha(fs.readFileSync(path.join(root, relative)));
function temporary(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-cs3-gate-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  function ref(name, value) {
    const file = path.join(directory, name), bytes = typeof value === 'string' ? value : JSON.stringify(value);
    fs.writeFileSync(file, bytes); return { path: file, sha256: sha(bytes) };
  }
  return { directory, ref };
}
function denialFixture(t) {
  const { directory, ref } = temporary(t), binary = ref('Cs3BreakawayCanary.exe', 'synthetic never executed');
  const node = { sha256: 'a'.repeat(64) };
  const ids = ['owned_read', 'owned_write', 'host_read_0', 'host_write_0', 'host_read_1', 'host_write_1', 'host_read_2', 'host_write_2', 'host_read_3', 'host_write_3', 'junction_read', 'junction_write', 'staged_read', 'staged_write', 'ipv4_loopback', 'ipv6_loopback'];
  const receipt = { schema: 'cs3-native-denial-qualification/1', status: 'pass', model_calls: 0, cleanup: 'listeners_closed', inputs_unchanged: true, node_sha256: node.sha256,
    runner_sha256: hashFile('scripts/evals/cs3-boundary-qualification.ps1'), canary_sha256: hashFile('scripts/evals/cs3-boundary-canary.cjs'), fixture_sha256: hashFile('src/tests/support/windows/AppContainerFixture.cs'), breakaway_source_sha256: hashFile('src/tests/support/windows/Cs3BreakawayCanary.cs'), breakaway_binary_sha256: binary.sha256,
    profiles: Array.from({ length: 3 }, () => ({ state: 'removed' })), runs: ['control', 'restricted', 'control'].map(mode => {
      const restricted = mode === 'restricted';
      return { mode, listeners_verified: true, canaries_unchanged: true, profile_removed: true,
        token: { ExitCode: 0, AppContainer: restricted, TokenSidMatchesProfile: restricted, CapabilityCount: 0, RestrictedToken: false, IntegrityLevel: restricted ? 'S-1-16-4096' : 'S-1-16-8192' },
        rows: (restricted ? [...ids, 'external_ipv4', 'external_ipv6'] : ids).map(id => ({ id, outcome: !restricted || ['owned_read', 'owned_write', 'staged_read'].includes(id) ? 'allowed' : 'denied', code: 'EPERM' })),
        breakaway: { schema: 'cs3-breakaway-canary/1', created: false, win32_error: 5, job_flags: 8968 } };
    }) };
  return { receipt, node, reference: { path: path.join(directory, 'result.json') } };
}
test('denial admission reconstructs bracketed operations, tokens, cleanup and source identities', t => {
  const { receipt, reference, node } = denialFixture(t);
  assert.equal(gates.denial(receipt, reference, node).operations, 50);
  for (const mutate of [r => { r.status = 'passed'; }, r => { r.runs[1].rows[2].outcome = 'allowed'; }, r => { r.runs[1].rows.pop(); }, r => { r.runs[0].token.AppContainer = true; }, r => { r.runs[1].breakaway.created = true; }, r => { r.profiles[0].state = 'reserved'; }, r => { r.runner_sha256 = '0'.repeat(64); }]) {
    const bad = structuredClone(receipt); mutate(bad); assert.throws(() => gates.denial(bad, reference, node));
  }
});
function nodeFixture() {
  const controls = require('./cs3-comparison-controls.cjs').controls, node = { sha256: 'b'.repeat(64) };
  const receipt = { schema: 'cs3-node-oracle-controls/1', status: 'passed', model_calls: 0, executor: 'windows-appcontainer-node-fixture', node_sha256: node.sha256,
    source_sha256: hashFile('scripts/evals/cs3-comparison-controls.cjs'), oracle_sha256: hashFile('scripts/evals/cs3-comparison-oracle.cjs'), grader_sha256: hashFile('scripts/evals/developer-grader.cjs'),
    results: controls.map((control, index) => {
      const result = passed => ({ passed, executor: 'windows-appcontainer-node-fixture', checks: gates.nodeChecks[index].map((name, i) => ({ name, passed: passed || i !== 0 })) });
      return { case_id: control.id, passed: true, positive: result(true), negatives: control.mutations.map(() => result(false)) };
    }) };
  return { receipt, node };
}
test('native Node controls require all exact positive assertions and rejected mutants', () => {
  const { receipt, node } = nodeFixture();
  assert.equal(gates.nodeControls(receipt, node).rejected_mutants, 9);
  for (const mutate of [r => { r.results.pop(); }, r => { r.results[0].positive.checks.pop(); }, r => { r.results[0].negatives.pop(); }, r => { r.results[0].negatives[0].passed = true; }, r => { r.results[0].positive.checks[0].passed = false; }, r => { r.oracle_sha256 = '0'.repeat(64); }]) {
    const bad = structuredClone(receipt); mutate(bad); assert.throws(() => gates.nodeControls(bad, node));
  }
});
function isolatedWeb(adapter, retained, decision) {
  const filename = require.resolve('./cs3-comparison-gates.cjs'), actual = createRequire(filename), module = { exports: {} };
  const scopedRequire = name => name === './webapp-execution.cjs' ? adapter : name === './cs3-retained-ui-import.cjs' && retained ? retained : name.endsWith('/acceptance-decision.json') && decision ? decision : actual(name);
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(module.exports, scopedRequire, module, filename, path.dirname(filename));
  return module.exports;
}
test('WEB admission binds raw native/build and exact six projected rows, not summary flags', t => {
  const { ref, directory } = temporary(t), real = require('./webapp-execution.cjs');
  const sourceNames = ['HostContract.cs', 'HostContractTests.cs', 'WebViewHost.cs', 'FrozenWebHost.cs', 'FrozenWebResources.cs', 'FrozenWebEvidence.cs', 'UiArtifactResource.cs', 'UiArtifactHost.cs', 'UiArtifactEvidence.cs', 'WebDomContract.cs', 'WebDomContractTests.cs', 'NativeProbe.cs', 'WebViewSupervisor.cs', 'WorkerGuardian.cs', 'ProbeContract.cs', 'DomEvidence.cs', 'InputDiagnosticEvidence.cs', 'Invoke-NativeProbe.ps1', 'Input-Policy.ps1', 'Controller-helpers.ps1', 'Pe-Contract.ps1', 'Test-Contracts.ps1', 'Test-WorkerGuardian.ps1'];
  const native = ref('native.json', { test: 'opaque synthetic receipt; never accepted by real gradeNative' });
  const build = ref('inputs.json', { sources: sourceNames.map(name => ({ path: name, sha256: hashFile('src/tests/support/windows/webapp/' + name) })) });
  const nativeRows = ['WEB-normal-form-v1', 'WEB-normal-polling-v1', 'WEB-hostile-redirect-v1'].map(case_id => ({ case_id, status: 'passed', synthetic_test_only: true }));
  let calls = 0;
  const scoped = isolatedWeb({ ...real, gradeNative(receipt, source) { calls++; assert.equal(receipt.test.startsWith('opaque'), true); assert.equal(sha(source.bytes), build.sha256); assert.equal(source.expectedSha256, build.sha256); return nativeRows; } });
  const rows = [
    { case_id: 'WEB-boundary-occupied-port-v1', status: 'passed', outcome: 'pass', requested_port: 43127, collision: 'EADDRINUSE', user_listener_marker: 'KEEP_RUNNING', listener_preserved: true, candidate_server_claimed: false },
    { case_id: 'WEB-missing-browser-v1', status: 'expected_not_run', outcome: 'pass', browser_interaction: 'not_run', dom: 'not_run', accessibility: 'not_run', layout: 'not_run', lifecycle: 'not_run', installation: false, substitution: false },
    { case_id: 'WEB-near-miss-unit-v1', status: 'bug_detected', outcome: 'baseline_defect_detected', oracle: { pass: false, failed_assertions: 1 }, browser: 'not_applicable', files_modified: false }, ...nativeRows ];
  const receipt = { schema: 'cs3-webapp-execution/1', model_calls: 0, web_oracles: { schema: 'cs3-web-oracles/1', status: 'passed' }, ...real.embeddedInventory(), native_receipt_sha256: native.sha256, results: rows };
  const references = rows.map(row => ({ case_id: row.case_id, ...ref(row.case_id + '.json', row) }));
  const reference = { native, build };
  assert.equal(scoped.web(receipt, reference, references).cases, 6); assert.equal(calls, 1);
  const aggregate = { ...ref('aggregate.json', receipt), ...reference }, destination = path.join(directory, 'projections');
  const projection = scoped.projectWeb(aggregate, destination);
  assert.equal(projection.model_calls, 0); assert.equal(projection.web_evidence.length, 6);
  assert.equal(scoped.web(receipt, aggregate, projection.web_evidence).cases, 6);
  assert.throws(() => scoped.projectWeb(aggregate, destination), /New private/);
  assert.throws(() => gates.web(receipt, reference, references), /native receipt/);
  for (const mutate of [r => { r.results[0].listener_preserved = false; }, r => { r.results[1].installation = true; }, r => { r.results[2].status = 'passed'; }, r => { r.results[3].synthetic_test_only = false; }, r => { r.results.pop(); }, r => { r.native_receipt_sha256 = '0'.repeat(64); }]) {
    const bad = structuredClone(receipt); mutate(bad); assert.throws(() => scoped.web(bad, reference, references));
  }
  const swapped = structuredClone(references); [swapped[0].case_id, swapped[1].case_id] = [swapped[1].case_id, swapped[0].case_id];
  assert.throws(() => scoped.web(receipt, reference, swapped), /projection/);
  assert.throws(() => scoped.web(receipt, { ...reference, native: { ...native, sha256: '0'.repeat(64) } }, references), /changed/);
});
test('generic prerequisite summaries cannot satisfy raw evidence schemas', () => {
  const summary = { status: 'passed', model_calls: 0 };
  assert.throws(() => gates.denial(summary, {}, {}), /native denial/);
  assert.throws(() => gates.nodeControls(summary, {}), /native Node/);
  assert.throws(() => gates.web(summary, {}, []), /raw execution/);
  assert.throws(() => gates.uiQualification(summary, {}), /matrix identity/);
  assert.throws(() => gates.retainedUi(summary, {}), /Historical retained UI regrade validator is unavailable/);
  assert.throws(() => gates.retainedUi({ schema: 'cs3-retained-ui-import/1', status: 'prepared' }, {}), /unavailable/);
  assert.throws(() => gates.retainedUi({ schema: 'cs3-ui-artifact-browser/1', status: 'passed' }, {}), /unavailable/);
});

test('prospective UI admission authenticates approved exact controls and every native artifact/vector', t => {
  const { ref } = temporary(t), ui = require('./cs3-ui-artifact.cjs'), controls = require('./cs3-ui-controls.cjs');
  const decision = structuredClone(require('../../src/evals/skills/cs3-comparison/acceptance-decision.json'));
  const receipt = { schema: 'cs3-ui-control-qualification/1', status: 'passed', model_calls: 0, visual_review: 'not_run',
    operating_condition: 'serial native execution; no concurrent compiler/test workload', positive_controls: 6, negative_controls: 17,
    source_closure_sha256: sha(JSON.stringify(ui.sourceNames.map(name => ({ path: name, sha256: hashFile('src/tests/support/windows/webapp/' + name) })))),
    controls: ui.cases.flatMap(case_id => controls.variants(case_id).map(variant => {
      const files = controls.artifact(case_id, variant), failed = gates.uiFailures(case_id, variant);
      const documents = variant === 'transparent' ? 2 : case_id === ui.cases[0] ? 10 : 9;
      const assertions = ui.assertions(case_id).map(name => ({ name, passed: !failed.includes(name) }));
      const coverage = [{ type: 'job_process_coverage', complete: true, total_processes: 9, verified_identities: 9 }];
      return { case_id, variant, documents, expected_failed_assertions: failed, coverage, collector_diagnostics: [], receipt: {
        case_id, artifact_sha256: sha(JSON.stringify(files, null, 2) + '\n'), status: failed.length ? 'failed' : 'passed', assertions,
        native_receipt: ref(case_id + '-' + variant + '.json', { synthetic_test_only: variant, case_id, events: [...coverage, { type: 'ui_artifact_observed', documents }] }) } };
    })) };
  let calls = 0;
  const scoped = isolatedWeb({ validateUiArtifact(row, files, caseId) { calls++; assert.equal(row.case_id, caseId); assert.equal(typeof files['index.html'], 'string'); return row; } }, null, decision);
  const authenticate = value => {
    const reference = ref('matrix.json', value); decision.qualification_sha256 = reference.sha256;
    return scoped.uiQualification(value, reference);
  };
  const result = authenticate(receipt);
  assert.equal(result.controls, 23); assert.equal(calls, 23); assert.equal(result.historical_ui_regrade, 'not_run_originals_unavailable');
  const reference = ref('matrix.json', receipt); decision.qualification_sha256 = reference.sha256;
  assert.throws(() => gates.uiQualification(receipt, reference), /matrix identity/); // Real fixed approval never trusts synthetic controls.
  assert.throws(() => scoped.uiQualification({ ...receipt, status: 'failed' }, reference), /matrix identity/);
  for (const mutate of [r => { r.controls.pop(); }, r => { r.controls[1] = r.controls[0]; }, r => { r.source_closure_sha256 = '0'.repeat(64); },
    r => { r.controls[0].receipt.artifact_sha256 = '0'.repeat(64); }, r => { r.controls[1].expected_failed_assertions = []; },
    r => { r.controls[1].receipt.assertions[8].passed = true; }, r => { r.controls[0].coverage = []; }, r => { r.controls[0].documents = 1; },
    r => { r.controls[1].receipt.native_receipt = r.controls[0].receipt.native_receipt; }]) {
    const changed = structuredClone(receipt); mutate(changed); assert.throws(() => authenticate(changed));
  }
  const realValidator = isolatedWeb(require('./webapp-execution.cjs'), null, decision);
  const syntheticRef = ref('matrix.json', receipt); decision.qualification_sha256 = syntheticRef.sha256;
  assert.throws(() => realValidator.uiQualification(receipt, syntheticRef), /observed UI browser receipt/);
  assert.throws(() => isolatedWeb({}, null, decision).uiQualification(receipt, syntheticRef), /validator unavailable/);
});
