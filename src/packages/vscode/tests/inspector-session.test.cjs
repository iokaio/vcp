// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const { InspectorSession } = require('../dist/inspector_session.js');
const { decodeInspectorArtifact } = require('../dist/inspector_projection.js');
const scope = {workspace:'workspace',session:'session'};
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve,reject; const promise=new Promise((a,b)=>{resolve=a;reject=b;}); return {promise,resolve,reject}; }
const history = (task='root', text='retained', cursor='next') => ({kind:'history',value:{scope,task,source_watermark:'9007199254740993',observed_watermark:'9007199254740994',newer_events:'1',search_scope:text,rows:[],gaps:[],claim_links:[],claim_links_truncated:false,next_cursor:cursor,complete:cursor===null}});
const status = generation => ({phase:'connected',generation,editorTrusted:true,engineTrust:'trusted',limitations:[]});
function harness(handler=()=>history(), prompt=async()=>undefined) {
 const calls=[],states=[],contexts=[]; let invalidations=0, dispatched=0;
 const actions={register(c){contexts.push(c);return [{id:require('node:crypto').randomUUID(),label:'Controlled action'}];},invalidate(){invalidations++;},records(){return[];},async dispatch(){dispatched++;}};
 const client={scope, initialized:{methods:['history/query','memory/query','memory/history','context/inspect','routing/explain','artifact/read','usage/read','policy/read','routing/status','routing/reportRead','memory/forgetPreviewRead','memory/forgetRead','backup/status','backup/read'],capabilities:['history/query/1','memory/query-sources/1','memory/history/1','policy/inspection/1','routing/status/1','routing/optimizer/1','memory/retention/1','backup/publisher/1']},async call(method,params,options){calls.push({method,params,options});return handler(method,params,options);},async dispose(){}};
 client.initialized.capabilities.push(...client.initialized.methods);
 const session=new InspectorSession({publish(s){states.push(s);},prompt,actions,refreshMs:60000});
 const connect=async()=>{session.connection(status(1),client);session.selection({task:'root',revision:'1',synchronized:true});session.visible(true);await tick();};
 return {session,client,calls,states,contexts,actions,connect,state:()=>states.at(-1),invalidations:()=>invalidations,dispatched:()=>dispatched,async click(label){const action=states.at(-1).actions.find(a=>a.label===label);assert.ok(action,label);await session.dispatch({action:'invoke',id:action.id});}};
}
test('workspace revision changes invalidate old handles even without a new connection generation',async t=>{
 const h=harness();t.after(()=>h.session.dispose());await h.connect();
 const old=h.state().actions.find(action=>action.label==='Controlled action').id;
 h.session.connection({...status(1),workspaceRevision:'2'},h.client);
 await h.session.dispatch({action:'invoke',id:old});assert.equal(h.dispatched(),0);
 await tick();assert.equal(h.session.current().status.workspaceRevision,'2');
});

test('history pagination is host-only; every page/back reauthorizes and stale handles do nothing',async t=>{
 const h=harness((_,p)=>history(p.task,'command:evil javascript:alert(1)',p.cursor?null:'secret-cursor'));t.after(()=>h.session.dispose());await h.connect();
 assert.ok(JSON.stringify(h.state()).includes('9007199254740993'));
 assert.ok(!JSON.stringify(h.state()).includes('secret-cursor'));
 const old=h.state().actions.find(a=>a.label==='Next page').id;
 await h.click('Next page');assert.equal(h.calls.at(-1).params.cursor,'secret-cursor');
 const count=h.calls.length;await h.session.dispatch({action:'invoke',id:old});assert.equal(h.calls.length,count);
 await h.click('Back — recheck access');assert.equal(h.calls.at(-1).params.cursor,null);
 assert.equal(h.dispatched(),0);
});
test('selection A-B-A and generation changes suppress delayed replies including delayed failures',async t=>{
 const slow=deferred();let first=true;const h=harness((_,p)=>{if(first){first=false;return slow.promise;}return history(p.task,'new');});t.after(()=>h.session.dispose());await h.connect();
 const signal=h.calls[0].options.signal;
 h.session.selection({task:'other',revision:'2',synchronized:true});await tick();h.session.selection({task:'root',revision:'3',synchronized:true});await tick();
 const before=h.state();assert.ok(signal.aborted);slow.reject(new Error('private-native-path'));await tick();assert.equal(h.state(),before);assert.ok(!JSON.stringify(h.states).includes('private-native-path'));
 const second=deferred();h.client.call=async()=>second.promise;const pending=h.session.refresh();h.session.connection(status(2),undefined);second.resolve(history('root','obsolete'));await pending;assert.equal(h.state().phase,'disconnected');assert.equal(h.session.current(),undefined);
});
test('hidden content is purged and reveal/ready reread rather than restore retained state',async t=>{
 let restricted=false;const h=harness(()=>{if(restricted)throw new Error('purged secret');return history('root','visible sensitive snippet');});t.after(()=>h.session.dispose());await h.connect();
 const old=h.state().actions[0].id;h.session.visible(false);assert.equal(h.state().sections.length,0);assert.equal(h.session.current(),undefined);
 restricted=true;h.session.visible(true);await tick();assert.equal(h.state().phase,'unavailable');assert.ok(!JSON.stringify(h.state()).includes('visible sensitive'));
 await h.session.dispatch({action:'invoke',id:old});assert.equal(h.dispatched(),0);
 const count=h.calls.length;await h.session.dispatch({action:'ready'});assert.equal(h.calls.length,count+1);assert.equal(h.state().sections.length,0);
});
test('purge between pages discards cursor; refresh restarts and never republishes old page',async t=>{
 let gone=false;const h=harness((_,p)=>{if(p.cursor)throw new Error('stale');return history('root',gone?'new safe view':'old snippet');});t.after(()=>h.session.dispose());await h.connect();gone=true;
 await h.click('Next page');assert.equal(h.state().phase,'unavailable');assert.equal(h.state().sections.length,0);await h.session.refresh();assert.equal(h.calls.at(-1).params.cursor,null);assert.ok(!JSON.stringify(h.state()).includes('old snippet'));
});
test('scope/task/section mismatches fail closed and unsupported profiles make no requests',async t=>{
 const h=harness(()=>({...history(),value:{...history().value,scope:{...scope,session:'foreign'}}}));t.after(()=>h.session.dispose());await h.connect();assert.equal(h.state().phase,'unavailable');assert.equal(h.state().actions.length,0);
 h.client.initialized.capabilities=[];const before=h.calls.length;await h.session.refresh();assert.equal(h.state().phase,'unsupported');assert.equal(h.calls.length,before);
});
test('host query prompt is bounded and old task prompt completion cannot submit',async t=>{
 const answer=deferred();const h=harness((m,p)=>m==='memory/query'?{kind:'memory_query',value:{scope,task:p.task,generation:null,generation_watermark:null,canonical_watermark:'1',sequence:'1',findings:[],rebuild_required:false,degraded:[],truncated:false,complete:true}}:history(p.task),()=>answer.promise);t.after(()=>h.session.dispose());await h.connect();await h.session.dispatch({action:'tab',tab:'memory'});
 const waiting=h.click('Search memory');h.session.selection({task:'child',revision:'1',synchronized:true});answer.resolve('old secret query');await waiting;assert.ok(!h.calls.some(c=>c.method==='memory/query'));
});
test('oversized exact preview rejects entire display and clears every action',async t=>{
 const h=harness();t.after(()=>h.session.dispose());await h.connect();const old=h.state().actions[0].id;
 h.session.showOptimizerPreview({scope,selected:[],prior:{allowed_models:['x'.repeat(300000)]},persisted:{},effective:{}});
 assert.equal(h.state().phase,'partial');assert.equal(h.session.current(),undefined);await h.session.dispatch({action:'invoke',id:old});assert.equal(h.dispatched(),0);
});
test('artifact decoding enforces actual bytes, identity/hash and range continuation',()=>{
 const request={scope,task:'root',artifact:'a',offset:'9007199254740993',length:3};
 const value={artifact:'a',offset:request.offset,total_bytes:'9007199254740999',encoding:'base64',content:Buffer.from('abc').toString('base64'),sha256:'a'.repeat(64),complete:false};
 assert.equal(decodeInspectorArtifact(value,request).next,'9007199254740996');
 for(const bad of [{...value,content:'!!!!'},{...value,artifact:'other'},{...value,offset:'0'},{...value,complete:true},{...value,content:Buffer.from('four').toString('base64')}])assert.throws(()=>decodeInspectorArtifact(bad,request));
 assert.throws(()=>decodeInspectorArtifact(value,request,'b'.repeat(64)));
});
test('gap invalidation prevents task reads until explicit synchronized selection',async t=>{
 const h=harness();t.after(()=>h.session.dispose());await h.connect();h.session.invalidate('gap');const count=h.calls.length;await h.session.refresh();assert.equal(h.calls.length,count);h.session.selection({task:'root',revision:'1',synchronized:true});await tick();assert.equal(h.calls.length,count+1);
});
test('all read tabs use real methods and preserve exact policy/cost/publication distinctions',async t=>{
 const h=harness((method,p)=>{
  const base={scope,task:p.task,watermark:'3'};
  switch(method){
   case 'history/query':return history(p.task);
   case 'context/inspect': case 'routing/explain': return {kind:'evidence',value:{...base,rows:[{id:'context',schema:'context-manifest/1',revision:'1',content:{artifact:'a',offset:'0',length:'3',sha256:'a'.repeat(64)}}],next_cursor:null,complete:true}};
   case 'usage/read': return {kind:'usage',value:{...base,root:'root',currency:'USD',cap_micros:'9007199254740995',settled_micros:'2',reserved_micros:'3',unresolved_micros:'9007199254740994',overrun:false}};
   case 'policy/read':return {kind:'policy',value:{...base,section:p.section,persisted:{mode:'ask'},effective:{state:'unavailable',reason:'binding_unavailable'},rows:[],next_cursor:null,complete:true}};
   case 'routing/status':return {kind:'routing_status',value:{...base,section:p.section,rows:[],next_cursor:null,complete:true}};
   case 'backup/status':return {kind:'backup_status',value:{scope,capability:{state:'loaded',reference:'PRIVATE-HOST-HANDLE',generation:'1',configuration_revision:'2'},cloud_transfer:'unknown',restore_verification:'not_observed'}};
   case 'backup/read':return {kind:'backup_job',value:{scope,operation:p.operation,phase:'published',cancel_requested:true,local_publication:'published',checkpoint:'pending',cleanup:'reconciliation_required',source_pins:'held',cloud_transfer:'unknown',restore_verification:'not_observed'}};
   case 'routing/reportRead':return {kind:'routing_report',value:{scope,report:p.report,section:p.section,rows:[],next_cursor:null,complete:true}};
   case 'memory/forgetRead':return {kind:'retention',value:{scope,task:p.task,job:p.job,logical_unavailable:true,rewrite_complete:false,local_cleanup_complete:false}};
   default:throw new Error(method);
  }
 });t.after(()=>h.session.dispose());await h.connect();
 for(const tab of ['evidence','cost','policy','routing','publisher']){await h.session.dispatch({action:'tab',tab});assert.equal(h.state().phase,'current',tab);}
 assert.ok(!JSON.stringify(h.state()).includes('PRIVATE-HOST-HANDLE'));
 await h.session.dispatch({action:'tab',tab:'cost'});assert.ok(JSON.stringify(h.state()).includes('9007199254740994'));assert.ok(JSON.stringify(h.state()).includes('Unresolved'));
 await h.session.dispatch({action:'tab',tab:'policy'});await h.click('Show grants');assert.equal(h.calls.at(-1).params.section,'grants');assert.ok(JSON.stringify(h.state()).includes('binding_unavailable'));
 await h.session.dispatch({action:'tab',tab:'routing'});await h.click('Observed routing evidence');assert.equal(h.calls.at(-1).method,'routing/explain');assert.ok(!h.calls.some(c=>c.method==='evidence/read'));
 await h.session.showReport('report');await h.click('Report cohorts');assert.equal(h.calls.at(-1).params.section,'cohorts');
 await h.session.showPublisher('operation');const shown=JSON.stringify(h.state());assert.ok(shown.includes('published')&&shown.includes('pending')&&shown.includes('unknown'));
 await h.session.showPruningJob('job');assert.equal(h.calls.at(-1).method,'memory/forgetRead');assert.equal(h.calls.at(-1).params.job,'job');
});
test('pruning offset paging and memory versions keep exact target identity',async t=>{
 const preview={scope,task:'root',preview:'OPAQUE-PREVIEW',digest:'a'.repeat(64),action:'purge',watermark:'3',authority:'1',deletion:'0',selected_count:'33',dependent_count:'0',protected_count:'1',retained_bytes:'12',bytes_are_exact:false,offset:0,next_offset:16,targets:[],expires_in_ms:60000};
 const h=harness((m,p)=>m==='memory/forgetPreviewRead'?{kind:'retention_preview',value:{...preview,offset:p.offset,next_offset:null}}:m==='memory/history'?{kind:'memory_history',value:{scope,task:p.task,claim:p.claim,at:'9007199254740993',watermark:'5',versions:[],next_cursor:null,complete:true}}:history());t.after(()=>h.session.dispose());await h.connect();
 h.session.showPruningPreview(preview);assert.ok(!JSON.stringify(h.state()).includes('OPAQUE-PREVIEW'));await h.click('Next preview page');assert.equal(h.calls.at(-1).params.offset,16);assert.equal(h.calls.at(-1).params.preview,'OPAQUE-PREVIEW');
 await h.session.showClaim('claim');assert.equal(h.calls.at(-1).method,'memory/history');assert.equal(h.calls.at(-1).params.claim,'claim');assert.ok(JSON.stringify(h.state()).includes('9007199254740993'));
});
test('UTF8 split ranges remain inspectable as exact bytes without fabricated replacement text',()=>{
 const request={scope,task:'root',artifact:'a',offset:'0',length:2};const bytes=Buffer.from([0xe2,0x82]);
 const result=decodeInspectorArtifact({artifact:'a',offset:'0',total_bytes:'3',encoding:'base64',content:bytes.toString('base64'),sha256:'a'.repeat(64),complete:false},request);
 assert.equal(result.next,'2');assert.ok(result.text.includes(bytes.toString('base64')));assert.ok(result.text.includes('split UTF-8'));assert.ok(!result.text.includes('\ufffd'));
});


test('advertised read method without negotiated method capability is unavailable without a request',async t=>{
 const h=harness();t.after(()=>h.session.dispose());h.client.initialized.capabilities=h.client.initialized.capabilities.filter(value=>value!=='history/query');await h.connect();assert.equal(h.state().phase,'unsupported');assert.equal(h.calls.length,0);
});
test('hide discards cursors, back navigation, and ephemeral pruning review requests',async t=>{
 const h=harness((_,params)=>history(params.task,'retained',params.cursor?null:'old-cursor'));t.after(()=>h.session.dispose());await h.connect();await h.click('Next page');assert.equal(h.calls.at(-1).params.cursor,'old-cursor');h.session.visible(false);h.session.visible(true);await tick();assert.equal(h.calls.at(-1).params.cursor,null);assert.ok(!h.state().actions.some(action=>action.label==='Back — recheck access'));
 h.session.showPruningPreview({scope,task:'root',preview:'ephemeral-preview',digest:'a'.repeat(64),action:'purge',offset:0,next_offset:null,targets:[],expires_in_ms:60000});const count=h.calls.length;h.session.visible(false);h.session.visible(true);await tick();assert.equal(h.calls.length,count);assert.ok(!JSON.stringify(h.state()).includes('ephemeral-preview'));assert.ok(!h.state().sections.some(section=>section.title.includes('Exact pruning')));
});

test('same-task event refresh preserves durable report/job targets but discards old content and cursors',async t=>{
 const h=harness((m,p)=>m==='routing/reportRead'?{kind:'routing_report',value:{scope,report:p.report,section:p.section,rows:[],next_cursor:'old-cursor',complete:false}}:m==='memory/forgetRead'?{kind:'retention',value:{scope,task:p.task,job:p.job,logical_unavailable:true,rewrite_complete:false,local_cleanup_complete:false}}:history(p.task));t.after(()=>h.session.dispose());await h.connect();
 await h.session.showReport('durable-report');await h.click('Next page');assert.equal(h.calls.at(-1).params.cursor,'old-cursor');
 h.session.selection({task:'root',revision:undefined,synchronized:false});await tick();assert.equal(h.calls.at(-1).method,'routing/reportRead');assert.equal(h.calls.at(-1).params.report,'durable-report');assert.equal(h.calls.at(-1).params.cursor,null);
 h.session.selection({task:'root',revision:'2',synchronized:true});await tick();await h.session.showPruningJob('durable-job');
 const before=h.calls.length;h.session.selection({task:'root',revision:undefined,synchronized:false});await tick();assert.equal(h.calls.length,before);assert.equal(h.state().sections.length,0);
 h.session.selection({task:'root',revision:'3',synchronized:true});await tick();assert.equal(h.calls.at(-1).method,'memory/forgetRead');assert.equal(h.calls.at(-1).params.job,'durable-job');
 h.session.selection({task:'other',revision:'1',synchronized:true});await tick();assert.equal(h.state().sections.length,0);assert.ok(!JSON.stringify(h.state()).includes('durable-job'));
});

test('same-task refresh preserves history search criteria but expires exact pruning preview',async t=>{
 const h=harness((m,p)=>history(p.task,'fresh',null),async()=> 'retained query');t.after(()=>h.session.dispose());await h.connect();await h.click('Search history');
 h.session.selection({task:'root',revision:undefined,synchronized:false});await tick();h.session.selection({task:'root',revision:'2',synchronized:true});await tick();assert.equal(h.calls.at(-1).params.text,'retained query');assert.equal(h.calls.at(-1).params.cursor,null);
 const preview={scope,task:'root',preview:'expired-selection',digest:'a'.repeat(64),action:'purge',watermark:'3',authority:'1',deletion:'0',selected_count:'1',dependent_count:'0',protected_count:'0',retained_bytes:'12',bytes_are_exact:true,offset:0,next_offset:null,targets:[],expires_in_ms:60000};
 h.session.showPruningPreview(preview);const old=h.state().actions[0]?.id;const count=h.calls.length;h.session.invalidate('events');await tick();assert.equal(h.calls.length,count);assert.equal(h.state().sections.length,0);if(old)await h.session.dispatch({action:'invoke',id:old});assert.equal(h.dispatched(),0);
 h.session.showPruningPreview(preview);h.session.selection({task:'root',revision:'3',synchronized:true});await tick();assert.equal(h.calls.length,count);assert.equal(h.state().sections.length,0);
});
test('action failures show a fixed safe status and preserve journal metadata without replay',async t=>{
 const h=harness();t.after(()=>h.session.dispose());await h.connect();
 const record={commandId:'11111111-1111-4111-8111-111111111111',method:'routing/apply',phase:'unknown'};
 h.actions.records=()=>[record];h.actions.dispatch=async()=>{throw Error('C:/private/provider-key.txt SECRET');};
 const count=h.calls.length;await h.click('Controlled action');assert.equal(h.state().phase,'unavailable');assert.equal(h.state().sections.length,0);assert.deepEqual(h.state().commands,[record]);assert.ok(h.state().message.includes('reconcile'));assert.ok(!JSON.stringify(h.states).includes('SECRET'));assert.equal(h.calls.length,count);
});
test('external reference prompts read actual scoped reports/previews/jobs without mutating',async t=>{
 const prompts=[];const h=harness((m,p)=>{
  if(m==='history/query')return history();
  if(m==='routing/reportRead')return{kind:'routing_report',value:{scope,report:p.report,section:p.section,rows:[],next_cursor:null,complete:true}};
  if(m==='memory/forgetPreviewRead')return{kind:'retention_preview',value:{scope,task:p.task,preview:p.preview,offset:p.offset,targets:[],next_offset:null}};
  if(m==='memory/forgetRead')return{kind:'retention',value:{scope,task:p.task,job:p.job}};
  if(m==='backup/status')return{kind:'backup_status',value:{scope,active_operation:'active-backup'}};
  if(m==='backup/read')return{kind:'backup_job',value:{scope,operation:p.operation,phase:'interrupted',cancel_requested:false,local_publication:'not_observed',checkpoint:'not_observed',cleanup:'pending',source_pins:'not_observed',cloud_transfer:'unknown',restore_verification:'not_observed'}};
  throw Error(m);
 },async input=>{prompts.push(input);return 'external-ID_1';});t.after(()=>h.session.dispose());await h.connect();
 for(const [tab,label,method,key] of [['optimizer','Open report','routing/reportRead','report'],['pruning','Open pruning preview','memory/forgetPreviewRead','preview'],['pruning','Open pruning job','memory/forgetRead','job'],['publisher','Open backup','backup/read','operation']]){
  await h.session.dispatch({action:'tab',tab});await h.click(label);assert.equal(h.calls.at(-1).method,method);assert.equal(h.calls.at(-1).params[key],'external-ID_1');assert.deepEqual(h.calls.at(-1).params.scope,scope);
 }
 await h.session.dispatch({action:'tab',tab:'publisher'});await h.click('Open active backup');assert.equal(h.calls.at(-1).params.operation,'active-backup');assert.equal(prompts.length,4);assert.ok(prompts.every(p=>p.maxLength===96));assert.equal(h.dispatched(),0);
});
test('reference prompts reject hostile IDs and late selection answers, and require negotiated reads',async t=>{
 let answer='command:steal';const late=deferred();const h=harness(()=>history(),()=>answer==='late'?late.promise:Promise.resolve(answer));t.after(()=>h.session.dispose());await h.connect();await h.session.dispatch({action:'tab',tab:'optimizer'});const before=h.calls.length;
 await h.click('Open report');assert.equal(h.calls.length,before);assert.equal(h.state().phase,'unavailable');assert.ok(!JSON.stringify(h.state()).includes('command:steal'));
 await h.session.refresh();answer='late';const pending=h.click('Open report');h.session.selection({task:'child',revision:'2',synchronized:true});late.resolve('valid-ID');await pending;assert.ok(!h.calls.some(c=>c.method==='routing/reportRead'));
 h.client.initialized.capabilities=h.client.initialized.capabilities.filter(c=>c!=='routing/optimizer/1');await h.session.refresh();assert.ok(!h.state().actions.some(a=>a.label==='Open report'));
 await h.session.dispatch({action:'tab',tab:'pruning'});h.session.selection({task:'child',revision:undefined,synchronized:false});await tick();assert.ok(!h.state().actions.some(a=>a.label.startsWith('Open pruning')));
});

test('pruning job read rejects a different job identity before displaying or registering actions',async t=>{
 const h=harness((method,params)=>method==='memory/forgetRead'?{kind:'retention',value:{scope,task:params.task,job:'different-job',logical_unavailable:true,rewrite_complete:true,local_cleanup_complete:true}}:history());
 t.after(()=>h.session.dispose());await h.connect();await h.session.showPruningJob('requested-job');
 assert.equal(h.calls.at(-1).params.job,'requested-job');assert.equal(h.state().phase,'unavailable');assert.equal(h.state().sections.length,0);assert.equal(h.state().actions.length,0);assert.equal(h.session.current(),undefined);
 assert.ok(!JSON.stringify(h.state()).includes('different-job'));assert.ok(!h.contexts.some(context=>context.page?.kind==='retention'));
});
