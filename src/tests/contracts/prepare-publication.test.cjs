// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { execFileSync } = require('node:child_process');
const p = require('../../../scripts/release/provenance.cjs');
const { stages } = require('../../../scripts/release/evidence.cjs');
const { preparePublication } = require('../../../scripts/release/prepare-publication.cjs');
const privateText = 'PRIVATE-PACKET-CANARY C:\\private-build\\credentials-not-public';
const commit = 'a'.repeat(40), digest = p.hash('source');

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-publication-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const packet = path.join(root, 'packet'); fs.mkdirSync(packet);
  for (const name of ['artifacts', 'receipts', 'logs']) fs.mkdirSync(path.join(packet, name));
  const files = new Set();
  function write(name, value) {
    files.add(name); fs.mkdirSync(path.dirname(path.join(packet,name)),{recursive:true});
    fs.writeFileSync(path.join(packet, name), typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value, null, 2) + '\n');
  }
  function sums() {
    fs.writeFileSync(path.join(packet, 'SHA256SUMS'), [...files].sort().map(name => `${p.fileHash(path.join(packet, name))}  ${name}`).join('\n') + '\n');
  }
  const selected = { channel: 'internal-beta', native_version: '0.2.0-beta.1',
    sdk_version: '0.2.1', vsix_version: '0.2.1', target: 'x86_64-pc-windows-msvc', signing: { status: 'unsigned' },
    config_sha256: digest };
  const release = p.releaseIdentity(selected, { commit, content_sha256: digest }, digest);
  write('artifacts/native.zip', 'original ZIP'); write('artifacts/setup.exe', 'original installer'); write('artifacts/editor.vsix', 'original VSIX');
  const build = { schema: 'vcp-local-build/1', exit_code: 0, cargo_exit_code: 0, source_commit: commit,
    source_dirty: false, source_stable: true, source_content_sha256: digest, release,
    qualification_build: false, profile: 'release', target: selected.target, executable_sha256: digest, private: privateText };
  write('receipts/build.json', build); const buildHash = p.fileHash(path.join(packet, 'receipts/build.json'));
  const native = { schema: 'vcp-distribution-result/1', status: 'release-candidate', package: 'native.zip',
    archive_sha256: p.fileHash(path.join(packet, 'artifacts/native.zip')),
    manifest: { release, source: { git_commit: commit, dirty: false }, files: [{ path: 'vcp.exe', sha256: digest }],
      build: { status: 'verified-release-build', receipt_sha256: buildHash }, private: privateText } };
  write('receipts/native.json', native);
  const vsix = { schema: 'vcp-vsix-package/1', release,
    archive: { file: 'editor.vsix', sha256: p.fileHash(path.join(packet, 'artifacts/editor.vsix')) },
    extension: { id: 'iokaio.vcp', version: '0.2.1', source: { git_commit: commit, dirty: false } }, sdk: { version: '0.2.1' },
    engine: { native_archive_sha256: native.archive_sha256, executable_sha256: digest, build_receipt_sha256: buildHash,
      source_commit: commit, source_dirty: false, native_manifest_sha256: p.fileHash(path.join(packet, 'receipts/native.json')) } };
  const setup = { schema: 'vcp-setup-result/1', candidate_id: release.candidate_id, native_archive_sha256: native.archive_sha256,
    build_receipt_sha256: buildHash, archive: { file: 'setup.exe', sha256: p.fileHash(path.join(packet, 'artifacts/setup.exe')) } };
  write('receipts/vsix.json', vsix); write('receipts/setup.json', setup);
  const pair = p.pairIdentity(native, vsix, setup);
  pair.receipts = Object.fromEntries(['native', 'vsix', 'setup'].map(name => [name + '_sha256', p.fileHash(path.join(packet, `receipts/${name}.json`))]));
  write('pair.json', pair);
  const evidence = { schema: 'vcp-beta-evidence/1', status: 'qualification-required', selection_status: 'pass',
    pipeline_status: 'incomplete', stop_after: 'pair', reviewed_commit: commit, pair_id: pair.pair_id,
    validation_failures: [], environment: { secret: privateText }, observations: stages.map((id, i) => {
      if (i > stages.indexOf('pair')) return { id, status: 'not run' };
      write(`logs/${id}.log`, privateText);
      return { id, status: 'pass', exit_code: 0, log: `logs/${id}.log`, ended_at: '2026-10-01T13:50:23.7676196Z',
        ...(id === 'pair' ? { verified_pair_sha256: p.fileHash(path.join(packet, 'pair.json')) } : {}) };
    }) };
  write('evidence.json', evidence); sums();
  const options = { packet, output: path.join(root, 'publication'), runId: '36868151228', attempt: '2', expectedPair: pair.pair_id, expectedCommit: commit };
  function rebind() {
    write('receipts/build.json', build);
    native.manifest.build.receipt_sha256 = p.fileHash(path.join(packet, 'receipts/build.json'));
    setup.build_receipt_sha256 = vsix.engine.build_receipt_sha256 = native.manifest.build.receipt_sha256;
    write('receipts/native.json', native);
    vsix.engine.native_manifest_sha256 = p.fileHash(path.join(packet, 'receipts/native.json'));
    write('receipts/vsix.json', vsix); write('receipts/setup.json', setup);
    const next = p.pairIdentity(native, vsix, setup);
    next.receipts = Object.fromEntries(['native', 'vsix', 'setup'].map(name => [name + '_sha256', p.fileHash(path.join(packet, `receipts/${name}.json`))]));
    write('pair.json', next); evidence.pair_id = next.pair_id;
    evidence.observations[7].verified_pair_sha256 = p.fileHash(path.join(packet, 'pair.json'));
    write('evidence.json', evidence); sums(); return next;
  }
  return { root, packet, options, evidence, pair, native, setup, vsix, build, write, sums, rebind };
}
function absent(f) { assert.equal(fs.existsSync(f.options.output), false, 'Refusal must precede publication output creation'); }
function changeJson(f, name, value) { f.write(name, value); f.sums(); }

function signedFixture(t) {
  const f=fixture(t), policy={status:'signed',provider:'azure-artifact-signing',account:'ioka-llc-signing',profile:'WritingForgePro',
    endpoint:'https://wus2.codesigning.azure.net/',publisher:'CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US',
    identity_eku:'1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669'};
  const original=Buffer.alloc(512);original.writeUInt16LE(0x5a4d);original.writeUInt32LE(64,60);original.writeUInt32LE(0x4550,64);
  original.writeUInt16LE(240,84);original.writeUInt16LE(0x20b,88);original.writeUInt32LE(16,196);
  const signed=Buffer.concat([original,Buffer.alloc(16)]);signed.writeUInt32LE(512,232);signed.writeUInt32LE(16,236);
  signed.writeUInt32LE(12,512);signed.writeUInt16LE(0x200,516);signed.writeUInt16LE(2,518);signed.writeUInt32LE(1234,520);
  const inputHash=p.hash(original),outputHash=p.hash(signed);
  f.build.executable_sha256=f.build.launcher_sha256=inputHash;
  const release=p.releaseIdentity({...f.build.release,signing:policy},{commit,content_sha256:digest},inputHash);
  f.build.release=f.native.manifest.release=f.vsix.release=release;f.setup.candidate_id=release.candidate_id;
  f.write('receipts/build.json',f.build);const buildHash=p.fileHash(path.join(f.packet,'receipts/build.json'));
  f.native.manifest.files[0].sha256=f.vsix.engine.executable_sha256=outputHash;f.setup.launcher_sha256=outputHash;
  f.native.manifest.files.push({path:'vcp-launch.exe',sha256:outputHash});
  f.write('artifacts/setup.exe',signed);f.setup.archive.sha256=outputHash;
  for(const name of ['vcp.exe','vcp-launch.exe']) {f.write('build-output/'+name,original);f.write('signed/'+name,signed);}
  f.evidence.log_transformations=[];
  for(const [stage,roles] of [['native',['engine','launcher']],['setup',['setup','uninstaller']]]) {
    const receipt={schema:'vcp-authenticode-transform/1',stage,status:'verified',candidate_id:release.candidate_id,reviewed_commit:commit,
      build_receipt_sha256:buildHash,policy,tools:{signtool_sha256:digest,dlib_sha256:digest},files:roles.map(role=>({
        role,input_sha256:inputHash,input_bytes:original.length,output_sha256:outputHash,output_bytes:signed.length,content_preserved:true,
        signature:{status:'Valid',subject:policy.publisher,identity_eku:policy.identity_eku,certificate_sha256:digest,timestamp_certificate_sha256:digest,timestamp_present:true},
        verification:{exit_code:0,log_sha256:p.hash('synthetic signature check')}
      }))};
    f.write(`receipts/${stage}-signing.json`,receipt);
    const binding={status:'signed',receipt:'signing-receipt.json',receipt_sha256:p.fileHash(path.join(f.packet,`receipts/${stage}-signing.json`)),transformation:receipt};
    if(stage==='native') f.native.manifest.signing=binding;else f.setup.signing=binding;
    for(const role of roles) {
      const name=`logs/${stage}-${role}-signature.log`;f.write(name,'synthetic signature check');
      f.evidence.log_transformations.push({path:name,original_sha256:p.hash('synthetic signature check'),retained_sha256:p.hash('synthetic signature check'),sanitized:false});
      if(stage==='setup') {f.write(`signing/${role}/unsigned.exe`,original);f.write(`signing/${role}/signed.exe`,signed);}
    }
  }
  f.options.expectedPair=f.rebind().pair_id;return f;
}

test('signed publication binds native/setup transformations and rejects altered signing evidence',t=>{
  const f=signedFixture(t), result=preparePublication(f.options);
  assert.equal(result.qualification.signing,'signed');assert.match(fs.readFileSync(path.join(f.options.output,'notes.md'),'utf8'),/Signed Windows x64 beta/);
  for(const mutate of [g=>g.write('signed/vcp.exe',Buffer.alloc(528)),g=>g.write('signing/uninstaller/unsigned.exe',Buffer.alloc(512)),
    g=>g.write('receipts/native-signing.json',{}),g=>{g.evidence.log_transformations[0].original_sha256='0'.repeat(64);g.write('evidence.json',g.evidence)}]) {
    const changed=signedFixture(t);mutate(changed);changed.sums();assert.throws(()=>preparePublication(changed.options));absent(changed);
  }
});

test('publication copies only exact assets and deterministic sanitized metadata; CLI agrees', t => {
  const f = fixture(t), result = preparePublication(f.options), assets = path.join(f.options.output, 'assets');
  assert.equal(result.tag, `v0.2.0-beta.1-${f.pair.pair_id.slice(0, 12)}`);
  assert.equal(result.candidateAt, '2026-10-01T13:50:23.767Z');
  assert.equal(result.candidateRunId, '36868151228'); assert.equal(result.candidateAttempt, '2');
  assert.equal(result.runUrl, 'https://github.com/iokaio/vcp/actions/runs/36868151228/attempts/2');
  assert.equal(result.qualification.pipelineStatus, 'incomplete');
  assert.deepEqual(fs.readdirSync(f.options.output).sort(), ['assets', 'notes.md']);
  assert.deepEqual(fs.readdirSync(assets).sort(), ['SHA256SUMS', 'editor.vsix', 'native.zip', 'release.json', 'setup.exe']);
  assert.equal(JSON.parse(fs.readFileSync(path.join(assets, 'release.json'), 'utf8')).extensionId, 'iokaio.vcp');
  for (const artifact of result.artifacts) {
    assert.deepEqual(fs.readFileSync(path.join(assets, artifact.name)), fs.readFileSync(path.join(f.packet, 'artifacts', artifact.name)));
    assert.equal(artifact.sha256, p.fileHash(path.join(assets, artifact.name)));
    assert.equal(artifact.bytes, fs.statSync(path.join(assets, artifact.name)).size);
    assert.equal(artifact.href, `https://github.com/iokaio/vcp/releases/download/${result.tag}/${artifact.name}`);
  }
  const sums = fs.readFileSync(path.join(assets, 'SHA256SUMS'), 'utf8').trim().split('\n');
  assert.equal(sums.length, 4);
  for (const line of sums) { const [hash, name] = line.split('  '); assert.equal(hash, p.fileHash(path.join(assets, name))); }
  const publicText = fs.readFileSync(path.join(assets, 'release.json'), 'utf8') + fs.readFileSync(path.join(f.options.output, 'notes.md'), 'utf8');
  assert.doesNotMatch(publicText, /PRIVATE-PACKET-CANARY|private-build|credentials-not-public|"environment"|"command"|publishedAt/);
  assert.match(publicText, /Unsigned Windows x64 beta/); assert.match(publicText, /Full qualification remains incomplete/);
  const args = ['--packet', f.packet, '--output', path.join(f.root, 'retry'), '--run-id', '36868151228', '--attempt', '2',
    '--expected-pair', f.options.expectedPair, '--expected-commit', commit];
  const cli = JSON.parse(execFileSync(process.execPath, [path.resolve(__dirname, '../../../scripts/release/prepare-publication.cjs'), ...args],
    { encoding: 'utf8', windowsHide: true, timeout: 10000 }));
  assert.deepEqual(cli, result);
  for (const name of fs.readdirSync(assets)) assert.deepEqual(fs.readFileSync(path.join(assets, name)), fs.readFileSync(path.join(f.root, 'retry/assets', name)));
});

test('every checksum-bound input is verified, including private logs and archives', t => {
  for (const name of ['logs/pair.log', 'artifacts/setup.exe', 'artifacts/native.zip', 'artifacts/editor.vsix', 'receipts/native.json']) {
    const f = fixture(t); fs.appendFileSync(path.join(f.packet, name), 'changed');
    assert.throws(() => preparePublication(f.options), /checksum mismatch/); absent(f);
  }
});

test('checksum paths refuse escape, platform aliases, duplicates, missing required files and linked ancestors', t => {
  for (const name of ['../outside', '/absolute', 'logs\\pair.log', 'C:/absolute', 'logs/./pair.log', 'logs//pair.log',
    'logs/pair.log:stream', 'logs/pair.log ', 'logs/NUL', 'logs/con.txt', 'logs/pair.log.']) {
    const f = fixture(t); fs.appendFileSync(path.join(f.packet, 'SHA256SUMS'), `${digest}  ${name}\n`);
    assert.throws(() => preparePublication(f.options), /Noncanonical/); absent(f);
  }
  for (const name of ['logs/pair.log', 'LOGS/PAIR.LOG', 'SHA256SUMS']) {
    const f = fixture(t); fs.appendFileSync(path.join(f.packet, 'SHA256SUMS'), `${digest}  ${name}\n`);
    assert.throws(() => preparePublication(f.options), /Duplicate or recursive/); absent(f);
  }
  const missing = fixture(t), sums = path.join(missing.packet, 'SHA256SUMS');
  fs.writeFileSync(sums, fs.readFileSync(sums, 'utf8').split('\n').filter(line => !line.endsWith('  receipts/build.json')).join('\n'));
  assert.throws(() => preparePublication(missing.options), /not checksum-bound/); absent(missing);
  const linked = fixture(t), external = path.join(linked.root, 'outside');
  fs.renameSync(path.join(linked.packet, 'logs'), external);
  fs.symlinkSync(external, path.join(linked.packet, 'logs'), process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => preparePublication(linked.options), /Linked/); absent(linked);
});

test('evidence admission rejects failed, interrupted, incomplete, unordered and mismatched selections', t => {
  const mutations = [
    e => { e.schema = 'unknown'; }, e => { e.selection_status = 'fail'; }, e => { e.pipeline_status = 'fail'; },
    e => { e.validation_failures.push('invalid'); }, e => { e.reviewed_commit = 'b'.repeat(40); },
    e => { e.pair_id = 'b'.repeat(64); }, e => { e.observations[1].status = 'fail'; },
    e => { e.observations[7].status = 'not run'; }, e => { e.observations[8].status = 'pass'; },
    e => { [e.observations[1], e.observations[2]] = [e.observations[2], e.observations[1]]; },
    e => { e.observations.pop(); }, e => { e.observations[2].exit_code = 1; },
    e => { delete e.observations[2].log; }, e => { e.stop_after = 'production-build'; },
    e => { e.observations[7].verified_pair_sha256 = 'b'.repeat(64); }, e => { e.observations[7].ended_at = 'private string'; },
  ];
  for (const mutate of mutations) {
    const f = fixture(t); mutate(f.evidence); changeJson(f, 'evidence.json', f.evidence);
    assert.throws(() => preparePublication(f.options)); absent(f);
  }
});

test('expected pair/source and independent receipt bindings cannot be replaced by fresh packet checksums', t => {
  for (const option of ['expectedPair', 'expectedCommit']) {
    const f = fixture(t); f.options[option] = 'b'.repeat(option === 'expectedPair' ? 64 : 40);
    assert.throws(() => preparePublication(f.options)); absent(f);
  }
  for (const [name, mutate] of [
    ['native', r => { r.manifest.source.git_commit = 'b'.repeat(40); }],
    ['native', r => { r.archive_sha256 = 'b'.repeat(64); }],
    ['vsix', r => { r.engine.native_manifest_sha256 = 'b'.repeat(64); }],
    ['setup', r => { r.build_receipt_sha256 = 'b'.repeat(64); }],
    ['setup', r => { r.archive.file = '../escape.exe'; }],
    ['build', r => { r.qualification_build = true; }],
    ['build', r => { r.source_commit = 'b'.repeat(40); }],
  ]) {
    const f = fixture(t); mutate(f[name]); changeJson(f, `receipts/${name}.json`, f[name]);
    assert.throws(() => preparePublication(f.options)); absent(f);
  }
  const f = fixture(t); f.pair.receipts.native_sha256 = 'b'.repeat(64); changeJson(f, 'pair.json', f.pair);
  f.evidence.observations[7].verified_pair_sha256 = p.fileHash(path.join(f.packet, 'pair.json')); changeJson(f, 'evidence.json', f.evidence);
  assert.throws(() => preparePublication(f.options), /receipt binding/); absent(f);
});

test('full automated pipeline still requires manual qualification; output cannot overwrite or enter packet', t => {
  const f = fixture(t); f.evidence.pipeline_status = 'pass'; f.evidence.stop_after = 'installed-editor';
  for (let i = 8; i < stages.length; i++) {
    const id = stages[i]; f.write(`logs/${id}.log`, privateText);
    f.evidence.observations[i] = { id, status: 'pass', exit_code: 0, log: `logs/${id}.log` };
  }
  changeJson(f, 'evidence.json', f.evidence);
  const result = preparePublication(f.options);
  assert.equal(result.qualification.status, 'qualification-required'); assert.equal(result.qualification.pipelineStatus, 'pass');
  assert.throws(() => preparePublication(f.options), /new directory/);
  const other = fixture(t); other.options.output = path.join(other.packet, 'publication');
  assert.throws(() => preparePublication(other.options), /outside the packet/); absent(other);
});

test('self-consistent receipt rewrites cannot bypass production build and candidate identity guards', t => {
  for (const mutate of [
    f => { f.build.qualification_build = true; }, f => { f.build.executable_sha256 = 'b'.repeat(64); },
    f => { f.build.target = 'aarch64-pc-windows-msvc'; }, f => { f.build.source_stable = false; },
    f => { f.native.manifest.release.candidate_id = 'b'.repeat(64); f.setup.candidate_id = 'b'.repeat(64); },
    f => { f.native.manifest.release.native_version = '0.2.0'; },
  ]) {
    const f = fixture(t); mutate(f);
    const rewritten = f.rebind(); f.options.expectedPair = rewritten.pair_id;
    assert.throws(() => preparePublication(f.options), /identity mismatch|Production build|Invalid beta/); absent(f);
  }
});
