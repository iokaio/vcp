// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {rename} from 'node:fs/promises';
import {launchLocal,reconnectObserverLocal} from '../dist/index.js';
import {createEncryptedBackup} from '../examples-dist/encrypted-publisher.js';
let raw='';for await(const chunk of process.stdin)raw+=chunk;const input=JSON.parse(raw);
const capabilities=['backup/publisher/1','backup/status','backup/create','backup/read','backup/retry','backup/cancel','controller/read','controller/acquire','workspace/open','workspace/binding/1','command/read'];
const initialize={protocol_version:'1.0',client:{name:'native-publisher',version:'1'},capabilities,required_capabilities:capabilities};
const clients=[];const value=(reply,kind)=>{assert.equal(reply.kind,kind);return reply.value;};
const safe=reply=>{const wire=JSON.stringify(reply);assert(Buffer.byteLength(wire)<=16384);for(const marker of ['AGE-SECRET-KEY','VCP writer','native-publisher-recovery','native-publisher-vault','native-publisher-staging','.recovery',':\\\\'])assert(!wire.includes(marker),`private marker in public reply: ${marker}`);return reply;};
const status=async client=>value(safe(await client.call('backup/status',{scope:input.scope})),'backup_status');
const read=async client=>value(safe(await client.call('backup/read',{scope:input.scope,operation:input.operation})),'backup_job');
const launch=async options=>{const client=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,initialize,...options});clients.push(client);return client;};
let moved=false;
try {
  if(input.scenario==='reopen') {
    const observer=await launch({role:'observer',transport:'stdio'});
    assert.equal((await status(observer)).capability.state,'unavailable');
    assert.deepEqual(await observer.call('command/read',{scope:input.scope,command_id:input.operation}),input.receipt);
    const job=await read(observer);assert.equal(job.local_publication,'published');assert.equal(job.cleanup,'complete');assert.equal(job.cloud_transfer,'unknown');assert.equal(job.restore_verification,'not_observed');
    assert.notEqual(value(await observer.call('controller/read',{scope:input.scope}),'controller').ownership,'this_connection');
    process.stdout.write(JSON.stringify({ok:true}));
  } else {
    process.stderr.write('phase: unavailable explicit profile\n');
    const failed=await launch({role:'controller',transport:'stdio',publisher:{profile:input.profile+'.missing'}});
    assert.equal((await status(failed)).capability.state,'unavailable');await failed.dispose();
    process.stderr.write('phase: loaded native publisher\n');
    // Exercise the documented default initializer, including the required binding profile.
    const owner=await launch({role:'controller',transport:'windows_pipe',publisher:{profile:input.profile},initialize:undefined});
    value(await owner.call('controller/acquire',{scope:input.scope,command_id:randomUUID(),expected_revision:null}),'acceptance');
    const initial=await status(owner);assert.equal(initial.capability.state,'loaded');assert.equal(initial.busy,false);assert.equal(initial.active_operation,null);
    assert.equal(initial.destination,'local_encrypted_vault');assert.equal(initial.cloud_transfer,'unknown');
    const workspace=value(await owner.call('workspace/open',{command_id:randomUUID(),host:owner.initialized.execution_host.id,root:input.root}),'workspace');
    process.stderr.write('phase: durable backup create\n');
    const receipt=safe(await createEncryptedBackup(owner,workspace,input.operation));value(receipt,'acceptance');
    const request={scope:input.scope,mutation:{command_id:input.operation,expected_revision:workspace.revision,steering_revision:'0'},expected_binding_revision:workspace.binding_revision,capability:initial.capability.reference,expected_capability_generation:initial.capability.generation};
    assert.deepEqual(safe(await owner.call('backup/create',request)),receipt);
    await assert.rejects(owner.call('backup/create',{...request,expected_capability_generation:'99999'}));
    const reference=owner.observerReconnectReference();assert(reference);
    await rename(input.profile,input.profile+'.held');moved=true;
    const observer=await reconnectObserverLocal({executable:input.executable,reference,initialize});clients.push(observer);
    assert.equal((await status(observer)).capability.reference,initial.capability.reference);
    assert.equal(value(await owner.call('controller/read',{scope:input.scope}),'controller').ownership,'this_connection');
    assert.equal(value(await observer.call('controller/read',{scope:input.scope}),'controller').ownership,'other_connection');
    await assert.rejects(observer.call('backup/create',{...request,mutation:{...request.mutation,command_id:randomUUID()}}));
    process.stderr.write('phase: observe encrypted local completion\n');
    let job;for(let i=0;i<600;i++) {
      job=await read(observer);
      if(job.local_publication==='published'&&job.cleanup==='complete')break;
      assert(!['interrupted','cancelled','reconciliation_required'].includes(job.phase),JSON.stringify(job));
      await new Promise(resolve=>setTimeout(resolve,50));
    }
    assert.equal(job.phase,'published');assert.equal(job.local_publication,'published');assert.equal(job.checkpoint,'matched');assert.equal(job.cleanup,'complete');assert.equal(job.source_pins,'released');assert.equal(job.cloud_transfer,'unknown');assert.equal(job.restore_verification,'not_observed');
    assert.deepEqual(await observer.call('command/read',{scope:input.scope,command_id:input.operation}),receipt);
    await rename(input.profile+'.held',input.profile);moved=false;
    process.stdout.write(JSON.stringify({ok:true,receipt}));
  }
}finally {
  if(moved)await rename(input.profile+'.held',input.profile);
  for(const client of clients.reverse())await client.dispose().catch(()=>{});
}
