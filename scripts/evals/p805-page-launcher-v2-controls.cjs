'use strict'; // SPDX-License-Identifier: Apache-2.0
// Offline controls for a future binding; never opens an owner campaign.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const repository = path.resolve(__dirname, '../..');
const fixture = path.join(repository, 'src/evals/release/p8-owner-v3');
const visible = ['--test', 'test/page.test.cjs'];
const canonical = ['--test', '--test-reporter=tap', '--test-concurrency=1', 'test/page.test.cjs'];
const digest = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
function inventory(root, relative = '') {
  return fs.readdirSync(path.join(root, relative), { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name)).flatMap(entry => {
    const name = path.posix.join(relative, entry.name);
    assert(!entry.isSymbolicLink(), `symlink: ${name}`);
    if (entry.isDirectory()) return inventory(root, name);
    assert(entry.isFile(), `non-file: ${name}`);
    return [{ path: name, bytes: fs.statSync(path.join(root, name)).size, sha256: digest(path.join(root, name)) }];
  });
}
function main(buildFile) {
  assert.equal(process.platform, 'win32', 'native Windows required');
  const build = JSON.parse(fs.readFileSync(buildFile, 'utf8'));
  assert.equal(build.schema, 'p805-page-launcher-build/2');
  assert.equal(build.exit_code, 0); assert.equal(build.parser_test_exit_code, 0); assert.equal(build.inputs_unchanged, true);
  assert.equal(build.node_sha256, '8490398f5e0082772dfb0ae5a6ebdff98a97696a20cb9778b4f82eec79b6d0a1');
  assert.equal(build.compiler_sha256, 'e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a');
  assert.equal(path.resolve(build.source), path.join(__dirname, 'p805-page-check-launcher-v2.rs'));
  assert.equal(path.resolve(build.builder), path.join(__dirname, 'p805-page-launcher-v2-build.ps1'));
  const identities = Object.fromEntries(['source', 'builder', 'node', 'compiler', 'launcher'].map(key => [build[key], build[`${key}_sha256`]]));
  identities[path.resolve(buildFile)] = digest(buildFile); identities[__filename] = digest(__filename);
  const audit = () => { for (const [file, hash] of Object.entries(identities)) assert.equal(digest(file), hash, `input changed: ${file}`); };
  audit();
  const manifestFile = path.join(fixture, 'manifest.json');
  assert.equal(digest(manifestFile), '6a27284e55f440ffbc8580562b415f8cab1157e54b569dde88fbe07ec5c8969b');
  const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
  const original = inventory(fixture);
  for (const entry of manifest.files) {
    assert(!path.isAbsolute(entry.path) && !entry.path.split('/').includes('..'));
    assert.equal(digest(path.join(fixture, entry.path)), entry.sha256, entry.path);
    assert.equal(fs.statSync(path.join(fixture, entry.path)).size, entry.bytes, entry.path);
  }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-p805-page-launcher-v2-'));
  const receipt = { schema: 'p805-page-launcher-controls/2', disposition: 'future-binding-proposal', paid_authorization: false, model_calls: 0, root, started_at: new Date().toISOString(), identities, fixture_manifest_sha256: digest(manifestFile), cases: [], pass: false };
  function run(name, executable, args, cwd) {
    audit();
    const result = spawnSync(executable, args, { cwd, env: { SystemRoot: process.env.SystemRoot, VCP_FAKE_PROVIDER_SECRET: 'must-not-be-inherited' }, encoding: 'utf8', timeout: 10000, maxBuffer: 262144, windowsHide: true });
    const stdout = path.join(root, `${name}.stdout.txt`), stderr = path.join(root, `${name}.stderr.txt`);
    fs.writeFileSync(stdout, result.stdout || ''); fs.writeFileSync(stderr, result.stderr || '');
    receipt.cases.push({ name, executable, args, cwd, status: result.status, signal: result.signal, error: result.error?.message, stdout_sha256: digest(stdout), stderr_sha256: digest(stderr) });
    assert.ifError(result.error); assert.equal(result.signal, null); audit(); return result;
  }
  try {
    const workspace = path.join(root, 'reference-workspace');
    fs.cpSync(path.join(fixture, 'u03/workspace'), workspace, { recursive: true, errorOnExist: true });
    for (const file of ['src/domain/window.cjs', 'src/api/page.cjs']) fs.copyFileSync(path.join(fixture, 'u03/hidden/reference', file), path.join(workspace, file));
    const referenceBefore = inventory(workspace), summaries = [];
    for (const [name, args] of [['visible', visible], ['canonical', canonical]]) {
      const result = run(`reference-${name}`, build.launcher, args, workspace);
      assert.equal(result.status, 0, result.stderr); assert.match(result.stdout, /# pass 3\b/); assert.match(result.stdout, /# fail 0\b/);
      summaries.push(result.stdout.split(/\r?\n/).filter(line => /^(?:ok |# (?:tests|pass|fail) )/.test(line)));
    }
    assert.deepEqual(summaries[0], summaries[1], 'both forms must run the same tests');
    const oracle = path.join(fixture, 'u03/hidden/oracle.cjs');
    const grade = run('reference-oracle', build.node, ['--permission', `--allow-fs-read=${workspace}`, `--allow-fs-read=${path.dirname(oracle)}`, oracle, workspace], workspace);
    assert.equal(grade.status, 0, grade.stderr); const result = JSON.parse(grade.stdout); assert.equal(result.pass, true); assert.equal(result.passed, 37); assert.equal(result.total, 37);
    const rejected = [[], ['--version'], ['--eval', 'process.exit(0)'], ['--test', '../test/page.test.cjs'], ['--test', 'test\\page.test.cjs'], ['--test', path.join(workspace, 'test/page.test.cjs')], ['--test', 'test/other.cjs'], ['--test', 'test/page.test.cjs; calc.exe'], ['--test', '--test-concurrency=1', '--test-reporter=tap', 'test/page.test.cjs'], ['--test', '--test-reporter=tap', '--test-concurrency=2', 'test/page.test.cjs']];
    for (const form of [visible, canonical]) for (const extra of ['--allow-net', '--allow-child-process', '--allow-fs-write=*', '--import=evil.cjs']) rejected.push([...form, extra]);
    for (const [i, args] of rejected.entries()) {
      const denied = run(`hostile-${i + 1}`, build.launcher, args, workspace);
      assert.equal(denied.status, 1); assert.equal(denied.stdout, ''); assert.match(denied.stderr, /only the two declared U03 test commands are permitted/);
    }
    assert.deepEqual(inventory(workspace), referenceBefore, 'reference workspace changed');
    const probe = path.join(root, 'permission-workspace'); fs.mkdirSync(path.join(probe, 'test'), { recursive: true });
    const outside = path.join(root, 'outside.txt'), unexpected = path.join(probe, 'unexpected.txt'); fs.writeFileSync(outside, 'not-readable');
    fs.writeFileSync(path.join(probe, 'test/page.test.cjs'), `
      const test=require('node:test'), assert=require('node:assert/strict');
      test('permission and environment fences', async()=>{
        // The pinned test runner itself adds NODE_TEST_WORKER_ID after launch.
        assert.deepEqual(Object.keys(process.env).map(k=>k.toUpperCase()).sort(),['NODE_TEST_WORKER_ID','SYSTEMROOT']);
        for(const scope of ['net','fs.write','child','worker','addons'])assert.equal(process.permission.has(scope),false,scope);
        assert.throws(()=>require('node:fs').readFileSync(${JSON.stringify(outside)}),{code:'ERR_ACCESS_DENIED'});
        assert.throws(()=>require('node:fs').writeFileSync(${JSON.stringify(unexpected)},'bad'),{code:'ERR_ACCESS_DENIED'});
        assert.throws(()=>require('node:child_process').spawnSync(process.execPath,['--version']),{code:'ERR_ACCESS_DENIED'});
        assert.throws(()=>new(require('node:worker_threads').Worker)('0',{eval:true}),{code:'ERR_ACCESS_DENIED'});
        await new Promise((resolve,reject)=>{const socket=require('node:net').connect({host:'127.0.0.1',port:9});socket.once('connect',()=>{socket.destroy();reject(Error('unexpected connection'));});socket.once('error',error=>{try{assert.equal(error.code,'ERR_ACCESS_DENIED');resolve();}catch(e){reject(e);}});});
      });
    `);
    const probeBefore = inventory(probe);
    for (const [name, args] of [['visible', visible], ['canonical', canonical]]) {
      const checked = run(`permissions-${name}`, build.launcher, args, probe);
      assert.equal(checked.status, 0, checked.stdout + checked.stderr); assert.match(checked.stdout, /# pass 1\b/);
    }
    assert.deepEqual(inventory(probe), probeBefore); assert.equal(fs.existsSync(unexpected), false); assert.equal(fs.readFileSync(outside, 'utf8'), 'not-readable');
    receipt.pass = true;
  } catch (error) { receipt.failure = error.stack; }
  finally {
    try { audit(); assert.deepEqual(inventory(fixture), original, 'frozen fixture changed'); receipt.inputs_unchanged = true; }
    catch (error) { receipt.pass = false; receipt.integrity_failure = error.stack; }
    receipt.completed_at = new Date().toISOString();
    fs.writeFileSync(path.join(root, 'controls-receipt.json'), JSON.stringify(receipt, null, 2) + '\n');
    fs.writeFileSync(path.join(root, 'preparation-proposal.json'), JSON.stringify({ schema: 'p805-page-launcher-preparation-proposal/2', disposition: 'draft-not-executable', runnable: false, spend_authorized: false, model_calls: 0, build_receipt: path.resolve(buildFile), build_receipt_sha256: digest(buildFile), controls_receipt: path.join(root, 'controls-receipt.json'), controls_receipt_sha256: digest(path.join(root, 'controls-receipt.json')), controls_pass: receipt.pass, launcher: build.launcher, launcher_sha256: build.launcher_sha256, accepted_arguments: [visible, canonical], normalized_arguments: canonical, preserved_fixture_manifest_sha256: receipt.fixture_manifest_sha256, proposed_limits: manifest.proposed_limits, prior_cohort: 'preserved-failed-no-reclassification', remaining: ['Separately identify and review a future runner/preparer binding to this launcher and receipts.', 'Preserve existing fixture, thresholds, request limit and original cohort evidence.', 'Obtain authorization for any future provider attempt; this proposal authorizes none.'] }, null, 2) + '\n');
  }
  console.log(path.join(root, 'controls-receipt.json'));
  if (!receipt.pass) { console.error(receipt.failure || receipt.integrity_failure); process.exitCode = 1; }
}
if (require.main === module) { assert.equal(process.argv.length, 3, 'usage: node p805-page-launcher-v2-controls.cjs <build-receipt.json>'); main(path.resolve(process.argv[2])); }
