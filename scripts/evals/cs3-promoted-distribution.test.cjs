// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const helperFile = path.join(__dirname, 'cs3-promoted-distribution.cjs'), actual = createRequire(helperFile);
const assetTools = actual('../skills/builtin-assets.cjs'), repo = path.resolve(__dirname, '../..');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
function fixture(t) {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-promoted-contract-')), root = path.join(temp, 'new'), paidRoot = path.join(temp, 'paid');
  t.after(() => fs.rmSync(temp, { recursive: true, force: true }));
  const ref = file => ({ path: file, sha256: sha(fs.readFileSync(file)) });
  const put = (file, value) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, Buffer.isBuffer(value) || typeof value === 'string' ? value : JSON.stringify(value)); return ref(file); };
  const ids = actual('./cs3-promoted-distribution.cjs').ids, builtin = path.join(root, 'src/skills/builtin'), baselineRoot = path.join(paidRoot, 'build/skills/builtin');
  fs.cpSync(path.join(repo, 'src/skills/builtin'), baselineRoot, { recursive: true }); fs.cpSync(baselineRoot, builtin, { recursive: true });
  const baselineCatalog = fs.readFileSync(path.join(baselineRoot, 'catalog.json')), baseline = assetTools.inspectAssets(baselineRoot, baselineCatalog).inventory;
  const harness = 'src/crates/vcp-cli/tests/executable.rs', moduleFile = 'src/crates/vcp-cli/tests/support/promoted_skills.rs';
  put(path.join(paidRoot, harness), '// original exact harness\n');
  put(path.join(root, harness), '#[path = "support/promoted_skills.rs"]\nmod promoted_skills;\n// original exact harness\n');
  put(path.join(root, moduleFile), '// synthetic qualification-only tests\n');
  const file = (base, relative) => ({ path: relative, bytes: fs.statSync(path.join(base, relative)).size, sha256: sha(fs.readFileSync(path.join(base, relative))) });
  const priorBuild = put(path.join(paidRoot, 'build/receipt.json'), { source_inputs: { files: [file(paidRoot, harness)] } });
  const candidateInventories = ids.map(id => {
    const source = path.join(repo, 'src/evals/skills', id === 'skill-authoring' ? 'cs3-skill-remediation' : id === 'document-authoring' ? 'cs3-document-remediation' : 'cs3-comparison', 'candidates', id);
    fs.cpSync(source, path.join(builtin, id), { recursive: true });
    const raw = fs.readFileSync(path.join(source, 'skill.json')), descriptor = JSON.parse(raw), parts = [descriptor.body, ...descriptor.resources];
    return { source_id: 'synthetic-explicit-candidate', path: path.dirname(source), files: ['skill.json', ...parts.map(p => p.path)].map(relative => file(builtin, id + '/' + relative)),
      entries: [{ id, version: descriptor.version, qualified_id: 'synthetic::' + id, descriptor_sha256: sha(raw), parts }] };
  });
  const plans = ids.map((id, i) => put(path.join(paidRoot, id + '.json'), { spec: { build_receipt: priorBuild, executable: { path: path.join(paidRoot, 'build/vcp.exe'), sha256: '0'.repeat(64) } }, assets: baseline, candidate_assets: candidateInventories[i] }));
  const proof = put(path.join(temp, 'proof.json'), { schema: 'cs3-controller-recovery-fresh-historical-proof/1', dispositions: ids.map((skill, i) => ({ skill, plan: plans[i], candidate_assets: candidateInventories[i], disposition: put(path.join(paidRoot, skill + '-disposition.json'), { status: 'qualified', synthetic: true }) })) });
  const finalSpec = put(path.join(temp, 'recovery-spec.json'), { historical_proof: proof, decision: put(path.join(temp, 'decision.json'), { synthetic: true }) });
  const observed = { schema: 'cs3-controller-recovery-qualification/1', status: 'passed', six_skills_qualified: true, model_calls: 0, historical_results_modified: false };
  const finalResult = put(path.join(temp, 'recovery-result.json'), observed), catalog = JSON.parse(baselineCatalog), coverage = JSON.parse(fs.readFileSync(path.join(builtin, 'coverage.json')));
  catalog.version = '9.0.0';
  for (const [i, id] of ids.entries()) {
    const descriptor = JSON.parse(fs.readFileSync(path.join(builtin, id, 'skill.json'))), candidate = candidateInventories[i].entries[0];
    catalog.skills.push({ id, version: descriptor.version, descriptor: id + '/skill.json', descriptor_sha256: candidate.descriptor_sha256, body: descriptor.body, resources: descriptor.resources, source: descriptor.source, license: descriptor.license });
    coverage.families.push({ id, version: descriptor.version, qualification: { schema: 'cs3-promoted-skill-coverage/1', recovery_result_sha256: finalResult.sha256,
      comparison_disposition_sha256: ref(path.join(paidRoot, id + '-disposition.json')).sha256, candidate_inventory_sha256: sha(JSON.stringify(candidateInventories[i])), scope: 'frozen_cs3_six_case_comparison_only', visual_review: 'not_run' } });
  }
  const coverageRef = put(path.join(builtin, 'coverage.json'), coverage); catalog.coverage.sha256 = coverageRef.sha256;
  put(path.join(builtin, 'catalog.json'), catalog);
  const executable = put(path.join(root, 'build/vcp.exe'), Buffer.concat([Buffer.from('synthetic never executed\n'), fs.readFileSync(path.join(builtin, 'catalog.json'))]));
  fs.cpSync(builtin, path.join(root, 'build/skills/builtin'), { recursive: true });
  const buildValue = { status: 'passed', source_inputs: { files: [file(root, harness), file(root, moduleFile)] } }, buildReceipt = put(path.join(root, 'build/receipt.json'), buildValue);
  const spec = put(path.join(temp, 'promotion-spec.json'), { recovery_spec: finalSpec, recovery_result: finalResult, executable, build_receipt: buildReceipt });
  let calls = 0, failRecovery = false;
  const module = { exports: {} };
  // Explicit synthetic dependency substitutions: these tests prove the new join,
  // not native package behavior or six-skill qualification. Production exposes no
  // callback override and invokes the real existing recovery/build validators.
  const local = name => name === './cs3-controller-recovery-qualification.cjs' ? {
    validate(value) { calls++; assert.deepEqual(value, JSON.parse(fs.readFileSync(finalSpec.path))); assert(!failRecovery, 'real prerequisite failed'); return structuredClone(observed); },
    decision() { return { skill_remediation_lineage: { root: paidRoot } }; },
  } : name === './cs3-document-remediation.cjs' ? { build(value, approved) { assert.deepEqual(value.executable, executable); assert.equal(approved.executable_sha256, executable.sha256); assert.deepEqual(JSON.parse(fs.readFileSync(buildReceipt.path)), buildValue); return structuredClone(buildValue); } }
    : name === '../skills/builtin-assets.cjs' ? { ...assetTools, inspectAssets(directory, expected = fs.readFileSync(path.join(builtin, 'catalog.json'))) { return assetTools.inspectAssets(directory, expected); } } : actual(name);
  new Function('require', 'module', 'exports', '__dirname', fs.readFileSync(helperFile, 'utf8'))(local, module, module.exports, path.join(root, 'scripts/evals'));
  const helper = module.exports;
  function receipts() {
    const admission = helper.admit(spec), installationRoot = path.join(temp, 'installation'), installedRoot = path.join(installationRoot, 'release');
    fs.cpSync(path.join(root, 'build'), installedRoot, { recursive: true });
    const archive = put(path.join(temp, 'package/archive.zip'), 'synthetic unsigned archive'), packageResult = put(path.join(temp, 'package/result.json'), { schema: 'vcp-distribution-result/1', status: 'candidate', package: 'archive.zip', archive_sha256: archive.sha256, manifest: { skills: { catalog_sha256: admission.catalog.sha256 } } });
    const stage = (action, fresh) => ({ action, skills: fresh ? 27 : 21, protected_data_unchanged: true, release: fresh ? installedRoot : 'old-release', executable_sha256: fresh ? executable.sha256 : '0'.repeat(64), catalog_sha256: fresh ? admission.catalog.sha256 : admission.baseline_catalog.sha256 });
    const installRunner = 'scripts/evals/authoring-package-qualification.ps1'; put(path.join(root, installRunner), fs.readFileSync(path.join(repo, installRunner)));
    const previousArchive = put(path.join(temp, 'old-archive.zip'), 'retained synthetic rollback archive');
    const installationValue = { schema: 'cs3-promoted-package-installation/1', status: 'pass', inputs_unchanged: true, runner_sha256: sha(fs.readFileSync(path.join(root, installRunner))), promotion_spec: spec, promotion_admission: admission, executable_sha256: executable.sha256,
      archive_sha256: archive.sha256, previous_archive: previousArchive.path, previous_archive_sha256: previousArchive.sha256, package_result: packageResult.path, installed_candidate: installedRoot, directory: installationRoot,
      stages: [stage('Install', false), stage('Upgrade', true), stage('Rollback', false), stage('Upgrade', true)] };
    installationValue.stages.forEach((stage, index) => { stage.exit_code = 0; stage.help_exit_code = 0;
      stage.log_sha256 = put(path.join(installationRoot, stage.action + '-' + index + '.log'), 'synthetic successful installer').sha256;
      stage.help_log_sha256 = put(path.join(installationRoot, 'help-' + index + '.log'), 'synthetic startup help').sha256;
    });
    const installation = put(path.join(temp, 'installation.json'), installationValue), testBase = path.join(temp, 'native-tests');
    const runner = 'scripts/evals/cs3-promoted-installed-skills-qualification.ps1'; put(path.join(root, runner), fs.readFileSync(path.join(repo, runner)));
    const stages = helper.filters.map(filter => { const log = put(path.join(testBase, filter.replaceAll('::', '-') + '.log'), `test ${filter} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n`); return { filter, exit_code: 0, log_sha256: log.sha256 }; });
    const installedValue = { schema: 'cs3-promoted-installed-skills/1', status: 'pass', inputs_unchanged: true, paid_requests: 0, installation_report_sha256: installation.sha256,
      executable_sha256: executable.sha256, catalog_sha256: admission.catalog.sha256, archive_sha256: archive.sha256, admission: put(path.join(testBase, 'admission.json'), admission),
      stages, runner_sha256: sha(fs.readFileSync(path.join(root, runner))), test_sources: [harness, moduleFile].map(relative => ({ path: relative, sha256: sha(fs.readFileSync(path.join(root, relative))) })) };
    return { input: { promotion_spec: spec, installation, installed: put(path.join(testBase, 'result.json'), installedValue) }, installedValue, installationValue, testBase };
  }
  return { root, paidRoot, builtin, put, ref, spec, helper, receipts, catalog, coverage, executable, buildValue, priorBuild,
    fail() { failRecovery = true; }, calls: () => calls };
}

test('exact qualified bytes, preserved baseline and current genuine build are mandatory before any package acceptance', t => {
  const f = fixture(t), value = f.helper.admit(f.spec);
  assert.equal(value.status, 'eligible_for_package_checks'); assert.equal(value.assets.skills, 27); assert.equal(value.assets.files.length, 57);
  assert.equal(value.skills.length, 6); assert.equal(f.calls(), 2);
  f.fail(); assert.throws(() => f.helper.admit(f.spec), /prerequisite failed/);
});
test('candidate, baseline and coverage mutations are rejected rather than relabelled qualified', t => {
  for (const kind of ['candidate', 'baseline', 'coverage']) {
    const f = fixture(t);
    if (kind === 'candidate') fs.appendFileSync(path.join(f.builtin, 'skill-authoring/SKILL.md'), '\nchanged');
    if (kind === 'baseline') fs.appendFileSync(path.join(f.builtin, 'testing/SKILL.md'), '\nchanged');
    if (kind === 'coverage') { f.coverage.families.at(-1).qualification.visual_review = 'passed'; const ref = f.put(path.join(f.builtin, 'coverage.json'), f.coverage); f.catalog.coverage.sha256 = ref.sha256; f.put(path.join(f.builtin, 'catalog.json'), f.catalog); }
    assert.throws(() => f.helper.admit(f.spec));
  }
});
test('runtime drift allowlist permits only the new qualification module and exact import', t => {
  const f = fixture(t), prior = JSON.parse(fs.readFileSync(f.priorBuild.path)).source_inputs;
  assert.doesNotThrow(() => f.helper.runtimeInputs(f.buildValue.source_inputs, prior, f.paidRoot));
  assert.throws(() => f.helper.runtimeInputs({ files: [...f.buildValue.source_inputs.files, { path: 'src/crates/vcp-cli/src/main.rs', sha256: 'a'.repeat(64), bytes: 1 }] }, prior, f.paidRoot), /product/);
  fs.appendFileSync(path.join(f.root, 'src/crates/vcp-cli/tests/executable.rs'), 'unrelated test change\n');
  assert.throws(() => f.helper.runtimeInputs(f.buildValue.source_inputs, prior, f.paidRoot), /test source changed/);
});
test('a paid source that already includes promotion tests requires exact native closure with no special allowance', t => {
  const f = fixture(t), prior = structuredClone(f.buildValue.source_inputs);
  assert.doesNotThrow(() => f.helper.runtimeInputs(f.buildValue.source_inputs, prior, f.paidRoot));
  for (const name of ['src/crates/vcp-cli/tests/executable.rs', 'src/crates/vcp-cli/tests/support/promoted_skills.rs']) {
    const changed = structuredClone(prior); changed.files.find(file => file.path === name).sha256 = 'b'.repeat(64);
    assert.throws(() => f.helper.runtimeInputs(changed, prior, f.paidRoot), /qualification-test source/);
  }
});
test('complete synthetic promoted installation and three actually executed exact filters join without external candidate acceptance', t => {
  const f = fixture(t), evidence = f.receipts(), result = f.helper.validate(evidence.input);
  assert.equal(result.status, 'passed'); assert.equal(result.production_release, false); assert.equal(result.model_calls, 0);
});
test('old candidate receipts, wrong rollback, skipped tests and changed raw logs cannot satisfy promotion', t => {
  for (const kind of ['candidate-receipt', 'rollback', 'skipped', 'log', 'admission']) {
    const f = fixture(t), e = f.receipts();
    if (kind === 'candidate-receipt') e.installationValue.schema = 'cs-authoring-package-qualification/1';
    if (kind === 'rollback') e.installationValue.stages[2].executable_sha256 = 'a'.repeat(64);
    if (['candidate-receipt', 'rollback'].includes(kind)) { e.input.installation = f.put(e.input.installation.path, e.installationValue); e.installedValue.installation_report_sha256 = e.input.installation.sha256; }
    if (kind === 'admission') { const changed = JSON.parse(fs.readFileSync(e.installedValue.admission.path)); changed.skills[0].version = '0.0.0'; e.installedValue.admission = f.put(e.installedValue.admission.path, changed); }
    if (['skipped', 'log'].includes(kind)) { const row = e.installedValue.stages[0], target = path.join(e.testBase, row.filter.replaceAll('::', '-') + '.log'); const changed = f.put(target, 'test result: ok. 0 passed; 0 failed; 1 ignored;\n'); if (kind === 'skipped') row.log_sha256 = changed.sha256; }
    e.input.installed = f.put(e.input.installed.path, e.installedValue);
    assert.throws(() => f.helper.validate(e.input), undefined, kind);
  }
});
test('PowerShell promotion entrypoints retain candidate default and use three fail-closed explicit ignored tests', () => {
  const installer = fs.readFileSync(path.join(__dirname, 'authoring-package-qualification.ps1'), 'utf8'), runner = fs.readFileSync(path.join(__dirname, 'cs3-promoted-installed-skills-qualification.ps1'), 'utf8');
  assert(installer.includes("elseif ($Action -eq 'Upgrade')")); assert(installer.includes('Research candidate leaked into installed builtin skills'));
  assert(installer.indexOf('admit $PromotionSpec') < installer.indexOf("$temporary ="));
  for (const filter of actual('./cs3-promoted-distribution.cjs').filters) assert(runner.includes(filter));
  assert(runner.includes('-- --ignored --exact --nocapture')); assert(runner.includes('1 passed; 0 failed; 0 ignored;'));
  assert(runner.includes('[Environment]::SetEnvironmentVariable($name, $previous[$name])'));
});
