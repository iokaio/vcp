// SPDX-License-Identifier: Apache-2.0
'use strict';
// Qualification child: decrypted bytes arrive only on stdin, never argv/logs.
const assert=require('node:assert/strict'),crypto=require('node:crypto');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
let input=[],size=0;
process.stdin.on('data',b=>{size+=b.length;if(size>32*1024*1024)process.exit(1);input.push(b);});
process.stdin.on('end',()=>{try{
 const {envelope:e,expected:x}=JSON.parse(Buffer.concat(input));
 const bytes=a=>{assert(Array.isArray(a)&&a.every(v=>Number.isInteger(v)&&v>=0&&v<=255));return Buffer.from(a);};
 const body=bytes(e.body),writer=bytes(e.writer),signature=bytes(e.signature);
 assert.equal(writer.length,32);assert.equal(signature.length,64);assert.deepEqual(e.writer,x.writer);
 const key=crypto.createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),writer]),format:'der',type:'spki'});
 assert(crypto.verify(null,Buffer.concat([Buffer.from('vcp-portable-manifest-signature-v1\0'),body]),key,signature));
 const m=JSON.parse(body);assert.equal(m.format,'vcp-signed-age/1');assert.equal(m.workspace,x.workspace);assert.equal(m.lineage,x.lineage);
 assert.equal(m.sequence,x.sequence);assert.equal(m.deletion,x.deletion);assert.equal(m.parent,x.parent);assert.equal(hash(body),x.manifest_sha256);
 const names=Object.keys(m.objects).sort();assert(names.length>0&&names.length<=4096);assert.deepEqual(names,Object.keys(e.payloads).sort());
 const payloads=new Map();let total=0;
 for(const name of names){assert.match(name,/^[a-f0-9]{64}$/);const b=bytes(e.payloads[name]);assert.equal(m.objects[name].sha256,name);assert.equal(b.length,m.objects[name].bytes);assert.equal(hash(b),name);total+=b.length;assert(total<=16*1024*1024);payloads.set(name,b);
  for(const excluded of ['AGE-SECRET-KEY-','VCP writer Ed25519','synthetic-cli-qualification'])assert(!b.includes(excluded));
 }
 const inventories=[...payloads.values()].flatMap(b=>{try{const v=JSON.parse(b);return v.format==='vcp-neutral-history/1'?[v]:[];}catch{return [];}});
 assert.equal(inventories.length,1);const inv=inventories[0];assert.equal(inv.workspace,x.workspace);assert.equal(inv.authority,'historical_only_rebind_required');assert.equal(inv.coverage.workspace_checkpoint,true);
 const state=JSON.parse(payloads.get(inv.canonical));assert.equal(state.watermark,inv.watermark);
 for(const row of Object.values(state.records))assert.equal(row.workspace,x.workspace);
 assert(Array.isArray(state.events)&&state.events.length>0);for(const row of state.events)assert.equal(row.event.workspace,x.workspace);
 assert(Object.keys(state.commands).length>0);for(const row of Object.values(state.commands))assert.equal(row.workspace,x.workspace);
 for(const collection of ['task','verification','ledger'])assert(x.records.some(row=>row.collection===collection));
 for(const row of x.records){const actual=state.records[row.collection+':'+row.id];assert.equal(actual.workspace,x.workspace);assert.deepEqual(actual.value,row.value);}
 const source=inv.inputs.checkpoint.sources['canary.txt'];assert.equal(typeof source,'string');
 const descriptor=state.records['artifact:'+source].value;assert.equal(descriptor.spec.scope.workspace,x.workspace);assert.equal(descriptor.state,'complete');
 const chunks=inv.parts.filter(p=>p.artifact===source&&p.role.kind==='chunk').sort((a,b)=>a.role.index-b.role.index);
 assert(chunks.length>0);const full=Buffer.concat(chunks.map((p,i)=>{assert.equal(p.role.index,i);const b=payloads.get(p.digest);assert.equal(p.bytes,b.length);return b;}));
 assert.equal(hash(full),x.source_sha256);assert.equal(full.length,x.source_bytes);assert.equal(descriptor.sha256,x.source_sha256);assert.equal(String(descriptor.length),String(x.source_bytes));
 process.stdout.write(JSON.stringify({schema:'p803-independent-envelope/1',signature_verified:true,writer_matches_enrollment:true,payloads_verified:names.length,payload_bytes_verified:total,scoped_source_verified:true,task_verification_ledger_records_verified:x.records.length,events_scope_verified:state.events.length,commands_scope_verified:Object.keys(state.commands).length,excluded_key_and_provider_markers:true,manifest_sha256:hash(body)})+'\n');
}catch{process.stderr.write('Independent envelope validation failed\n');process.exitCode=1;}});
