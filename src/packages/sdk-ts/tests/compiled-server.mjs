// SPDX-License-Identifier: Apache-2.0
// Invoked by the native Rust fixture; stdin contains bounded launch metadata.
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { launchLocal, attachLocal } from '../dist/index.js';
import * as examples from '../examples-dist/index.js';
import { connectGated } from './delivery-gate.mjs';

const pause = (ms) => new Promise(resolve => setTimeout(resolve, ms));
const required = ['controller/read','controller/acquire','session/create','session/read','session/snapshot','events/subscribe','events/next','events/unsubscribe','task/read','task/cancel','artifact/read','command/read'];
const initialize = { protocol_version:'1.0', client:{name:'compiled-sdk-qualification',version:'1'}, capabilities:required, required_capabilities:required };
function value(reply, kind) { assert.equal(reply.kind,kind); return reply.value; }

export async function attachmentScenario(input) {
  const owned=[];
  const open = async (role, init=initialize) => {
    const client=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,transport:input.transport,role,initialize:init}); owned.push(client); return client;
  };
  try {
    // Failed pipe initialization still leaves an independently owned idle server;
    // qualify version rejection on stdio, whose owned server closes with bridge.
    if(input.transport==='stdio')await assert.rejects(open('observer',{...initialize,protocol_version:'2.0'}));
    const client=await open('controller');
    assert.deepEqual(client.scope,input.scope);
    assert.equal(client.role,'controller');
    const before=value(await client.call('task/read',{scope:input.scope,task:input.task}),'task');
    assert.equal(before.state,'paused');
    const child=value(await examples.inspectChildProgress(client,input.scope,'sdk-retained-child'),'task');
    assert.equal(child.parent,input.task); assert.equal(child.root,input.task); assert.equal(child.state,'paused');
    const cut=value(await examples.snapshotOrGap(client,{scope:input.scope,limit:128,cursor:null}),'snapshot');
    assert.equal(cut.complete,true);
    const original={scope:input.scope,mutation:{command_id:'sdk-create-once',expected_revision:'0',steering_revision:'0'},new_session:'sdk-created-session',configuration_revision:'0'};
    await assert.rejects(client.call('session/create',original));
    value(await client.call('controller/acquire',{scope:input.scope,command_id:'sdk-owner',expected_revision:null}),'acceptance');
    const first=await client.call('session/create',original);
    assert.deepEqual(await client.call('session/create',structuredClone(original)),first);
    assert.deepEqual(await client.reconcile({scope:input.scope,command_id:original.mutation.command_id}),first);
    await assert.rejects(client.call('session/create',{...original,new_session:'sdk-conflicting-session'}));
    const next={scope:input.scope,subscription:cut.subscription,cursor:cut.event_cursor};
    const events=value(await client.call('events/next',next),'events');
    assert.deepEqual(value(await client.call('events/next',next),'events'),events);
    let sequence=BigInt(cut.sequence);
    for(const event of events.events) { assert.equal(BigInt(event.sequence),++sequence); assert.equal(Object.hasOwn(event,'data'),false); }
    assert(events.events.some(event=>event.command_id==='sdk-create-once'));
    value(await client.call('events/unsubscribe',{scope:input.scope,subscription:cut.subscription}),'unsubscribed');
    assert.equal((await client.call('events/next',{...next,cursor:events.cursor})).kind,'gap');
    // More than the server's eight subscriptions: abandoning iteration must
    // release each subscription rather than leak an unread prefetch queue.
    for(let index=0;index<12;index++) {
      const stream=client.events({scope:input.scope,after_sequence:'0',limit:1});
      for await(const batch of stream){ assert.equal(batch.kind,'events'); assert(batch.value.events.length<=1); break; }
      await stream.close();
    }
    const firstBatch=await examples.firstEventBatch(client,{scope:input.scope,after_sequence:'0',limit:1});
    assert.equal(firstBatch.done,false);
    assert.equal(firstBatch.value.kind,'events');
    // Real server calls saturate bounded SDK admission. Excess readers fail
    // locally; admitted readers and the connection remain usable.
    const pressure=await Promise.allSettled(Array.from({length:128},()=>client.call('task/read',{scope:input.scope,task:input.task})));
    assert(pressure.some(result=>result.status==='rejected'));
    assert(pressure.some(result=>result.status==='fulfilled'));
    const artifact=await examples.readArtifactRange(client,{scope:input.scope,task:input.task,artifact:input.artifact,offset:'0',length:128});
    assert.deepEqual(Buffer.from(artifact.bytes),Buffer.from([0,...Buffer.from('sdk-retained-bytes'),255]));
    // Aborting a local wait is not an engine cancel command.
    const aborted=new AbortController(); aborted.abort();
    await assert.rejects(client.call('task/read',{scope:input.scope,task:input.task},{signal:aborted.signal}));
    assert.equal(value(await client.call('task/read',{scope:input.scope,task:input.task}),'task').state,'paused');
    const abandoned=client.events({scope:input.scope,after_sequence:'0',limit:1});
    assert.equal((await abandoned.next()).done,false); // Client disposal owns cleanup.
    if(input.transport==='windows_pipe') {
      const attachment=client.attachment(); const observerAttachment=client.observerAttachment();
      assert(!JSON.stringify(attachment).includes('ticket'));
      const observer=await attachLocal({executable:input.executable,attachment:observerAttachment,initialize}); owned.push(observer);
      assert.equal(observer.role,'observer');
      await assert.rejects(observer.call('controller/acquire',{scope:input.scope,command_id:'sdk-observer-denied',expected_revision:'0'}));
      await client.dispose();
      let lease;
      for(let i=0;i<100;i++){lease=value(await observer.call('controller/read',{scope:input.scope}),'controller'); if(lease.ownership==='released')break; await pause(20);}
      assert.equal(lease.ownership,'released');
      const reattached=await attachLocal({executable:input.executable,attachment,initialize}); owned.push(reattached);
      assert.equal(value(await reattached.call('task/read',{scope:input.scope,task:input.task}),'task').state,'paused');
      value(await reattached.call('controller/acquire',{scope:input.scope,command_id:'sdk-reconnect',expected_revision:lease.revision}),'acceptance');
      assert.deepEqual(await reattached.call('session/create',original),first);
    }
  } finally { for(const client of owned.reverse()) await client.dispose(); }
}

export async function pendingScenario(input) {
  const capabilities=[...required,'turn/start','approval/respond','approval/source-revisions/1'];
  const init={...initialize,capabilities,required_capabilities:capabilities};
  const owned=[];
  const read=async client=>value(await client.call('task/read',{scope:input.scope,task:input.task}),'task');
  const lease=async client=>value(await client.call('controller/read',{scope:input.scope}),'controller');
  try {
    const first=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,role:'controller',transport:'windows_pipe',rootTask:input.task,execution:{profile:input.profile,providerCredential:input.credential},initialize:init}); owned.push(first);
    const attachment=first.attachment();
    const observer=await attachLocal({executable:input.executable,attachment:first.observerAttachment(),initialize:init}); owned.push(observer);
    value(await first.call('controller/acquire',{scope:input.scope,command_id:'sdk-pending-owner',expected_revision:null}),'acceptance');
    const original={scope:input.scope,mutation:{command_id:'sdk-start-once',expected_revision:'0',steering_revision:'0'},task:input.task,turn:input.turn,objective:'Change value.txt once and verify it',constraints:[],acceptance:['value.txt contains 42'],budget:{cap_micros:'1000000',currency:'USD',max_requests:8,deadline_seconds:300}};
    const started=await examples.startAndInspect(first,original);
    const replay=await examples.retryOriginalStart(first,original);
    assert.deepEqual(replay,started.acceptance);
    let pending;
    for(let i=0;i<500;i++){pending=await read(observer);if(pending.pending_inputs.length)break;await pause(20);}
    assert(pending.pending_inputs.length);
    assert.deepEqual(await examples.inspectPendingInput(observer,{scope:input.scope,task:input.task}),pending.pending_inputs);
    const question=pending.pending_inputs[0];
    assert.equal(typeof question.effect_revision,'string'); assert.equal(typeof question.policy_revision,'string');
    const answer={scope:input.scope,task:input.task,mutation:{command_id:'sdk-deny-once',expected_revision:question.revision,steering_revision:pending.steering_revision},approval:question.id,operation_digest:question.operation_digest,effect_revision:question.effect_revision,policy_revision:question.policy_revision,decision:'deny'};
    await assert.rejects(examples.answerApproval(observer,answer));
    await first.dispose();
    let current;
    for(let i=0;i<500;i++){current=await lease(observer);if(current.ownership==='released')break;await pause(20);}
    assert.equal(current.ownership,'released');
    assert.deepEqual((await read(observer)).pending_inputs,pending.pending_inputs);
    const replacement=await attachLocal({executable:input.executable,attachment,initialize:init});owned.push(replacement);
    await assert.rejects(examples.answerApproval(replacement,answer));
    value(await replacement.call('controller/acquire',{scope:input.scope,command_id:'sdk-pending-reconnect',expected_revision:current.revision}),'acceptance');
    await assert.rejects(examples.answerApproval(replacement,{...answer,effect_revision:'18446744073709551615',mutation:{...answer.mutation,command_id:'sdk-stale-question'}}));
    const accepted=await examples.answerApproval(replacement,answer);
    assert.deepEqual(await examples.answerApproval(replacement,structuredClone(answer)),accepted);
    assert.deepEqual(await examples.reconcileOriginal(observer,{scope:input.scope,command_id:'sdk-deny-once'}),accepted);
    const after=await read(observer); assert.equal(after.state,'paused');assert.deepEqual(after.pending_inputs,[]);
  } finally {for(const client of owned.reverse())await client.dispose();}
}

export async function executionScenario(input) {
  const capabilities=[...required,'session/resume','turn/pause'];
  const init={...initialize,capabilities,required_capabilities:capabilities};
  const client=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,role:'controller',execution:{profile:input.profile,providerCredential:input.credential},initialize:init});
  try {
    const before=value(await client.call('task/read',{scope:input.scope,task:input.task}),'task');
    assert.equal(before.state,'paused');
    value(await client.call('controller/acquire',{scope:input.scope,command_id:'sdk-execution-owner',expected_revision:null}),'acceptance');
    const request={scope:input.scope,task:input.task,mutation:{command_id:'sdk-resume-once',expected_revision:before.revision,steering_revision:before.steering_revision}};
    const resumed=await examples.resumeAndInspect(client,request);
    for(let i=0;i<500;i++){if(await readFile(join(input.workspace,'value.txt'),'utf8')==='42\n')break;await pause(20);}
    assert.equal(await readFile(join(input.workspace,'value.txt'),'utf8'),'42\n');
    await pause(500); // The second synthetic provider response remains delayed.
    const active=value(await client.call('task/read',{scope:input.scope,task:input.task}),'task');
    assert.equal(active.state,'running'); assert.equal(typeof active.turn,'string');
    const paused=await examples.pauseAndInspect(client,{scope:input.scope,task:input.task,turn:active.turn,reason:'SDK explicit connected pause',mutation:{command_id:'sdk-connected-pause',expected_revision:active.revision,steering_revision:active.steering_revision}});
    assert.equal(paused.task.value.state,'paused');
    assert.equal(value(await client.call('controller/read',{scope:input.scope}),'controller').ownership,'this_connection');
    assert.deepEqual(await client.call('session/resume',request),resumed.acceptance);
    assert.equal(value(await client.call('task/read',{scope:input.scope,task:input.task}),'task').state,'paused');
    const signal=new AbortController(); signal.abort();
    await assert.rejects(client.call('task/cancel',{scope:input.scope,task:input.task,reason:'aborted local intent',mutation:{command_id:'sdk-unsent-cancel',expected_revision:paused.task.value.revision,steering_revision:paused.task.value.steering_revision}},{signal:signal.signal}));
    assert.equal(value(await client.call('task/read',{scope:input.scope,task:input.task}),'task').state,'paused');
  } finally {await client.dispose();}
}

export async function deliveryScenario(input) {
  const gate=await connectGated(input,initialize);const client=gate.client;
  try {
    value(await client.call('controller/acquire',{scope:input.scope,command_id:'sdk-delivery-owner',expected_revision:null}),'acceptance');
    const original={scope:input.scope,mutation:{command_id:'sdk-delivery-once',expected_revision:'0',steering_revision:'0'},new_session:'sdk-delivery-session',configuration_revision:'0'};
    const received=gate.hold(original.mutation.command_id);
    const abort=new AbortController();
    const awaited=client.call('session/create',original,{signal:abort.signal});
    const rejection=assert.rejects(awaited);
    const genuine=await received;
    assert.equal(genuine.result.kind,'acceptance'); // Durable admission, not a timer assumption.
    abort.abort();await rejection;
    gate.release();
    assert.deepEqual(await client.reconcile({scope:input.scope,command_id:original.mutation.command_id}),genuine.result);
    assert.deepEqual(await client.call('session/create',original),genuine.result);
    assert.equal(value(await client.call('task/read',{scope:input.scope,task:input.task}),'task').state,'paused');
  }finally{await client.dispose();}
}

if(process.argv[1]===fileURLToPath(import.meta.url)) {
  let bytes=Buffer.alloc(0);
  for await(const chunk of process.stdin){ assert(bytes.length+chunk.length<=16*1024); bytes=Buffer.concat([bytes,chunk]); }
  const input=JSON.parse(bytes.toString('utf8'));
  if(input.scenario==='attachment')await attachmentScenario(input);
  else if(input.scenario==='pending')await pendingScenario(input);
  else if(input.scenario==='execution')await executionScenario(input);
  else if(input.scenario==='delivery')await deliveryScenario(input);
  else assert.fail('unknown compiled SDK scenario');
  process.stdout.write(JSON.stringify({ok:true}));
}
