// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const {selected,rangeBytes,exportArchive} = require('../../../scripts/evals/export-execution-archive.cjs');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture() {
  const scope = {workspace:'w',session:'s',task:'t'}, bytes = Buffer.from('{"allocation":{}}');
  const descriptor = {spec:{id:'a',scope,schema:'context-manifest/1',channel:'evidence'},state:'complete',length:String(bytes.length),sha256:hash(bytes)};
  const bundle = {schema_version:1,kind:'inspection_bundle',task:{scope},source_watermark:'8',views:{tools:[{items:[{collection:'artifact',visibility:'available',id:'a',record:descriptor}]}]}};
  const page = {scope,source_watermark:'8',next_cursor:null,gaps:[],items:[{artifact:'a',visibility:'available',range:{start:0,end:bytes.length},bytes:[...bytes]}]};
  return {bundle,descriptor,page,bytes};
}
test('selected captures must have exact authorized scoped descriptors', () => {
  const {bundle} = fixture();
  assert.equal(selected(bundle).length,1);
  bundle.views.routing = structuredClone(bundle.views.tools);
  assert.equal(selected(bundle).length,1);
  bundle.views.routing[0].items[0].record.length='7';
  assert.throws(() => selected(bundle),/Conflicting/);
  delete bundle.views.routing;
  bundle.views.tools[0].items[0].record.spec.scope={workspace:'w',session:'s',task:'foreign'};
  assert.throws(() => selected(bundle),/descriptor/);
});
test('range refuses changed cut, hidden data, incorrect identity and malformed octets', () => {
  const {bundle,descriptor,page,bytes} = fixture();
  assert.deepEqual(rangeBytes(page,bundle,descriptor,0,65536),bytes);
  for (const mutate of [p=>p.source_watermark='9',p=>p.scope={...p.scope,task:'other'},
    p=>p.items[0].artifact='b',p=>p.items[0].visibility='redacted',p=>p.items[0].bytes[0]=256,
    p=>p.items[0].range.end++,p=>p.next_cursor='more',p=>p.gaps=[{visibility:'unavailable'}]]) {
    const changed=structuredClone(page); mutate(changed);
    assert.throws(() => rangeBytes(changed,bundle,descriptor,0,65536));
  }
});
test('only declared credential omissions permit intact captured-byte export', () => {
  const {bundle,descriptor,page,bytes} = fixture();
  page.gaps=[{artifact:'a',capture_state:'complete',omissions:['authentication_headers'],visibility:'omitted',reason:'only retained observed bytes are available; not reconstructed'},
    {artifact:'a',omissions:['recovery_material'],visibility:'redacted',range:null,reason:'excluded at capture boundary; no retained byte offsets exist'}];
  assert.deepEqual(rangeBytes(page,bundle,descriptor,0,65536),bytes);
  page.gaps[1].omissions=['owner_redaction'];
  assert.throws(() => rangeBytes(page,bundle,descriptor,0,65536));
});
test('export publishes manifest only after hash-verified CLI ranges and refuses overwrite', () => {
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-diagnostic-export-'));
  try {
    const {bundle,page,bytes}=fixture(), bundleFile=path.join(temporary,'bundle.json');
    fs.writeFileSync(bundleFile,JSON.stringify(bundle));
    const config={executable:'vcp',workspace:'workspace',data:'data',bundleFile,directory:path.join(temporary,'good')};
    const frame={schema_version:1,correlation:'inspection',scope:null,type:'result',exit_code:0,data:page};
    const invoke=(_exe,args)=>{assert.equal(args[7],'inspect');return Buffer.from(JSON.stringify(frame)+'\n');};
    assert.deepEqual(exportArchive(config,invoke),{artifacts:1,bytes:bytes.length});
    assert.deepEqual(fs.readFileSync(path.join(config.directory,'artifacts/a.bin')),bytes);
    const manifest=JSON.parse(fs.readFileSync(path.join(config.directory,'manifest.json')));
    const receipt=manifest.inspections[0];
    assert.equal(receipt.sha256,hash(fs.readFileSync(path.join(config.directory,receipt.path))));
    assert.throws(() => exportArchive(config,invoke),/exist/i);
    config.directory=path.join(temporary,'mixed');
    assert.throws(() => exportArchive(config,(...args)=>Buffer.concat([invoke(...args),Buffer.from('{"type":"error"}\n')])),/inspection failed/);
    assert.equal(fs.existsSync(path.join(config.directory,'manifest.json')),false);
    config.directory=path.join(temporary,'corrupt');page.items[0].bytes[0]^=1;
    assert.throws(() => exportArchive(config,invoke),/hash/);
    assert.equal(fs.existsSync(path.join(config.directory,'manifest.json')),false);
  } finally {fs.rmSync(temporary,{recursive:true,force:true});}
});
