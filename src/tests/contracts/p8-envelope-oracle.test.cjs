// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),crypto=require('node:crypto'),path=require('node:path'),{spawnSync}=require('node:child_process');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function fixture(mutateState=()=>{}){
 const pair=crypto.generateKeyPairSync('ed25519'),writer=[...pair.publicKey.export({format:'der',type:'spki'}).subarray(-32)],source=Buffer.from('fresh canary\n');
 const payloads={};const add=v=>{const b=Buffer.isBuffer(v)?v:Buffer.from(JSON.stringify(v)),id=hash(b);payloads[id]=[...b];return id;};
 const sourceId=add(source),state={watermark:'1',records:{'artifact:canary':{workspace:'work',value:{spec:{scope:{workspace:'work'}},state:'complete',length:String(source.length),sha256:sourceId}}},events:[{event:{workspace:'work'}}],commands:{command:{workspace:'work'}}};
 const records=['task','verification','ledger'].map(collection=>({collection,id:'fixture-'+collection,value:{evidence:'retained-'+collection}}));
 for(const row of records)state.records[row.collection+':'+row.id]={workspace:'work',value:structuredClone(row.value)};
 mutateState(state);
 const canonical=add(state),inventory={format:'vcp-neutral-history/1',workspace:'work',watermark:'1',authority:'historical_only_rebind_required',coverage:{workspace_checkpoint:true},canonical,inputs:{checkpoint:{sources:{'canary.txt':'canary'}}},parts:[{artifact:'canary',role:{kind:'chunk',index:0},digest:sourceId,bytes:source.length}]};add(inventory);
 const manifest={format:'vcp-signed-age/1',workspace:'work',lineage:'a'.repeat(64),sequence:1,deletion:0,parent:null,objects:Object.fromEntries(Object.entries(payloads).map(([h,b])=>[h,{sha256:h,bytes:b.length}]))};
 const body=Buffer.from(JSON.stringify(manifest)),signature=crypto.sign(null,Buffer.concat([Buffer.from('vcp-portable-manifest-signature-v1\0'),body]),pair.privateKey);
 return {envelope:{writer,body:[...body],signature:[...signature],payloads},expected:{workspace:'work',lineage:'a'.repeat(64),writer,sequence:1,deletion:0,parent:null,manifest_sha256:hash(body),source_sha256:sourceId,source_bytes:source.length,records}};
}
function run(value){return spawnSync(process.execPath,[path.join(__dirname,'../../crates/vcp-cli/tests/support/verify_packaged_envelope.cjs')],{input:JSON.stringify(value),encoding:'utf8',timeout:10000,windowsHide:true});}
test('independent oracle verifies signed scoped payload inventory and exact source',()=>{const r=run(fixture());assert.equal(r.status,0,r.stderr);assert.equal(JSON.parse(r.stdout).scoped_source_verified,true);});
test('independent oracle rejects signature, enrolled writer, inventory, scope and source mismatches without dumping plaintext',()=>{
 for(const mutate of [v=>v.envelope.signature[0]^=1,v=>v.expected.writer=[...v.expected.writer].reverse(),v=>delete v.envelope.payloads[Object.keys(v.envelope.payloads)[0]],v=>v.expected.workspace='other',v=>v.expected.source_sha256='0'.repeat(64),v=>v.expected.manifest_sha256='0'.repeat(64)]){
  const value=fixture();mutate(value);const r=run(value);assert.notEqual(r.status,0);assert.equal(r.stdout,'');assert.equal(r.stderr,'Independent envelope validation failed\n');
 }
});
test('validly signed snapshots cannot omit baseline history or import cross-workspace events and commands',()=>{
 for(const mutate of [s=>delete s.records['task:fixture-task'],s=>delete s.records['verification:fixture-verification'],s=>delete s.records['ledger:fixture-ledger'],s=>s.events[0].event.workspace='other',s=>s.commands.command.workspace='other',s=>s.events=[],s=>s.commands={}]){
  const r=run(fixture(mutate));assert.notEqual(r.status,0);assert.equal(r.stdout,'');assert.equal(r.stderr,'Independent envelope validation failed\n');
 }
});
