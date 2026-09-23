// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {getEventListeners} from 'node:events';
import {connect,scope,tick} from './support/fake.mjs';
const methods=['events/subscribe','events/next','events/unsubscribe','session/snapshot'];
const page={kind:'events',value:{at_end:true,cursor:'cursor',events:[],snapshot_sequence:'0',subscription:'subscription'}};
test('stream has no prefetch and closes with exact subscription; gaps remain visible',async()=>{
 const {client,transport}=await connect(methods);const stream=client.events({scope,after_sequence:'0',limit:2});assert.equal(transport.writes.length,0);
 const next=stream.next();transport.reply(transport.writes[0],page);assert.deepEqual((await next).value,page);assert.equal(transport.writes.length,1);
 const second=stream.next();await new Promise(r=>setTimeout(r,110));
 const gap={kind:'gap',value:{reason:'cursor_expired',resubscribe_required:true,snapshot_sequence:'1',subscription:'subscription'}};
 // Use a real schema enum selected from generated schema in the fixture below.
 const schema=(await import('@vcp/protocol/schema.json',{with:{type:'json'}})).default;
 gap.value.reason=schema.definitions.GapReason.enum[0];
 transport.reply(transport.writes[1],gap);assert.equal((await second).value.kind,'gap');
 transport.onSend=request=>{if(request.method==='events/unsubscribe')transport.reply(request,{kind:'unsubscribed',value:{subscription:'subscription'}});};
 assert.equal((await stream.next()).done,true);assert.equal(transport.writes.at(-1).method,'events/unsubscribe');await client.dispose();
});
test('abandoned subscribe retains late reply long enough to unsubscribe',async()=>{
 const {client,transport}=await connect(methods);const stream=client.events({scope,after_sequence:'0',limit:2});
 const pending=stream.next();const rejected=assert.rejects(pending,e=>e.code==='ABORTED');const close=stream.close();await rejected;
 transport.onSend=request=>{if(request.method==='events/unsubscribe')transport.reply(request,{kind:'unsubscribed',value:{subscription:'subscription'}});};
 transport.reply(transport.writes[0],page);await close;assert.equal(transport.writes.at(-1).method,'events/unsubscribe');await client.dispose();
});
test('snapshot helper preserves gap after first page instead of claiming completeness',async()=>{
 const {client,transport}=await connect(methods);
 const pages=client.snapshots({scope,limit:1})[Symbol.asyncIterator]();
 const first=pages.next();transport.reply(transport.writes[0],{kind:'snapshot',value:{complete:false,event_cursor:'cursor',next_cursor:'page2',sequence:'0',session:{scope,revision:'0',configuration_revision:'0'},subscription:'subscription',tasks:[],watermark:'1'}});
 assert.equal((await first).value.kind,'snapshot');const second=pages.next();await tick();
 const schema=(await import('@vcp/protocol/schema.json',{with:{type:'json'}})).default;
 transport.reply(transport.writes[1],{kind:'gap',value:{reason:schema.definitions.GapReason.enum[0],resubscribe_required:true,snapshot_sequence:'2',subscription:'subscription'}});assert.equal((await second).value.kind,'gap');
 transport.onSend=request=>{if(request.method==='events/unsubscribe')transport.reply(request,{kind:'unsubscribed',value:{subscription:'subscription'}});};await pages.return();await client.dispose();
});
test('transport failure closes idle streams and removes external abort listeners without dispose',async()=>{
 const {client,transport}=await connect(methods);const abort=new AbortController();
 const stream=client.events({scope,after_sequence:'0',limit:2},{signal:abort.signal});
 assert.equal(getEventListeners(abort.signal,'abort').length,1);
 transport.failed();await tick();assert.equal(getEventListeners(abort.signal,'abort').length,0);
 assert.equal((await stream.next()).done,true);await client.dispose();
});
test('aborted first snapshot owns late subscription cleanup and leaves client usable',async()=>{
 const {client,transport}=await connect(methods);const abort=new AbortController();
 const pages=client.snapshots({scope,limit:1},{signal:abort.signal})[Symbol.asyncIterator]();
 const first=pages.next();const rejected=assert.rejects(first,e=>e.code==='ABORTED');abort.abort();await rejected;
 transport.onSend=request=>{if(request.method==='events/unsubscribe')transport.reply(request,{kind:'unsubscribed',value:{subscription:'late-snapshot'}});};
 transport.reply(transport.writes[0],{kind:'snapshot',value:{complete:true,event_cursor:'cursor',sequence:'0',session:{scope,revision:'0',configuration_revision:'0'},subscription:'late-snapshot',tasks:[],watermark:'1'}});
 await tick();assert.equal(transport.writes.at(-1).method,'events/unsubscribe');assert.equal(transport.writes.at(-1).params.subscription,'late-snapshot');assert.equal(transport.closed,0);await client.dispose();
});
test('snapshot continuation binds subscription, sequence and event cursor at its fixed watermark',async()=>{
 for(const field of ['subscription','sequence','event_cursor']){
  const {client,transport}=await connect(methods);const pages=client.snapshots({scope,limit:1})[Symbol.asyncIterator]();
  const value={complete:false,event_cursor:'cursor',next_cursor:'page2',sequence:'0',session:{scope,revision:'0',configuration_revision:'0'},subscription:'subscription',tasks:[],watermark:'1'};
  const first=pages.next();transport.reply(transport.writes[0],{kind:'snapshot',value});await first;
  const second=pages.next();const rejected=assert.rejects(second,e=>e.code==='MALFORMED_PEER');await tick();
  transport.reply(transport.writes[1],{kind:'snapshot',value:{...value,[field]:field==='sequence'?'1':'changed'}});await rejected;assert.ok(transport.closed>0);await client.dispose();
 }
});
test('event continuation rejects replacement subscription instead of silently switching ownership',async()=>{
 const {client,transport}=await connect(methods);const stream=client.events({scope,after_sequence:'0',limit:2});
 const first=stream.next();transport.reply(transport.writes[0],{...page,value:{...page.value,at_end:false}});await first;
 const second=stream.next();const rejected=assert.rejects(second,e=>e.code==='MALFORMED_PEER');
 transport.onSend=request=>{if(request.method==='events/unsubscribe')transport.reply(request,{kind:'unsubscribed',value:{subscription:'subscription'}});};
 transport.reply(transport.writes[1],{...page,value:{...page.value,subscription:'replaced'}});await rejected;await client.dispose();assert.ok(transport.closed>0);
});
