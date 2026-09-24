// SPDX-License-Identifier: Apache-2.0
// Real compiled host/SDK; expected semantics come from the CLI's governed queries.
import assert from 'node:assert/strict';
import {launchLocal} from '../dist/index.js';
let raw='';for await(const chunk of process.stdin)raw+=chunk;
const input=JSON.parse(raw);
const capabilities=['history/query','history/query/1','memory/history','memory/history/1','task/read','artifact/read','session/snapshot','events/unsubscribe','policy/read','policy/inspection/1','routing/status','routing/status/1'];
const initialize={protocol_version:'1.0',client:{name:'native-inspector-queries',version:'1'},capabilities,required_capabilities:capabilities};
const owned=[];
const value=(reply,kind)=>{assert.equal(reply.kind,kind);return reply.value;};
const counter=text=>{assert.equal(typeof text,'string');assert.match(text,/^(0|[1-9][0-9]*)$/);return BigInt(text);};
try {
  // An unnegotiated profile cannot silently expose a richer inspector response.
  process.stderr.write('phase: limited observer launch\n');
  const limited=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,transport:'stdio',role:'observer',initialize:{...initialize,capabilities:['history/query','policy/read','routing/status'],required_capabilities:['history/query','policy/read','routing/status']}});owned.push(limited);
  const query={scope:input.scope,task:null,selector:null,text:null,artifact:null,expand_compacted:false,limit:7,cursor:null};
  process.stderr.write('phase: limited observer ready\n');
  await assert.rejects(limited.call('history/query',query));
  await assert.rejects(limited.call('policy/read',{scope:input.scope,task:input.task,section:'denials',limit:1,cursor:null}));
  await assert.rejects(limited.call('routing/status',{scope:input.scope,task:input.task,section:'catalog',limit:1,cursor:null}));
  await limited.dispose();
  process.stderr.write('phase: full observer launch\n');
  const client=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,transport:'stdio',role:'observer',initialize});owned.push(client);
  process.stderr.write('phase: full observer ready\n');
  assert.equal(client.role,'observer');assert.deepEqual(client.scope,input.scope);
  const rows=[];let cursor=null,firstCursor;let pages=0;
  do {
    const page=value(await client.call('history/query',{...query,cursor}),'history');
    assert.equal(page.source_watermark,input.watermark);assert.equal(page.observed_watermark,input.watermark);assert.equal(page.newer_events,'0');counter(page.source_watermark);
    assert(page.rows.length<=7);assert.equal(page.complete,page.next_cursor===null);
    for(const row of page.rows){counter(row.sequence);counter(row.timestamp_ms);assert(!Object.hasOwn(row,'data'));assert(!Object.hasOwn(row,'record'));assert(!Object.hasOwn(row,'content'));rows.push(row);}
    cursor=page.next_cursor;firstCursor??=cursor;assert(++pages<100);
  }while(cursor);
  assert(pages>1);
  assert.deepEqual(rows.map(({id,session,task,sequence,timestamp_ms,visibility,artifacts})=>({id,session,task,sequence,timestamp_ms,visibility,artifacts})),input.history);
  const metadata=rows.find(row=>row.timestamp_ms==='9007199254740993');assert(metadata);assert.equal(metadata.metadata.provider,'offline-provider');assert.equal(metadata.metadata.model,'offline-model');assert.deepEqual(metadata.metadata.paths,['src/parser.rs']);
  assert(rows.some(row=>row.task===null),'session-level rows survive session inspection');
  const taskPage=value(await client.call('history/query',{...query,task:input.task}),'history');assert(taskPage.rows.every(row=>row.task===input.task));
  const backlinks=value(await client.call('history/query',{...query,artifact:input.artifact}),'history');assert(backlinks.rows.length>0);assert(backlinks.rows.every(row=>row.artifacts.some(artifact=>artifact.id===input.artifact)));
  await assert.rejects(client.call('history/query',{...query,task:input.task,cursor:firstCursor}));
  await assert.rejects(client.call('history/query',{...query,task:'missing-task'}));
  await assert.rejects(client.call('history/query',{...query,scope:{...input.scope,workspace:'foreign-workspace'}}));
  const request={scope:input.scope,task:input.task,claim:input.claim,limit:7,cursor:null};
  const versions=[];cursor=null;pages=0;let memoryCursor;let upper;
  do {
    const page=value(await client.call('memory/history',{...request,cursor}),'memory_history');
    assert.equal(page.watermark,input.watermark);counter(page.at);upper??=page.at;assert.equal(page.at,upper);
    assert(page.versions.length<=7);assert.equal(page.complete,page.next_cursor===null);
    for(const version of page.versions){assert.equal(version.finding.claim,input.claim);assert.equal(version.finding.content,`Parser revision ${versions.length}`);assert(version.origins.includes(input.origin));assert(version.finding.evidence.some(e=>e.artifact===input.artifact));assert.equal(version.content_truncated,false);versions.push(version.finding.version);}
    cursor=page.next_cursor;memoryCursor??=cursor;assert(++pages<10);
  }while(cursor);
  assert.equal(versions.length,33);assert.equal(new Set(versions).size,33);assert.deepEqual(versions,input.versions);assert(pages>=5);
  await assert.rejects(client.call('memory/history',{...request,limit:6,cursor:memoryCursor}));
  await assert.rejects(client.call('memory/history',{...request,task:'missing-task'}));
  await assert.rejects(client.call('memory/history',{...request,scope:{...input.scope,session:'foreign-session'}}));
  await assert.rejects(client.call('memory/history',{...request,claim:'missing-claim'}),error=>error.classification?.applicationCode==='STORE_UNAVAILABLE');

  const inspection=input.inspection;
  const policyRequest={scope:input.scope,task:input.task,section:'denials',limit:1,cursor:null};
  const policyRows={denials:[],grants:[]};let policyCursor;
  for(const section of ['denials','grants']){
    cursor=null;let count=0;
    do{
      const page=value(await client.call('policy/read',{...policyRequest,section,cursor}),'policy');
      assert.equal(page.watermark,input.watermark);assert.equal(page.authority_revision,inspection.authority);assert.equal(page.binding_revision,'0');
      assert.equal(page.persisted.revision,String(inspection.policy.revision));assert.equal(page.persisted.mode,inspection.policy.mode);assert.equal(page.persisted.timeout_ceiling_ms,String(inspection.policy.timeout_ceiling_ms));assert.equal(page.persisted.output_ceiling_bytes,String(inspection.policy.output_ceiling_bytes));
      assert.deepEqual(page.effective,{state:'unavailable',reason:'binding_unavailable'});assert.equal(page.assessment,'operation_not_evaluated');assert.equal(page.grant_visibility,'task_and_inherited_only');
      counter(page.observed_at_ms);assert(page.rows.length<=1);assert.equal(page.complete,page.next_cursor===null);policyRows[section].push(...page.rows);
      cursor=page.next_cursor;if(section==='denials')policyCursor??=cursor;assert(++count<10);
    }while(cursor);
  }
  assert.deepEqual(policyRows.denials.map(row=>row.value.id.text).sort(),inspection.policy.denials.map(row=>row.id).sort());
  for(const row of policyRows.denials){assert.equal(row.kind,'denial');assert.equal(row.value.layer,'canonical');assert.equal(row.value.reason.text,'private-denial-reason-marker');assert.deepEqual(row.value.effects,['publish']);assert.equal(row.value.path_count,'1');}
  assert.deepEqual(policyRows.grants.map(row=>row.value.id).sort(),['native-grant-0','native-grant-1']);
  for(const row of policyRows.grants){assert.equal(row.kind,'grant');assert.equal(row.value.actor,inspection.actor);assert.equal(row.value.scope.kind,'task');assert.equal(row.value.scope.task,input.task);assert.equal(row.value.authority_revision,inspection.authority);assert.equal(row.value.expires_at_ms,'18446744073709551615');assert.equal(row.value.target.kind,'exact');assert.equal(row.value.current_matches.not_revoked,row.value.id==='native-grant-0');assert.equal(row.value.current_matches.authority,true);assert.equal(row.value.current_matches.policy,true);assert.equal(row.value.current_matches.unexpired,true);}
  assert(!JSON.stringify(policyRows).includes('native-grant-2'),'workspace grant IDs are not disclosed through task inspection');
  await assert.rejects(client.call('policy/read',{...policyRequest,section:'grants',cursor:policyCursor}));
  await assert.rejects(client.call('policy/read',{...policyRequest,task:'missing-task'}));
  await assert.rejects(client.call('policy/read',{...policyRequest,scope:{...input.scope,session:'foreign-session'}}));
  const routingRequest={scope:input.scope,task:input.task,section:'policy_entries',limit:1,cursor:null};
  const routingRows={policy_entries:[],catalog:[]};let routingCursor;
  for(const section of ['policy_entries','catalog']){
    cursor=null;let count=0;
    do{
      const page=value(await client.call('routing/status',{...routingRequest,section,cursor}),'routing_status');
      assert.equal(page.watermark,input.watermark);assert.equal(page.authority_revision,inspection.authority);assert.equal(page.persisted.revision,inspection.routing_revision);assert.equal(page.persisted.actor,inspection.actor);assert.equal(page.persisted.policy.id,inspection.routing.id);assert.equal(page.persisted.policy.profile,'low');assert.equal(page.persisted.policy.quality_floor_bps,7000);assert.equal(page.persisted.policy.maximum_evidence_age_ms,'9007199254740993');assert.equal(page.persisted.policy.input_tokens,'654');assert.equal(page.persisted.policy.output_tokens,'321');assert.equal(page.persisted.policy.allowed_models_count,'2');
      assert.deepEqual(page.effective,{state:'unavailable',reason:'host_unconfigured'});assert.equal(page.optimizer.report_capture,'explicit_mutation_required');assert.equal(page.optimizer.preferences,'workspace_authority_required');assert.equal(page.optimizer.preview,'host_ceilings_required');assert.equal(page.optimizer.remote_advice,'disabled');
      assert.equal(page.registry.state,'observed');assert.equal(page.registry.revision,inspection.catalog_revision);assert.equal(page.registry.catalog_id,inspection.catalog.id);assert.equal(page.registry.source_task,input.task);assert.equal(page.registry.source_availability,'retained_metadata_only');
      assert(!JSON.stringify(page).includes('private-catalog-source-marker'));assert(!JSON.stringify(page).includes('private-raw-catalog-marker'));assert(!Object.hasOwn(page,'interview'));
      assert(page.rows.length<=1);assert.equal(page.complete,page.next_cursor===null);routingRows[section].push(...page.rows);cursor=page.next_cursor;if(section==='catalog')routingCursor??=cursor;assert(++count<10);
    }while(cursor);
  }
  assert.deepEqual(routingRows.policy_entries.map(row=>row.entry).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))),[{kind:'allowed_model',value:'native-model-a'},{kind:'allowed_model',value:'native-model-b'},{kind:'allowed_endpoint',value:'native-endpoint'},{kind:'allowed_group',value:'low'}].sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))));
  assert(routingRows.policy_entries.every(row=>row.kind==='policy_entry'&&row.representation==='persisted'));
  assert.deepEqual(routingRows.catalog.map(row=>({model:row.model,endpoint:row.endpoint,availability:row.availability})),inspection.catalog.entries.map(row=>({...row.identity,availability:row.availability})));
  assert(routingRows.catalog.every(row=>row.kind==='candidate'&&row.provenance_count==='1'&&row.compatibility_count==='0'&&row.role_evidence_count==='0'));
  await assert.rejects(client.call('routing/status',{...routingRequest,cursor:routingCursor}));
  await assert.rejects(client.call('routing/status',{...routingRequest,task:'missing-task'}));
  await assert.rejects(client.call('routing/status',{...routingRequest,scope:{...input.scope,workspace:'foreign-workspace'}}));

  process.stdout.write(JSON.stringify({ok:true,policyDenials:policyRows.denials.length,taskGrants:policyRows.grants.length,routingCandidates:routingRows.catalog.length,historyRows:rows.length,memoryVersions:versions.length,backends:'qualified by native caller'}));
} finally {for(const client of owned.reverse())await client.dispose();}
