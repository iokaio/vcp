// SPDX-License-Identifier: Apache-2.0
'use strict';
// Unpaid downstream distribution gate. This never promotes files and never
// substitutes package checks for the authenticated six-skill comparison gate.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const recovery = require('./cs3-controller-recovery-qualification.cjs');
const build = require('./cs3-document-remediation.cjs').build;
const assets = require('../skills/builtin-assets.cjs');
const { plain, read } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../..'), builtin = path.join(root, 'src/skills/builtin');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const ids = ['document-authoring', 'skill-authoring', 'frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
const versions = ['1.0.5', '1.0.3', '1.0.0', '1.0.1', '1.0.0', '1.0.1'];
const testModule = 'src/crates/vcp-cli/tests/support/promoted_skills.rs';
const testRoot = 'src/crates/vcp-cli/tests/executable.rs';
const declaration = '#[path = "support/promoted_skills.rs"]\nmod promoted_skills;\n';
const filters = [
  'promoted_skills::executable_promoted_six_builtin_offline_discovery_and_integrity',
  'promoted_skills::executable_promoted_six_builtin_report_only_profiles_complete_without_workspace_edits',
  'promoted_skills::executable_promoted_six_builtin_terminal_controls_preserve_precedence_and_revocation',
];
function need(value, message) { if (!value) throw Error(message); }
function bytes(file, limit = 64 * 1024 * 1024) {
  file = plain(file); need(fs.lstatSync(file).nlink === 1, 'Hardlinked promotion evidence rejected'); return read(file, limit);
}
function reference(file) { file = plain(path.resolve(file)); return { path: file, sha256: sha(bytes(file)) }; }
function bound(ref, limit) {
  need(ref && equal(Object.keys(ref).sort(), ['path', 'sha256']) && path.isAbsolute(ref.path || '') && /^[a-f0-9]{64}$/.test(ref.sha256 || ''), 'Absolute hash-bound promotion evidence required');
  const value = bytes(ref.path, limit); need(sha(value) === ref.sha256, 'Promotion evidence changed'); return value;
}
const json = ref => JSON.parse(bound(ref));
function exactKeys(value, keys) { need(value && equal(Object.keys(value).sort(), keys.sort()), 'Unexpected promotion fields'); }
function runtimeInputs(current, original, oldRoot) {
  const native = file => file.path.startsWith('src/crates/') || file.path.startsWith('src/third_party/') || file.path.startsWith('scripts/upstream/');
  need(current.files.filter(file => file.path === testModule).length === 1, 'Exact promoted qualification module required');
  if (original.files.some(file => file.path === testModule)) {
    need(equal(current.files.filter(native), original.files.filter(native)), 'Promotion changed qualified product or qualification-test source');
    return;
  }
  const unchanged = input => input.files.filter(file => native(file) && ![testModule, testRoot].includes(file.path));
  need(equal(unchanged(current), unchanged(original)), 'Promotion changed qualified product or upstream source');
  const oldTest = original.files.find(file => file.path === testRoot), newTest = current.files.find(file => file.path === testRoot);
  need(oldTest && newTest, 'Qualification test source identities missing');
  const prior = bytes(path.join(oldRoot, testRoot)), now = bytes(path.join(root, testRoot));
  need(sha(prior) === oldTest.sha256 && sha(now) === newTest.sha256, 'Qualification test source changed');
  const normalized = now.toString('utf8').replaceAll('\r\n', '\n'), baseline = prior.toString('utf8').replaceAll('\r\n', '\n');
  need(normalized.split(declaration).length === 2 && normalized.replace(declaration, '') === baseline, 'Only the explicit promoted test module declaration may change the old test harness');
}
function admit(specRef) {
  const spec = json(specRef); exactKeys(spec, ['recovery_spec', 'recovery_result', 'build_receipt', 'executable']);
  const finalSpec = json(spec.recovery_spec), retained = json(spec.recovery_result);
  // Re-execute the real source-authenticated reviewer/native join. A passed JSON
  // summary, callback supplied by a caller, or incomplete candidate is not enough.
  const observed = recovery.validate(finalSpec);
  need(equal(observed, retained) && retained.schema === 'cs3-controller-recovery-qualification/1' && retained.status === 'passed'
    && retained.six_skills_qualified === true && retained.model_calls === 0 && retained.historical_results_modified === false, 'Genuine complete recovery qualification required before promotion');
  const proof = json(finalSpec.historical_proof);
  need(proof.schema === 'cs3-controller-recovery-fresh-historical-proof/1' && proof.dispositions?.length === 6
    && equal(proof.dispositions.map(row => row.skill).sort(), [...ids].sort()), 'Exact final six-skill Friendli qualification required');
  const decisions = proof.dispositions.map(row => ({ ...row, planValue: json(row.plan) }));
  const first = decisions[0].planValue, priorBuildRef = first.spec.build_receipt, priorBuild = json(priorBuildRef);
  need(decisions.every(row => equal(row.planValue.spec.build_receipt, priorBuildRef) && equal(row.planValue.assets, first.assets)), 'All six qualified plans must share the same runtime and baseline catalog');
  const baselineRoot = path.join(path.dirname(first.spec.executable.path), 'skills/builtin');
  const baselineCatalogRef = reference(path.join(baselineRoot, 'catalog.json')), baselineCatalog = json(baselineCatalogRef);
  const baseline = assets.inspectAssets(baselineRoot, bound(baselineCatalogRef)).inventory;
  need(equal(baseline, first.assets) && baseline.skills === 21 && baseline.files.length === 44
    && !baselineCatalog.skills.some(item => ids.includes(item.id)), 'Exact original twenty-one builtin baseline required');
  const catalogRef = reference(path.join(builtin, 'catalog.json')), catalog = json(catalogRef), current = assets.inspectAssets(builtin).inventory;
  need(current.skills === 27 && current.files.length === 57 && /^\d+\.\d+\.\d+$/.test(catalog.version)
    && catalog.version.localeCompare(baselineCatalog.version, 'en', { numeric: true }) > 0, 'Versioned exact twenty-seven skill distribution required');
  need(equal(catalog.skills.filter(item => !ids.includes(item.id)), baselineCatalog.skills), 'Existing builtin catalog entries changed');
  for (const item of baseline.files.filter(file => !['catalog.json', 'coverage.json'].includes(file.path)))
    need(equal(current.files.find(file => file.path === item.path), item), 'Existing builtin bytes changed');
  const oldCoverage = JSON.parse(bytes(path.join(baselineRoot, 'coverage.json'))), coverage = JSON.parse(bytes(path.join(builtin, 'coverage.json')));
  need(coverage.schema === oldCoverage.schema && Array.isArray(coverage.families)
    && equal(coverage.families.filter(row => !ids.includes(row.id)), oldCoverage.families)
    && equal(Object.keys(coverage).sort(), Object.keys(oldCoverage).sort())
    && Object.keys(oldCoverage).filter(key => key !== 'families').every(key => equal(coverage[key], oldCoverage[key])), 'Baseline coverage changed');
  const skills = ids.map((id, index) => {
    const row = decisions.find(row => row.skill === id), inventory = row.candidate_assets, candidate = inventory.entries.filter(item => item.id === id);
    need(candidate.length === 1 && candidate[0].version === versions[index] && equal(row.planValue.candidate_assets, inventory), 'Qualified candidate inventory differs');
    const selected = candidate[0], expected = inventory.files.filter(item => item.path.startsWith(id + '/'));
    const order = rows => [...rows].sort((a, b) => a.path.localeCompare(b.path));
    need(expected.length === 1 + selected.parts.length && equal(order(current.files.filter(item => item.path.startsWith(id + '/'))), order(expected)), 'Promoted files differ from exact qualified candidate bytes');
    const descriptor = JSON.parse(bytes(path.join(builtin, id, 'skill.json'))), entry = catalog.skills.filter(item => item.id === id);
    need(entry.length === 1 && entry[0].descriptor_sha256 === selected.descriptor_sha256 && descriptor.version === versions[index]
      && equal([descriptor.body, ...descriptor.resources], selected.parts), 'Promoted descriptor differs from qualified package');
    const covered = coverage.families.filter(item => item.id === id);
    need(covered.length === 1 && equal(Object.keys(covered[0]).sort(), ['id', 'qualification', 'version']) && covered[0].version === versions[index] && equal(covered[0].qualification, {
      schema: 'cs3-promoted-skill-coverage/1', recovery_result_sha256: spec.recovery_result.sha256,
      comparison_disposition_sha256: row.disposition.sha256, candidate_inventory_sha256: sha(JSON.stringify(inventory)),
      scope: 'frozen_cs3_six_case_comparison_only', visual_review: 'not_run',
    }), 'Promotion coverage lacks exact bounded qualification provenance');
    return { id, version: versions[index], qualified_id: `vcp-builtin::${id}::${id}`, descriptor_sha256: selected.descriptor_sha256,
      body: descriptor.body, resources: descriptor.resources };
  });
  const receipt = build(spec, { executable_sha256: spec.executable.sha256 });
  const approved = recovery.decision(finalSpec.decision), paidRoot = approved.skill_remediation_lineage.root;
  runtimeInputs(receipt.source_inputs, priorBuild.source_inputs, paidRoot);
  const executable = bound(spec.executable, 1024 * 1024 * 1024);
  need(executable.includes(bound(catalogRef)) && equal(assets.inspectAssets(path.join(path.dirname(spec.executable.path), 'skills/builtin')).inventory, current), 'New executable must embed and stage the exact promoted catalog');
  need(equal(recovery.validate(json(spec.recovery_spec)), retained) && equal(json(specRef), spec)
    && equal(assets.inspectAssets(builtin).inventory, current) && equal(build(spec, { executable_sha256: spec.executable.sha256 }), receipt), 'Promotion inputs changed during admission');
  return { schema: 'cs3-promoted-distribution-admission/1', status: 'eligible_for_package_checks', promotion_spec: specRef,
    recovery_spec: spec.recovery_spec, recovery_result: spec.recovery_result, historical_proof: finalSpec.historical_proof,
    executable: spec.executable, build_receipt: spec.build_receipt, catalog: catalogRef, baseline_catalog: baselineCatalogRef,
    skills, assets: current, model_calls: 0, qualification_waiver: false };
}
function validate(input) {
  exactKeys(input, ['promotion_spec', 'installation', 'installed']);
  const approved = admit(input.promotion_spec), installation = json(input.installation), installed = json(input.installed);
  need(installation.schema === 'cs3-promoted-package-installation/1' && installation.status === 'pass' && installation.inputs_unchanged === true
    && installation.runner_sha256 === sha(bytes(path.join(__dirname, 'authoring-package-qualification.ps1')))
    && equal(installation.promotion_spec, input.promotion_spec) && equal(installation.promotion_admission, approved)
    && installation.executable_sha256 === approved.executable.sha256 && installation.stages?.length === 4, 'Exact promoted installation evidence required');
  const stages = installation.stages;
  need(equal(stages.map(row => row.action), ['Install', 'Upgrade', 'Rollback', 'Upgrade']) && stages.every(row => row.protected_data_unchanged === true)
    && stages[0].skills === 21 && stages[2].skills === 21 && stages[1].skills === 27 && stages[3].skills === 27
    && stages[0].catalog_sha256 === approved.baseline_catalog.sha256
    && ['release', 'executable_sha256', 'catalog_sha256'].every(key => stages[0][key] === stages[2][key] && stages[1][key] === stages[3][key])
    && stages[1].executable_sha256 === approved.executable.sha256 && stages[1].catalog_sha256 === approved.catalog.sha256
    && installation.previous_archive_sha256 !== installation.archive_sha256, 'Exact promoted upgrade/rollback identities required');
  need(path.isAbsolute(installation.previous_archive || '') && path.isAbsolute(installation.package_result || '')
    && path.isAbsolute(installation.directory || '') && path.isAbsolute(installation.installed_candidate || '')
    && installation.installed_candidate === stages[3].release
    && sha(bytes(installation.previous_archive, 1024 * 1024 * 1024)) === installation.previous_archive_sha256, 'Retained rollback archive or installed location changed');
  for (const [index, stage] of stages.entries()) {
    need(stage.exit_code === 0 && stage.help_exit_code === 0 && stage.log_sha256 === sha(bytes(path.join(installation.directory, stage.action + '-' + index + '.log')))
      && stage.help_log_sha256 === sha(bytes(path.join(installation.directory, 'help-' + index + '.log'))), 'Raw installation/startup evidence changed');
  }
  const packageRef = reference(installation.package_result), packaged = json(packageRef), packageRoot = path.dirname(packageRef.path);
  need(packaged.schema === 'vcp-distribution-result/1' && packaged.status === 'candidate'
    && packaged.archive_sha256 === installation.archive_sha256 && packaged.manifest.skills.catalog_sha256 === approved.catalog.sha256,
  'Exact unsigned native distribution package required');
  assets.portable(packaged.package);
  need(sha(bytes(path.join(packageRoot, packaged.package), 1024 * 1024 * 1024)) === installation.archive_sha256, 'Promoted archive changed');
  need(installed.schema === 'cs3-promoted-installed-skills/1' && installed.status === 'pass' && installed.inputs_unchanged === true && installed.paid_requests === 0
    && installed.installation_report_sha256 === input.installation.sha256 && installed.executable_sha256 === approved.executable.sha256
    && installed.catalog_sha256 === approved.catalog.sha256 && installed.archive_sha256 === installation.archive_sha256
    && equal(json(installed.admission), approved) && equal(installed.stages?.map(row => row.filter), filters), 'Exact promoted builtin native tests required');
  need(installed.runner_sha256 === sha(bytes(path.join(__dirname, 'cs3-promoted-installed-skills-qualification.ps1')))
    && equal(installed.test_sources, [testRoot, testModule].map(file => ({ path: file, sha256: sha(bytes(path.join(root, file))) }))), 'Promoted native test or runner source changed');
  for (const stage of installed.stages) {
    need(stage.exit_code === 0, 'Promoted native test failed');
    const log = bytes(path.join(path.dirname(input.installed.path), stage.filter.replaceAll('::', '-') + '.log'));
    need(sha(log) === stage.log_sha256 && /test result: ok\. 1 passed; 0 failed; 0 ignored;/.test(log.toString('utf8'))
      && log.toString('utf8').includes('test ' + stage.filter + ' ... ok'), 'Exact ignored test must actually execute and pass');
  }
  need(sha(bytes(path.join(installation.installed_candidate, 'vcp.exe'), 1024 * 1024 * 1024)) === approved.executable.sha256
    && equal(assets.inspectAssets(path.join(installation.installed_candidate, 'skills/builtin')).inventory, approved.assets), 'Installed promoted bytes changed');
  need(equal(admit(input.promotion_spec), approved) && equal(json(input.installation), installation) && equal(json(input.installed), installed), 'Distribution evidence changed during validation');
  return { schema: 'cs3-promoted-distribution/1', status: 'passed', ...input, admission: approved,
    package: packageRef, model_calls: 0, qualification_waiver: false, production_release: false };
}
module.exports = { admit, validate, reference, runtimeInputs, ids, versions, filters };
if (require.main === module) {
  try { const [command, file, ...rest] = process.argv.slice(2); need(!rest.length && file, 'Usage: admit SPEC | validate EVIDENCE');
    const result = command === 'admit' ? admit(reference(file)) : command === 'validate' ? validate(json(reference(file))) : (() => { throw Error('Unknown promotion operation'); })();
    process.stdout.write(JSON.stringify(result) + '\n');
  } catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
