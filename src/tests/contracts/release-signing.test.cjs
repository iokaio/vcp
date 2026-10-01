// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const s = require('../../../scripts/release/signing.cjs');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const policy = { status:'signed', provider:'azure-artifact-signing', account:'ioka-llc-signing', profile:'WritingForgePro',
  endpoint:'https://wus2.codesigning.azure.net/', publisher:'CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US',
  identity_eku:'1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669' };
function fixture() {
  return { schema:'vcp-authenticode-transform/1', stage:'native', status:'verified', candidate_id:hash('candidate'),
    reviewed_commit:'a'.repeat(40), build_receipt_sha256:hash('build'), policy,
    tools:{signtool_sha256:hash('signtool'),dlib_sha256:hash('dlib')}, files:['engine','launcher'].map(role => ({
      role, input_sha256:hash(role+'original'),input_bytes:512,output_sha256:hash(role+'signed'),output_bytes:528,content_preserved:true,
      signature:{status:'Valid',subject:policy.publisher,identity_eku:policy.identity_eku,certificate_sha256:hash('cert'),
        timestamp_certificate_sha256:hash('timestamp'),timestamp_present:true},verification:{exit_code:0,log_sha256:hash('log')}
    })) };
}
function pe(plus) {
  const input=Buffer.alloc(512);input.writeUInt16LE(0x5a4d);input.writeUInt32LE(64,60);input.writeUInt32LE(0x4550,64);
  input.writeUInt16LE(plus?240:224,84);input.writeUInt16LE(plus?0x20b:0x10b,88);
  input.writeUInt32LE(16,88+(plus?108:92));
  const output=Buffer.concat([input,Buffer.alloc(16)]), security=88+(plus?112:96)+32;
  output.writeUInt32LE(512,security);output.writeUInt32LE(16,security+4);output.writeUInt32LE(1234,152);
  output.writeUInt32LE(12,512);output.writeUInt16LE(0x200,516);output.writeUInt16LE(2,518);output.writeUInt32LE(0x12345678,520);
  return {input,output,security};
}
test('signing admission binds original build, complete roles, publisher, timestamp and verification',()=>{
  const receipt=fixture();assert.equal(s.validateReceipt(receipt,policy,{stage:'native',inputs:{engine:receipt.files[0].input_sha256}}),receipt);
  for(const mutate of [r=>r.files.pop(),r=>r.files[1].role='engine',r=>r.files[0].content_preserved=false,
    r=>r.files[0].signature.subject='CN=Someone Else',r=>r.files[0].signature.identity_eku='1.2.3',
    r=>r.files[0].signature.timestamp_present=false,r=>delete r.files[0].signature.timestamp_certificate_sha256,
    r=>r.files[0].verification.exit_code=2,r=>r.files[0].input_sha256=r.files[0].output_sha256,
    r=>delete r.tools.dlib_sha256,r=>r.policy.account='unapproved']) {
    const changed=structuredClone(receipt);mutate(changed);assert.throws(()=>s.validateReceipt(changed,policy));
  }
  assert.throws(()=>s.validateReceipt(receipt,policy,{candidateId:hash('other')}));
  assert.throws(()=>s.validateReceipt(receipt,policy,{inputs:{engine:hash('other')}}));
  const release={signing:policy,candidate_id:receipt.candidate_id,reviewed_commit:receipt.reviewed_commit};
  assert.throws(()=>s.validateBinding({status:'unsigned'},release,receipt.build_receipt_sha256,'native'));
  assert.throws(()=>s.validateBinding({status:'signed'},{signing:{status:'unsigned'}},receipt.build_receipt_sha256,'native'));
});
test('PE signing proof permits only certificate append, checksum and directory changes for PE32 and PE32+',t=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-signing-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const original=path.join(root,'original.exe'),signed=path.join(root,'signed.exe');
  for(const plus of [false,true]) {
    const {input,output,security}=pe(plus);fs.writeFileSync(original,input);fs.writeFileSync(signed,output);
    assert.equal(s.verifyPeTransformation(original,signed).output_sha256,hash(output));
    for(const mutate of [b=>b[400]^=1,b=>b.writeUInt32LE(504,security),b=>b.writeUInt32LE(8,security+4),
      b=>b.writeUInt16LE(1,518),b=>b[527]=1,b=>b.writeUInt32LE(100,512)]) {
      const changed=Buffer.from(output);mutate(changed);fs.writeFileSync(signed,changed);
      assert.throws(()=>s.verifyPeTransformation(original,signed));
    }
    fs.writeFileSync(signed,Buffer.concat([output,Buffer.alloc(8)]));assert.throws(()=>s.verifyPeTransformation(original,signed));
    fs.writeFileSync(original,output);fs.writeFileSync(signed,output);assert.throws(()=>s.verifyPeTransformation(original,signed),/unsigned original/);
  }
});
test('signed package verifies exact receipt bytes and both executable outputs',t=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-signing-payload-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const receipt=fixture(),build={executable_sha256:receipt.files[0].input_sha256,launcher_sha256:receipt.files[1].input_sha256,
    release:{signing:policy,candidate_id:receipt.candidate_id,reviewed_commit:receipt.reviewed_commit}};
  const buildBytes=JSON.stringify(build);receipt.build_receipt_sha256=hash(buildBytes);fs.writeFileSync(path.join(root,'build-receipt.json'),buildBytes);
  for(const [i,name] of ['vcp.exe','vcp-launch.exe'].entries()) {const bytes=Buffer.alloc(528);bytes.write(name);receipt.files[i].output_sha256=hash(bytes);fs.writeFileSync(path.join(root,name),bytes);}
  const bytes=JSON.stringify(receipt);fs.writeFileSync(path.join(root,'signing-receipt.json'),bytes);
  const binding={status:'signed',receipt:'signing-receipt.json',receipt_sha256:hash(bytes),transformation:receipt};
  s.verifyPayloadSigning(root,build,binding);
  fs.appendFileSync(path.join(root,'signing-receipt.json'),' ');assert.throws(()=>s.verifyPayloadSigning(root,build,binding),/receipt bytes/);
  fs.writeFileSync(path.join(root,'signing-receipt.json'),bytes);fs.writeFileSync(path.join(root,'vcp-launch.exe'),'tampered');
  assert.throws(()=>s.verifyPayloadSigning(root,build,binding),/payload mismatch/);
});
test('signed pairing requires matching signed engine, launcher, setup and uninstaller',()=>{
  const p=require('../../../scripts/release/provenance.cjs'),nativeReceipt=fixture(),setupReceipt=structuredClone(nativeReceipt);
  setupReceipt.stage='setup';setupReceipt.files.forEach((row,i)=>row.role=i?'uninstaller':'setup');
  const wrapper=receipt=>({status:'signed',receipt:'signing-receipt.json',receipt_sha256:hash(JSON.stringify(receipt)),transformation:receipt});
  const release={schema:'vcp-release-identity/1',signing:policy,candidate_id:nativeReceipt.candidate_id,reviewed_commit:nativeReceipt.reviewed_commit,sdk_version:'0.2.2',vsix_version:'0.2.2'};
  const native={schema:'vcp-distribution-result/1',status:'release-candidate',archive_sha256:hash('zip'),manifest:{release,source:{dirty:false},
    build:{status:'verified-release-build',receipt_sha256:nativeReceipt.build_receipt_sha256},signing:wrapper(nativeReceipt),
    files:[{path:'vcp.exe',sha256:nativeReceipt.files[0].output_sha256},{path:'vcp-launch.exe',sha256:nativeReceipt.files[1].output_sha256}]}};
  const vsix={schema:'vcp-vsix-package/1',release,extension:{version:'0.2.2',source:{git_commit:release.reviewed_commit,dirty:false}},sdk:{version:'0.2.2'},archive:{sha256:hash('vsix')},
    engine:{native_archive_sha256:native.archive_sha256,executable_sha256:nativeReceipt.files[0].output_sha256,build_receipt_sha256:nativeReceipt.build_receipt_sha256,source_commit:release.reviewed_commit,source_dirty:false}};
  const setup={schema:'vcp-setup-result/1',candidate_id:release.candidate_id,native_archive_sha256:native.archive_sha256,
    launcher_sha256:nativeReceipt.files[1].output_sha256,archive:{sha256:setupReceipt.files[0].output_sha256},signing:wrapper(setupReceipt)};
  assert.equal(p.pairIdentity(native,vsix,setup).signing.status,'signed');
  assert.throws(()=>p.pairIdentity(native,vsix),/signed setup/);
  for(const mutate of [value=>value.signing={status:'unsigned'},value=>value.signing.transformation.files.pop(),
    value=>value.launcher_sha256=hash('other launcher'),value=>value.archive.sha256=hash('other setup')]) {
    const changed=structuredClone(setup);mutate(changed);assert.throws(()=>p.pairIdentity(native,vsix,changed));
  }
  const changed=structuredClone(native);changed.manifest.files.pop();assert.throws(()=>p.pairIdentity(changed,vsix,setup));
});
