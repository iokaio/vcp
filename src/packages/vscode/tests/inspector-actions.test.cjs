// SPDX-License-Identifier: Apache-2.0
const test=require('node:test');
const assert=require('node:assert/strict');
const {InspectorActions}=require('../dist/inspector_actions.js');
const {InspectorJournal}=require('../dist/inspector_journal.js');
const scope={workspace:'workspace',session:'session'};
const methods=['command/read','routing/reportCapture','routing/preview','routing/apply','routing/rollback','routing/status','memory/forgetPreview','memory/forget','task/read','backup/status','backup/create','backup/retry','backup/cancel'];
const profiles=['routing/optimizer/1','memory/retention/1','backup/publisher/1'];
const deferred=()=>{let resolve;const promise=new Promise(done=>resolve=done);return {promise,resolve}};
const tick=()=>new Promise(setImmediate);
function fixture(options={}){
 const calls=[],saves=[],shown=[],prompts=[],receipts=new Map();let context;
 const session={refresh:async()=>shown.push(['refresh']),invalidate:reason=>shown.push(['invalidate',reason]),showReport:async id=>shown.push(['report',id]),showPublisher:async id=>shown.push(['publisher',id]),showPruningJob:async id=>shown.push(['pruning',id]),showOptimizerPreview:view=>shown.push(['preview',view]),showPruningPreview:view=>shown.push(['pruningPreview',view])};
 const client={scope,initialized:{methods,capabilities:[...methods,...profiles]},call:async(method,params)=>{
   calls.push({method,params:structuredClone(params)});
   if(options.call){const reply=await options.call(method,params);if(reply!==undefined)return reply}
   if(method==='command/read'){const value=receipts.get(params.command_id);if(!value)throw Error('missing receipt');return {kind:'acceptance',value}}
   if(method==='task/read')return {kind:'task',value:{task:params.task,scope,revision:'7',steering_revision:'8'}};
   if(method==='routing/status')return {kind:'routing_status',value:{persisted:{revision:'11'}}};
   const value={command_id:params.mutation.command_id,scope:params.scope,task:params.task??null,turn:null,outcome:'accepted',revision:'9007199254740993',watermark:'22'};receipts.set(value.command_id,value);return {kind:'acceptance',value};
 }};
 context={tab:'publisher',client,synchronized:true,status:{phase:'connected',generation:1,role:'controller',editorTrusted:true,engineTrust:'trusted',workspaceRevision:'4',bindingRevision:'2'},task:'task',page:{kind:'backup_status',value:{scope,capability:{state:'loaded',reference:'reference',generation:'1',configuration_revision:'7'},busy:false}}};
 const journal=new InspectorJournal(async rows=>{saves.push(structuredClone(rows));if(options.save)await options.save(rows)},options.saved);
 const actions=new InspectorActions({current:()=>context,session:()=>session,prompt:async(title,value)=>{prompts.push(title);return options.prompt?options.prompt(title,value):title.includes('start')?'0':'100'},choose:async(title,choices)=>options.choose?options.choose(title,choices):choices[0],confirm:async message=>options.confirm?options.confirm(message):true},journal);
 return {actions,journal,calls,saves,shown,prompts,receipts,context:()=>context,set:update=>context={...context,...update},register:()=>actions.register(context),find:label=>actions.register(context).find(action=>action.label===label)};
}
test('publisher submits once after durable identity save using exact displayed capability',async()=>{
 const gate=deferred();const f=fixture({save:rows=>rows[0].phase==='submitting'?gate.promise:undefined});const action=f.find('Publish encrypted workspace backup');const pending=f.actions.dispatch(action.id);await tick();assert.equal(f.calls.length,0);assert.equal(f.saves[0][0].phase,'submitting');
 const duplicate=f.actions.dispatch(action.id);gate.resolve();await Promise.all([pending,duplicate]);assert.equal(f.calls.length,1);assert.equal(f.calls[0].method,'backup/create');assert.equal(f.calls[0].params.capability,'reference');assert.equal(f.calls[0].params.expected_binding_revision,'2');assert.equal(f.journal.records()[0].phase,'accepted');assert.deepEqual(f.shown,[['publisher',f.journal.records()[0].commandId]]);
});
test('navigation during a host prompt or save prevents any mutation submission',async()=>{
 const prompt=deferred();const f=fixture({confirm:()=>prompt.promise});const action=f.find('Publish encrypted workspace backup');const pending=f.actions.dispatch(action.id);await tick();f.set({tab:'history'});prompt.resolve(true);await assert.rejects(pending,/changed/);assert.equal(f.calls.length,0);assert.equal(f.saves.length,0);
 const save=deferred();const g=fixture({save:rows=>rows[0].phase==='submitting'?save.promise:undefined});const start=g.actions.dispatch(g.find('Publish encrypted workspace backup').id);await tick();g.set({status:{...g.context().status,generation:2}});save.resolve();await assert.rejects(start,/changed/);assert.equal(g.calls.length,0);assert.equal(g.journal.records()[0].phase,'rejected');
});
test('unknown outcomes survive reload and reconciliation reads original command without replay',async()=>{
 const f=fixture({call:async method=>{if(method==='backup/create')throw Error('lost response')}});await assert.rejects(f.actions.dispatch(f.find('Publish encrypted workspace backup').id),/Reconcile/);assert.equal(f.journal.records()[0].phase,'unknown');assert.ok(f.find('Publish encrypted workspace backup').disabledReason);
 const saved=f.journal.records(),g=fixture({saved});await g.actions.dispatch(g.find('Reconcile submitted commands').id);assert.deepEqual(g.calls.map(call=>call.method),['command/read']);assert.equal(g.journal.records()[0].phase,'unknown');assert.equal(f.calls.filter(call=>call.method==='backup/create').length,1);
});
test('accepted late receipt persists after navigation without opening content in replacement inspector',async()=>{
 const response=deferred();const f=fixture({call:async(method,params)=>{if(method==='backup/create'){await response.promise;return {kind:'acceptance',value:{command_id:params.mutation.command_id,scope,task:null,outcome:'accepted',revision:'9',watermark:'10'}}}}});const pending=f.actions.dispatch(f.find('Publish encrypted workspace backup').id);await tick();f.set({tab:'history'});response.resolve();await pending;assert.equal(f.journal.records()[0].phase,'accepted');assert.deepEqual(f.shown,[]);
});
test('observer, revoked trust, absent publisher and missing negotiated profile cannot invoke writes',async()=>{
 for(const update of [f=>({status:{...f.context().status,role:'observer'}}),f=>({status:{...f.context().status,editorTrusted:false}}),f=>({status:{...f.context().status,engineTrust:'untrusted'}}),f=>({page:{kind:'backup_status',value:{scope,capability:{state:'unavailable'},busy:false}}}),f=>({client:{...f.context().client,initialized:{methods,capabilities:methods}}}),f=>({client:{...f.context().client,initialized:{methods,capabilities:[...methods.filter(method=>method!=='backup/create'),...profiles]}}}),f=>({client:{...f.context().client,initialized:{methods,capabilities:[...methods.filter(method=>method!=='command/read'),...profiles]}}})]){
   const f=fixture();f.set(update(f));const action=f.find('Publish encrypted workspace backup');assert.ok(!action||action.disabledReason,'unavailable authority/profile must disable the control');if(action)await f.actions.dispatch(action.id);assert.equal(f.calls.length,0);assert.equal(f.saves.length,0);
 }
});
test('policy review expiration is checked after confirmation and stale engine rejection never retries',async()=>{
 const confirm=deferred();const f=fixture({confirm:()=>confirm.promise});const now=Date.now;let clock=1000;Date.now=()=>clock;
 try{f.set({tab:'optimizer',page:{kind:'routing_preview',value:{operation:'apply',expires_in_ms:20,binding_revision:'2',preview_id:'preview',preview_sha256:'a'.repeat(64)}}});const pending=f.actions.dispatch(f.find('Apply reviewed policy').id);await tick();clock=1021;confirm.resolve(true);await assert.rejects(pending,/expired/);assert.equal(f.calls.length,0);await f.actions.dispatch(f.find('Apply reviewed policy').id);assert.equal(f.calls.length,0,'re-registering the same cached review cannot renew its expiry')}finally{Date.now=now}
 const g=fixture({call:async method=>{if(method==='routing/apply')throw {code:'rpc',classification:{applicationCode:'VERSION_CONFLICT',retry:'after_revalidation'}}}});g.set({tab:'optimizer',page:{kind:'routing_preview',value:{operation:'apply',expires_in_ms:60000,binding_revision:'2',preview_id:'preview',preview_sha256:'b'.repeat(64)}}});await assert.rejects(g.actions.dispatch(g.find('Apply reviewed policy').id),/rejected/);assert.equal(g.journal.records()[0].phase,'rejected');assert.equal(g.calls.length,1);assert.equal(g.calls[0].params.preview_sha256,'b'.repeat(64));
});
test('pruning unavailable preview capability cannot issue a hidden preview mutation',async()=>{
 const f=fixture();f.set({tab:'pruning',client:{...f.context().client,initialized:{methods:methods.filter(method=>method!=='memory/forgetPreview'),capabilities:[...methods,...profiles]}}});assert.equal(f.find('Preview pruning for selected task'),undefined);await f.actions.dispatch('00000000-0000-0000-0000-000000000000');assert.equal(f.calls.length,0);
});

test('preview completion can register an enabled apply action before its dispatch unwinds',async()=>{
 let apply;
 const preview={operation:'apply',expires_in_ms:60000,binding_revision:'2',preview_id:'preview',preview_sha256:'c'.repeat(64)};
 const f=fixture({call:async method=>method==='routing/preview'?{kind:'routing_preview',value:preview}:undefined});
 f.set({tab:'optimizer',page:{kind:'routing_report',value:{report:'report'}}});
 // Match the session callback: publishing a preview synchronously registers its actions.
 const ownerSession={};
 // Use a separate owner with this synchronous session implementation and the same journal/context.
 const actions=new InspectorActions({current:()=>f.context(),session:()=>({...ownerSession,refresh:async()=>{}}),prompt:async()=> '50',choose:async()=> 'input_tokens',confirm:async()=>true},f.journal);
 ownerSession.showOptimizerPreview=value=>{f.set({page:{kind:'routing_preview',value}});apply=actions.register(f.context()).find(action=>action.label==='Apply reviewed policy')};
 const select=actions.register(f.context()).find(action=>action.label==='Preview policy edit');await actions.dispatch(select.id);assert.ok(apply);assert.equal(apply.disabledReason,undefined);await actions.dispatch(apply.id);assert.equal(f.calls.filter(call=>call.method==='routing/apply').length,1);
});

test('unsynchronized tasks disable task-bound previews without disabling workspace publication or report capture',async()=>{
 const f=fixture();f.set({synchronized:false});assert.equal(f.find('Publish encrypted workspace backup').disabledReason,undefined);
 f.set({tab:'optimizer'});assert.equal(f.find('Capture optimization report').disabledReason,undefined);const rollback=f.find('Preview policy rollback');assert.ok(rollback.disabledReason);await f.actions.dispatch(rollback.id);
 f.set({tab:'pruning'});const pruning=f.find('Preview pruning for selected task');assert.ok(pruning.disabledReason);await f.actions.dispatch(pruning.id);assert.equal(f.calls.length,0);
});

test('observer pruning preview is a scoped read even without trust or command reconciliation, while apply remains disabled',async()=>{
 const preview={scope,task:'task',preview:'preview',digest:'a'.repeat(64),action:'exclude',expires_in_ms:60000};
 const f=fixture({saved:[{version:1,commandId:'11111111-1111-4111-8111-111111111111',scope,method:'backup/create',phase:'unknown'}],call:async(method)=>method==='memory/forgetPreview'?{kind:'retention_preview',value:preview}:undefined});
 f.set({tab:'pruning',page:undefined,status:{phase:'connected',generation:1,role:'observer',editorTrusted:false,engineTrust:'untrusted'}});
 f.context().client.initialized={methods:['memory/forgetPreview','memory/forget'],capabilities:['memory/forgetPreview','memory/forget','memory/retention/1']};
 const action=f.find('Preview pruning for selected task');assert.equal(action.disabledReason,undefined);await f.actions.dispatch(action.id);
 assert.deepEqual(f.calls.map(c=>c.method),['memory/forgetPreview']);assert.deepEqual(f.calls[0].params.scope,scope);assert.equal(f.calls[0].params.task,'task');assert.equal(f.calls[0].params.mutation,undefined);assert.equal(f.saves.length,0);assert.deepEqual(f.shown,[['pruningPreview',preview]]);
 f.set({page:{kind:'retention_preview',value:preview}});const apply=f.find('Apply reviewed pruning');assert.ok(apply.disabledReason);await f.actions.dispatch(apply.id);assert.equal(f.calls.length,1);assert.equal(f.saves.length,0);
});

test('read-only preview still requires negotiated capability, synchronized task and current connection',async()=>{
 const f=fixture();f.set({tab:'pruning',status:{phase:'connected',role:'observer',editorTrusted:false}});
 f.context().client.initialized={methods:['memory/forgetPreview'],capabilities:['memory/forgetPreview']};assert.ok(f.find('Preview pruning for selected task').disabledReason);
 f.context().client.initialized.capabilities.push('memory/retention/1');f.set({synchronized:false});assert.ok(f.find('Preview pruning for selected task').disabledReason);
 f.set({synchronized:true,status:{phase:'disconnected',role:'observer'}});const action=f.find('Preview pruning for selected task');assert.ok(action.disabledReason);await f.actions.dispatch(action.id);assert.equal(f.calls.length,0);assert.equal(f.saves.length,0);
});

test('preview expiring during durable metadata save is rejected before engine submission',async()=>{
 const save=deferred();const f=fixture({save:rows=>rows[0].phase==='submitting'?save.promise:undefined});const now=Date.now;let clock=1000;Date.now=()=>clock;
 try{f.set({tab:'optimizer',page:{kind:'routing_preview',value:{operation:'apply',expires_in_ms:20,binding_revision:'2',preview_id:'preview',preview_sha256:'d'.repeat(64)}}});const pending=f.actions.dispatch(f.find('Apply reviewed policy').id);await tick();clock=1021;save.resolve();await assert.rejects(pending,/expired|changed/);assert.equal(f.calls.length,0);assert.equal(f.journal.records()[0].phase,'rejected')}finally{Date.now=now}
});

test('pruning reload opens actual returned job identity rather than the command UUID',async()=>{
 const nativeJob='native-pruning-job';const f=fixture({call:async(method,params)=>method==='memory/forget'?{kind:'forgotten',value:{acceptance:{command_id:params.mutation.command_id,scope,task:params.task,outcome:'accepted',revision:'9',watermark:'10'},job:{scope,task:params.task,job:nativeJob}}}:undefined});
 f.set({tab:'pruning',page:{kind:'retention_preview',value:{scope,task:'task',preview:'preview',digest:'e'.repeat(64),action:'exclude',expires_in_ms:60000,selected_count:'1',protected_count:'0',dependent_count:'0'}}});await f.actions.dispatch(f.find('Apply reviewed pruning').id);const record=f.journal.records()[0];assert.equal(record.target,nativeJob);assert.notEqual(record.commandId,nativeJob);assert.deepEqual(f.shown,[['pruning',nativeJob]]);
 const restored=fixture({saved:f.journal.records()});restored.set({tab:'pruning'});await restored.actions.dispatch(restored.find(`Open pruning job ${nativeJob}`).id);assert.deepEqual(restored.shown,[['pruning',nativeJob]]);assert.equal(restored.calls.length,0);
});
