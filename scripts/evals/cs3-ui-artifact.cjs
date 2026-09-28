// SPDX-License-Identifier: Apache-2.0
'use strict';
// Model HTML is compiled as bounded Base64 data, never as C# or host JavaScript.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const cases = ['UI-cs3-filter-selection-v1', 'UI-cs3-disclosure-form-v1'];
const sourceNames = ['HostContract.cs', 'HostContractTests.cs', 'WebViewHost.cs', 'FrozenWebHost.cs', 'FrozenWebResources.cs', 'FrozenWebEvidence.cs', 'UiArtifactResource.cs', 'UiArtifactHost.cs', 'UiArtifactEvidence.cs', 'WebDomContract.cs', 'WebDomContractTests.cs', 'NativeProbe.cs', 'WebViewSupervisor.cs', 'WorkerGuardian.cs', 'ProbeContract.cs', 'DomEvidence.cs', 'InputDiagnosticEvidence.cs', 'Invoke-NativeProbe.ps1', 'Input-Policy.ps1', 'Controller-helpers.ps1', 'Pe-Contract.ps1', 'Test-Contracts.ps1', 'Test-WorkerGuardian.ps1'];
const common = ['keyboard_reachability', 'focus_visible', 'layout_320', 'layout_1024', 'reduced_motion', 'no_outbound_request'];
function assertions(caseId) {
  if (!cases.includes(caseId)) throw Error('Unknown UI artifact case');
  return [...common, ...(caseId === cases[0]
    ? ['initial_state', 'filter_case_insensitive', 'selection_persists_hidden', 'clear_hidden_selection', 'no_match']
    : ['initial_state', 'disclosure_relationship', 'empty_validation', 'whitespace_validation', 'trimmed_success_no_reload'])];
}
function htmlBytes(caseId, files) {
  assertions(caseId);
  if (!files || typeof files !== 'object' || Array.isArray(files) || !equal(Object.keys(files), ['index.html']) || typeof files['index.html'] !== 'string') throw Error('Exactly one index.html artifact required');
  const bytes = Buffer.from(files['index.html'], 'utf8');
  if (!bytes.length || bytes.length > 65536 || bytes.toString('utf8') !== files['index.html']) throw Error('UI artifact must be 1..65536 valid UTF-8 bytes');
  return bytes;
}
function renderResource(caseId, files, artifactHash) {
  const bytes = htmlBytes(caseId, files);
  if (!/^[a-f0-9]{64}$/.test(artifactHash)) throw Error('Exact artifact inventory hash required');
  return '// SPDX-License-Identifier: Apache-2.0\n' +
    '// Generated bounded data; never edit or execute it as host code.\n' +
    'namespace Vcp.Qualification.Webapp {\npublic static class UiArtifactResource {\n' +
    'public static readonly bool Enabled=true;\n' +
    `public static readonly string CaseId="${caseId}";\n` +
    `public static readonly string ArtifactSha256="${artifactHash}";\n` +
    `public static readonly FrozenWebResource Resource=new FrozenWebResource("ui-artifact","/ui-artifact/index.html",200,"text/html; charset=utf-8","${bytes.toString('base64')}");\n` +
    '}\n}\n';
}
function compile(caseId, file) {
  if (!path.isAbsolute(file)) throw Error('Absolute materialized artifact inventory required');
  const raw = read(plain(file), 4 * 1024 * 1024), files = JSON.parse(raw), bytes = htmlBytes(caseId, files), artifactHash = sha(raw);
  const source = renderResource(caseId, files, artifactHash);
  return { schema: 'cs3-ui-compiled-resource/1', case_id: caseId, artifact_sha256: artifactHash,
    html_sha256: sha(bytes), html_bytes: bytes.length, source_sha256: sha(source), source };
}
function bound(reference, maximum = 16 * 1024 * 1024) {
  if (!reference || !path.isAbsolute(reference.path || '') || !/^[a-f0-9]{64}$/.test(reference.sha256)) throw Error('Absolute hash-bound UI evidence required');
  const bytes = read(reference.path, maximum);
  if (sha(bytes) !== reference.sha256) throw Error('UI evidence changed');
  return bytes;
}
function validateSources(inputs, directory, expectedResource) {
  if (!Array.isArray(inputs.sources) || !equal(inputs.sources.map(entry => entry.path), sourceNames)) throw Error('Exact UI native source closure required');
  for (const entry of inputs.sources) {
    const staged = read(path.join(directory, entry.path));
    const reviewed = read(path.resolve(__dirname, '../../src/tests/support/windows/webapp', entry.path));
    const generated = entry.path === 'UiArtifactResource.cs';
    if (sha(staged) !== entry.sha256 || (generated ? entry.template_sha256 : entry.sha256) !== sha(reviewed)) throw Error('UI native source differs from reviewed source');
    if (generated ? staged.toString('utf8') !== expectedResource : Object.hasOwn(entry, 'template_sha256')) throw Error('UI compiled resource or source template differs');
  }
}
function validateUiArtifact(receipt, files, caseId) {
  const html = htmlBytes(caseId, files), names = assertions(caseId);
  if (!receipt || receipt.schema !== 'cs3-ui-artifact-browser/1' || receipt.case_id !== caseId || !/^[a-f0-9]{64}$/.test(receipt.artifact_sha256) || !['passed', 'failed'].includes(receipt.status)
    || receipt.cleanup !== 'completed' || receipt.containment !== 'qualified' || receipt.visual_review !== 'not_run') throw Error('Exact observed UI browser receipt required');
  const native = JSON.parse(bound(receipt.native_receipt)), buildBytes = bound(receipt.build), inputs = JSON.parse(buildBytes), directory = path.dirname(receipt.build.path);
  validateSources(inputs, directory, renderResource(caseId, files, receipt.artifact_sha256));
  // This independently validates WEB state values, exact server bytes, held
  // process coverage, runtime identity, profile deletion and clean termination.
  require('./webapp-execution.cjs').gradeNative(native, { bytes: buildBytes, expectedSha256: receipt.build.sha256, directory });
  const ui = inputs.ui_artifact;
  if (!ui || ui.enabled !== true || ui.case_id !== caseId || ui.artifact_sha256 !== receipt.artifact_sha256 || ui.html_sha256 !== sha(html) || ui.html_bytes !== html.length) throw Error('Compiled UI artifact inventory differs');
  const observations = native.events.filter(event => event.type === 'ui_artifact_observed');
  if (observations.length !== 1) throw Error('One independent native UI observation required');
  const observed = observations[0];
  if (observed.case_id !== caseId || observed.artifact_sha256 !== receipt.artifact_sha256 || observed.status !== receipt.status || observed.visual_review !== 'not_run' || !equal(observed.assertions, receipt.assertions)) throw Error('UI grade differs from independent native observation');
  if (!Array.isArray(observed.assertions) || observed.assertions.length !== names.length || !equal(observed.assertions.map(check => check.name).sort(), [...names].sort())
    || observed.assertions.some(check => typeof check.passed !== 'boolean') || (observed.assertions.every(check => check.passed) ? 'passed' : 'failed') !== observed.status) throw Error('UI required oracle coverage differs');
  const relays = native.events.filter(event => event.type === 'owned_server_relay' && event.resource === 'ui-artifact');
  if (relays.length !== 1 || relays[0].sha256 !== sha(html) || relays[0].bytes !== html.length) throw Error('Exact UI server relay absent');
  return { status: observed.status, assertions: observed.assertions, artifact_sha256: receipt.artifact_sha256, visual_review: 'not_run' };
}
function project(caseId, file, runId, nativeFile, buildFile, buildHash) {
  if (!/^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/.test(runId || '')) throw Error('Bounded UI run identity required');
  for (const input of [file, nativeFile, buildFile]) if (!path.isAbsolute(input || '')) throw Error('Absolute UI input paths required');
  const filesRaw = read(file, 4 * 1024 * 1024), nativeRaw = read(nativeFile), native = JSON.parse(nativeRaw);
  const observations = native.events?.filter(event => event.type === 'ui_artifact_observed');
  if (!observations || observations.length !== 1) throw Error('One actual UI observation required before projection');
  const observed = observations[0];
  const receipt = { schema: 'cs3-ui-artifact-browser/1', run_id: runId, case_id: caseId, artifact_sha256: sha(filesRaw),
    status: observed.status, assertions: observed.assertions, cleanup: 'completed', containment: 'qualified', visual_review: 'not_run',
    native_receipt: { path: plain(nativeFile), sha256: sha(nativeRaw) }, build: { path: plain(buildFile), sha256: buildHash } };
  validateUiArtifact(receipt, JSON.parse(filesRaw), caseId);
  return receipt;
}
module.exports = { cases, sourceNames, assertions, htmlBytes, renderResource, compile, validateSources, validateUiArtifact, project };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); const result = command === 'compile' && args.length === 2 ? compile(...args) : command === 'project' && args.length === 6 ? project(...args) : (() => { throw Error('Usage: compile CASE ARTIFACT_JSON | project CASE ARTIFACT_JSON RUN_ID NATIVE_RECEIPT BUILD_INPUTS BUILD_SHA256'); })(); process.stdout.write(JSON.stringify(result) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
