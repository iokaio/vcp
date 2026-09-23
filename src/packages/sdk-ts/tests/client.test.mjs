// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {connect,scope,session,tick} from './support/fake.mjs';

test('inflight abort retains wire slot; queued abort sends nothing; late response is consumed',async()=>{
 const {client,transport}=await connect();
 try {
  const abort=new AbortController(); const first=client.call('session/read',{scope},{signal:abort.signal});
  const firstRejected=assert.rejects(first,e=>e.code==='ABORTED');abort.abort();await firstRejected;
  const queuedAbort=new AbortController();const queued=client.call('session/read',{scope},{signal:queuedAbort.signal});const rejected=assert.rejects(queued,e=>e.code==='ABORTED');queuedAbort.abort();await rejected;
  const final=client.call('session/read',{scope});assert.equal(transport.writes.length,1);
  transport.reply(transport.writes[0],session);await tick();assert.equal(transport.writes.length,2);
  transport.reply(transport.writes[1],session);assert.deepEqual(await final,session);
 }finally{await client.dispose();}
});
test('queue bounded before send and caller mutation cannot change queued payload',async()=>{
 const {client,transport}=await connect();
 const calls=[client.call('session/read',{scope})];
 const params={scope:{...scope}};calls.push(client.call('session/read',params));params.scope.session='changed';
 for(let i=1;i<32;i++)calls.push(client.call('session/read',{scope}));
 const settlements=Promise.allSettled(calls);
 await assert.rejects(client.call('session/read',{scope}),e=>e.code==='RESOURCE_LIMIT');
 transport.reply(transport.writes[0],session);await tick();assert.equal(transport.writes[1].params.scope.session,'session');
 await client.dispose();await settlements;
});
test('unsolicited or duplicate replies poison connection, wrong typed result never resolves',async()=>{
 const {client,transport}=await connect();
 const pending=client.call('session/read',{scope});
 const rejected=assert.rejects(pending,e=>e.code==='MALFORMED_PEER');
 transport.reply(transport.writes[0],{kind:'unsubscribed',value:{subscription:'subscription'}});await rejected;
 await assert.rejects(client.call('session/read',{scope}),e=>e.code==='DISPOSED');await client.dispose();
 const other=await connect();const read=other.client.call('session/read',{scope});other.transport.reply(other.transport.writes[0],session);await read;
 other.transport.reply(other.transport.writes[0],session);await assert.rejects(other.client.call('session/read',{scope}),e=>e.code==='DISPOSED');await other.client.dispose();
});
test('deadline closes transport and preserves original command identity for reconciliation',async()=>{
 const {client,transport}=await connect(['controller/acquire']);
 const pending=client.call('controller/acquire',{scope,command_id:'stable-operation',expected_revision:null},{timeoutMs:10});
 await assert.rejects(pending,e=>e.code==='TIMEOUT'&&e.commandId==='stable-operation');assert.equal(transport.writes.length,1);await client.dispose();
});
test('unnegotiated method denied locally without sending',async()=>{
 const {client,transport}=await connect([]);
 await assert.rejects(client.call('session/read',{scope}),e=>e.code==='CAPABILITY_UNAVAILABLE');assert.equal(transport.writes.length,0);await client.dispose();
});
test('queued deadline expires without closing healthy transport or consuming its active slot',async()=>{
 const {client,transport}=await connect();
 const first=client.call('session/read',{scope},{timeoutMs:1000});
 await assert.rejects(client.call('session/read',{scope},{timeoutMs:10}),e=>e.code==='TIMEOUT');
 assert.equal(transport.closed,0);assert.equal(transport.writes.length,1);
 transport.reply(transport.writes[0],session);await first;
 const next=client.call('session/read',{scope});transport.reply(transport.writes[1],session);await next;await client.dispose();
});
