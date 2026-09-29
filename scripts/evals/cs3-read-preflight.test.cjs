// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const helper = require('./cs3-read-preflight.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture() {
  const scope={workspace:'workspace',session:'session',task:'task'};
  const attempts = [0,1,2,3].map(i=>({id:'attempt-'+i,scope,root:scope.task,phase:'settled',uncertain:false,previous:null,role:'main',charged:'1',provider_request:'response-'+i,request_digest:'digest-'+i}));
  const evidence = { costs:[{gaps:[],items:[{collection:'ledger',visibility:'available',record:{currency:'USD',cap:'600000',settled:'4',active:'0',unresolved:'0',overrun:false}},...attempts.map(record=>({collection:'attempt',visibility:'available',record})),...attempts.map(a=>({collection:'settlement',visibility:'available',record:{attempt:a.id,applied:true,observation:{final_usage:{}}}}))]}],verification:[{gaps:[],items:[{collection:'verification',record:{outputs:['whole'],outstanding_issues:[],unresolved_effects:[]}}]}] };
  evidence.costs[0].items[0].record.scope=scope;evidence.verification[0].items[0].record.scope=scope;evidence.verification[0].items[0].record.outputs=['whole','range'];evidence.verification[0].items[0].record.id='verification';
  const artifacts = [], add=(id,schema,value,channel='evidence')=>artifacts.push({item:{id,record:{spec:{schema,channel,scope,source:channel==='response'?'retained-codex-attempt:attempt-'+id.split('-').at(-1):'synthetic'}}},bytes:Buffer.from(typeof value==='string'?value:JSON.stringify(value))});
  const outputs = [
    [{type:'function_call',name:'vcp_read',arguments:JSON.stringify(helper.WHOLE)}],
    [{type:'function_call',name:'vcp_read',arguments:JSON.stringify(helper.RANGE)}],
    [{type:'function_call',name:'vcp_verify',arguments:JSON.stringify({citations:['whole','range']})}],
    [{type:'message',content:[{type:'output_text',text:JSON.stringify(helper.ANSWER)}]}]
  ];
  outputs.forEach((output,i)=>{if(output[0].type==='function_call')output[0].call_id='call-'+i;add('response-'+i,'responses','data: '+JSON.stringify({type:'response.completed',response:{id:'response-'+i,model:'deepseek/deepseek-v3.2',status:'completed',output}})+'\n\n','response');});
  add('whole','vcp-tool-result-v1',{text:helper.CONTENT,complete:true});
  add('range','vcp-tool-result-v1',{text:'CS3_READ_MIDDLE_A25E\n',returned_range:{start_line:2,end_line:2}});
  attempts.forEach((a,i)=>{const input=[];for(let n=0;n<i&&n<3;n++){input.push({...outputs[n][0]});const body=n===0?{evidence:'whole',result:{text:helper.CONTENT,complete:true}}:n===1?{evidence:'range',result:{text:'CS3_READ_MIDDLE_A25E\n',returned_range:{start_line:2,end_line:2}}}:{verification:evidence.verification[0].items[0].record};input.push({type:'function_call_output',call_id:'call-'+n,output:JSON.stringify(body)});}const request=JSON.stringify({model:'deepseek/deepseek-v3.2',input});a.request_digest=sha(request);add('request-'+i,'responses-request/1',request,'request_body');add('context-'+i,'context-manifest/1',{request_sha256:a.request_digest,included:[]});});
  return {evidence,artifacts,stdout:JSON.stringify({type:'accepted',scope})+'\n'+JSON.stringify({type:'result',scope,conditions:{completed:true}})+'\n',exit:{status:0,error:null}};
}
const check=f=>helper.oracle(f.evidence,f.artifacts,f.stdout,f.exit);
function changeCall(f,index,mutate) { const a=f.artifacts[index],event=JSON.parse(a.bytes.toString().slice(6)); mutate(event.response.output[0]); a.bytes=Buffer.from('data: '+JSON.stringify(event)+'\n\n'); }
test('exact null/numeric native read proof, no skill contexts and known ledger pass',()=>{assert.deepEqual(check(fixture()),{status:'passed',actual_cost_micros:4,observed_attempts:4,scope:{workspace:'workspace',session:'session',task:'task'},reads:2,selected_skills:0,preserved:true});});
test('malformed, quoted, missing, extra and swapped read arguments reject without coercion',()=>{
  for(const value of ['', '2', 2.5, -1]){const f=fixture();changeCall(f,1,c=>{const a=JSON.parse(c.arguments);a.start_line=value;c.arguments=JSON.stringify(a);});assert.throws(()=>check(f),/call sequence/);}
  for(const mutate of [a=>delete a.start_line,a=>a.max_bytes='4096',a=>a.extra=true,a=>a.path='other.txt']){const f=fixture();changeCall(f,0,c=>{const a=JSON.parse(c.arguments);mutate(a);c.arguments=JSON.stringify(a);});assert.throws(()=>check(f),/call sequence/);}
  const f=fixture();changeCall(f,0,c=>c.arguments='{malformed');assert.throws(()=>check(f),SyntaxError);
});
test('model claimed markers cannot replace independently retained read results',()=>{
  for(const id of ['whole','range']){const f=fixture();f.artifacts=f.artifacts.filter(a=>a.item.id!==id);assert.throws(()=>check(f),/canonical read results/);}
  const f=fixture();changeCall(f,2,c=>c.arguments=JSON.stringify({citations:['status.txt']}));assert.throws(()=>check(f),/canonical read results/);
});
test('missing verification, wrong final JSON and incomplete native execution reject',()=>{
  let f=fixture();f.evidence.verification[0].items[0].record.outstanding_issues=['unresolved'];assert.throws(()=>check(f),/verification/);
  f=fixture();f.exit.status=1;assert.throws(()=>check(f),/did not complete/);
  f=fixture();changeCall(f,3,c=>c.content[0].text=JSON.stringify({whole:[],range:'fabricated'}));assert.throws(()=>check(f),/marker answer/);
});
test('unknown accounting, missing response and injected skill context reject',()=>{
  let f=fixture();f.evidence.costs[0].items[0].record.unresolved='1';assert.throws(()=>check(f),/unknown liability/);
  f=fixture();f.artifacts=f.artifacts.filter(a=>a.item.id!=='response-1');assert.throws(()=>check(f),/coverage/);
  f=fixture();const a=f.artifacts.find(a=>a.item.id==='context-2'),m=JSON.parse(a.bytes);m.included=[{kind:'skill'}];a.bytes=Buffer.from(JSON.stringify(m));assert.throws(()=>check(f),/No-skill/);
});
test('absolute hash-bound input rejects changed bytes and unbound summaries',t=>{
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'cs3-read-bound-'));t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  const file=path.join(dir,'receipt.json');fs.writeFileSync(file,'{}');const ref={path:file,sha256:sha('{}')};assert.equal(helper.bound(ref).toString(),'{}');fs.writeFileSync(file,'{"status":"passed"}');assert.throws(()=>helper.bound(ref),/changed/);assert.throws(()=>helper.bound({path:'relative',sha256:'x'}),/hash-bound/);
  assert.throws(()=>helper.validateQualification({path:file,sha256:sha(fs.readFileSync(file))},{profile:{path:file,sha256:sha(fs.readFileSync(file))},catalog:{path:file,sha256:sha(fs.readFileSync(file))}}),/qualification identity/);
});
test('helper source closure is deterministic and excludes the mutable comparison orchestrator',()=>{
  const identity=helper.sources();assert.deepEqual(identity,helper.sources());assert(identity['scripts/evals/developer-runner.cjs']);assert(identity['scripts/evals/p6-task-quality.cjs']);assert(!identity['scripts/evals/cs3-comparison.cjs']);assert(!identity['scripts/evals/cs3-read-preflight.test.cjs']);
});
test('provider USD observation rounds upward using decimal arithmetic, including exponent notation',()=>{assert.equal(helper.dollarMicros(0.000100468),101);assert.equal(helper.dollarMicros(0.000001),1);assert.equal(helper.dollarMicros(1e-7),1);assert.equal(helper.dollarMicros(0),0);assert.throws(()=>helper.dollarMicros(-1));assert.throws(()=>helper.dollarMicros('0.1'));});
test('only exact fixed-provider explanatory routing gap is allowed outside privacy gaps',()=>{const gap={reason:'no automatic routing decision retained for this task; fixed provider or no admitted routed request',requested_model:'attempt.quote.price.model',served_model:'captured response bytes when observed; never inferred from requested model',visibility:'unavailable'};assert(helper.gapAllowed('routing',gap));assert(!helper.gapAllowed('tools',gap));assert(!helper.gapAllowed('routing',{...gap,reason:'missing capture'}));assert(!helper.gapAllowed('routing',{...gap,extra:true}));});
function qualification(t, mutate = ()=>{}) {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'cs3-read-qualification-'));t.after(()=>fs.rmSync(dir,{recursive:true,force:true}));
  const put=(name,value)=>{const file=path.join(dir,name);fs.writeFileSync(file,typeof value==='string'?value:JSON.stringify(value));return {path:file,sha256:sha(fs.readFileSync(file))};};
  const model='deepseek/deepseek-v3.2',endpoint='deepinfra/fp4',revision=model+'-20251201';
  const selected={tag:endpoint,model_id:model,provider_name:'DeepInfra',name:'DeepInfra | '+revision,status:0,context_length:163840,max_completion_tokens:16384,supported_parameters:['tools','tool_choice','max_tokens'],pricing:{prompt:'0.00000026',completion:'0.00000038'}};
  const catalog=put('catalog.json',{data:{id:model,endpoints:[selected]}}),binary=put('probe.exe','synthetic only');
  const probe={model,endpoint,cap_usd:'0.250000',max_output_tokens:2048,catalog:catalog.path,catalog_sha256:catalog.sha256,observed_at:'1000',valid_until:'86401000'};
  const probeSpec=put('spec.json',probe),candidate={raw_sha256:catalog.sha256,context:'163840',max_input:'163840',max_output:'16384',price:{model,provider:endpoint,rates:{input:{micros:'260000',per_units:'1000000'}}}};
  const report={schema:'p6-provider-conformance/1',status:'observed',responses_text_tools:true,candidate,actual_cost_micros:'2',ledger:{currency:'USD',cap:'250000',active:'0',unresolved:'0',overrun:false,settled:'2'},responses:[0,1].map(i=>({response_id:'response-'+i,status:'Completed',served_model:model,served_provider:null,usage:{cost:{currency:'USD',micros:'1'}},calls:i?[]:[{name:'vcp_conformance_echo',arguments:{marker:'VCP_CONFORMANCE_☃'}}],completed_messages:i?{answer:'VCP_CONFORMANCE_OK'}:{}}))};
  const generations=[0,1].map(i=>({data:{id:'response-'+i,model:revision,cancelled:false,streamed:true,is_byok:false,provider_name:'DeepInfra',total_cost:0.000001,provider_responses:[{status:200,is_byok:false,provider_name:'DeepInfra',model_permaslug:revision,endpoint_id:'observed-endpoint'}]}}));
  mutate({report,generations,selected});
  const generationRefs=generations.map((g,i)=>put('generation-'+i+'.json',g));
  const input={probe_spec:probeSpec,report:put('report.json',report),generations:generationRefs,catalog,observed_at:'2000',valid_until:'86400000'};
  const sources=put('sources.json',input),qualified={schema:'p6-provider-qualification/1',authorized_sources_sha256:sources.sha256,attribution:generations.map((g,i)=>({method:'generation-single-attempt-exact-catalog-model-provider/2',generation_sha256:generationRefs[i].sha256,response_id:'response-'+i,requested_model:model,catalog_endpoint:endpoint,provider_name:'DeepInfra',observed_endpoint_id:'observed-endpoint',observed_model_revision:revision}))};
  const snapshot={context:candidate.context,max_input:candidate.max_input,max_output:candidate.max_output,observed_at:input.observed_at,valid_until:input.valid_until,raw_sha256:catalog.sha256,compatibility:{id:'p6-generation-qualified/'+sources.sha256,model,endpoint,responses_text_tools:true,provider_preferences_qualified:true,deny_data_collection:true,require_zdr:false},price:{...candidate.price,currency:'USD',valid_until:input.valid_until}};
  const root=path.resolve(__dirname,'../..'),sourceNames={binary:'src/crates/vcp-cli/src/bin/vcp-provider-conformance.rs',lease:'src/crates/vcp-lifecycle/src/foundation/conformance.rs',settlement:'src/crates/vcp-lifecycle/src/foundation/worker/conformance.rs',catalog:'src/crates/vcp-models/src/catalog.rs'};
  const claim={binary_sha256:binary.sha256,spec:probe,spec_sha256:probeSpec.sha256,source_sha256:Object.fromEntries(Object.entries(sourceNames).map(([key,file])=>[key,sha(fs.readFileSync(path.join(root,file)))]))};
  const profile=put('profile.json',{provider:snapshot}),wrapper={schema:'cs3-successor-provider-qualification/1',model,endpoint,profile,catalog,binary,sources,probe_claim:put('claim.json',claim),qualification_claim:put('qualified.json',qualified),snapshot:put('snapshot.json',snapshot)};
  return {ref:put('wrapper.json',wrapper),spec:{profile,catalog},put,wrapper,claim};
}
test('qualification rederives source, catalog, generation attribution and both exact charged responses',t=>{const f=qualification(t);assert.deepEqual(helper.validateQualification(f.ref,f.spec),{status:'passed',actual_cost_micros:2,observed_attempts:2,model:'deepseek/deepseek-v3.2',endpoint:'deepinfra/fp4'});});
test('qualification rejects internally rehashed missing/unknown charges, bad echo and wrong served endpoint',t=>{
  for(const mutate of [f=>f.report.ledger.unresolved='1',f=>f.report.actual_cost_micros='0',f=>f.generations[0].data.total_cost=0,f=>f.generations[0].data.provider_responses.push(f.generations[0].data.provider_responses[0]),f=>f.generations[1].data.provider_responses[0].endpoint_id='other',f=>f.report.responses[0].calls[0].arguments.marker='fake',f=>f.report.responses[1].served_model='wrong']){const f=qualification(t,mutate);assert.throws(()=>helper.validateQualification(f.ref,f.spec));}
});
test('qualification rejects recomputed wrapper over stale embedded source hashes',t=>{const f=qualification(t);f.claim.source_sha256.lease='0'.repeat(64);f.wrapper.probe_claim=f.put('claim.json',f.claim);f.ref=f.put('wrapper.json',f.wrapper);assert.throws(()=>helper.validateQualification(f.ref,f.spec),/source closure/);});
test('UUID enumeration order is irrelevant but actual tool continuation is mandatory',()=>{const f=fixture();f.artifacts.reverse();assert.equal(check(f).status,'passed');const req=f.artifacts.find(a=>a.item.id==='request-2');req.bytes=Buffer.from(JSON.stringify({model:'deepseek/deepseek-v3.2',input:[]}));assert.throws(()=>check(f),/captured dispatched request/);});
test('foreign task accounting, final frame or response provenance cannot qualify',()=>{
  let f=fixture();f.evidence.costs[0].items[0].record.scope={workspace:'workspace',session:'session',task:'foreign'};assert.throws(()=>check(f),/another task/);
  f=fixture();f.evidence.costs[0].items[1].record.root='foreign';assert.throws(()=>check(f),/another task/);
  f=fixture();const frames=f.stdout.trim().split('\n').map(JSON.parse);frames[1].scope.task='foreign';f.stdout=frames.map(x=>JSON.stringify(x)).join('\n');assert.throws(()=>check(f),/native scope/);
  f=fixture();f.artifacts[0].item.record.spec.source='retained-codex-attempt:foreign';assert.throws(()=>check(f),/foreign-task/);
  f=fixture();f.artifacts[0].item.record.spec.scope={workspace:'workspace',session:'session',task:'foreign'};assert.throws(()=>check(f),/another task/);
});
test('a satisfied but different canonical verification cannot stand in for the actual final continuation',()=>{const f=fixture();f.evidence.verification[0].items[0].record.id='different-verification';assert.throws(()=>check(f),/actual satisfied verification/);});
function historicalFailure(){
  const scope={workspace:'workspace',session:'session',task:'failed-task'};
  const attempts=[{id:'a',phase:'settled',charged:'656',provider_request:'response-a'},{id:'b',phase:'settled',charged:'592',provider_request:'response-b'},{id:'c',phase:'reconciliation_pending',charged:'0',provider_request:null,uncertain:'retained response did not complete'}].map(a=>({...a,scope,root:scope.task,role:'main',previous:null,reservation:'reservation-'+a.id}));
  const ledger={currency:'USD',cap:'600000',active:'0',unresolved:'129576',settled:'1248',overrun:false,scope};
  const reservations=attempts.map(a=>({id:a.reservation,attempt:a.id,phase:a.phase,charged:a.charged,scope,root:scope.task,liability:a.phase==='settled'?'0':'129576'}));
  const settlements=attempts.slice(0,2).map(a=>({attempt:a.id,applied:true,total:a.charged,scope,observation:{final_usage:true,provider_request:a.provider_request,scope}}));
  const costs=[{gaps:[],items:[{collection:'ledger',record:ledger},...attempts.map(record=>({collection:'attempt',record})),...reservations.map(record=>({collection:'reservation',record})),...settlements.map(record=>({collection:'settlement',record}))].map(i=>({...i,visibility:'available'}))}];
  const bytes=Buffer.from(JSON.stringify({error:{code:429,metadata:{provider_name:'DeepInfra',is_byok:false,provider_error_code:'engine_overloaded',limit_source:'upstream_provider_shared_pool'}}}));
  const descriptor={state:'aborted',length:String(bytes.length),sha256:sha(bytes),spec:{id:'aborted-response',source:'retained-codex-attempt:c',channel:'response',scope}};
  const output=[{type:'accepted',scope},{type:'event',event:{event:{data:{facts:[{collection:'artifact',value:descriptor}]}}}},{type:'result',scope,conditions:{completed:false,durably_paused:true}}];
  return {stdout:output.map(f=>JSON.stringify(f)).join('\n'),costs,exit:{status:7,error:null},terminal:[{gaps:[],items:[{artifact:'aborted-response',visibility:'available',range:{start:0,end:bytes.length},bytes:[...bytes]}]}]};
}
const priorCheck=f=>helper.priorFailure(f.stdout,f.costs,f.exit,f.terminal);
test('separate replacement conservatively debits exact historical failure without settling unknown cost',()=>{const f=historicalFailure(),result=priorCheck(f);assert.equal(result.status,'conservative_failed_preflight_preserved');assert.equal(result.conservative_debit_micros,600000);assert.equal(result.actual_cost_micros,null);assert.equal(result.known_settled_micros,1248);assert.equal(result.unresolved_micros,129576);assert.equal(result.observed_attempts,3);assert.equal(result.quality,'failed_never_upgraded');});
test('replacement rejects rewritten accounting, foreign scope, changed terminal bytes and fabricated success',()=>{
  for(const change of [f=>f.costs[0].items[0].record.active='1',f=>f.costs[0].items[0].record.unresolved='0',f=>f.costs[0].items[1].record.root='foreign',f=>f.costs[0].items[3].record.phase='settled',f=>f.terminal[0].items[0].bytes[0]=0,f=>f.exit.status=0]){const f=historicalFailure();change(f);assert.throws(()=>priorCheck(f));}
});
test('replacement rejects different failure type even with rehashed terminal descriptor',()=>{const f=historicalFailure(),bytes=Buffer.from(JSON.stringify({error:{code:400,metadata:{provider_name:'DeepInfra',is_byok:false,provider_error_code:'engine_overloaded',limit_source:'upstream_provider_shared_pool'}}})),out=f.stdout.split('\n').map(JSON.parse),descriptor=out[1].event.event.data.facts[0].value;descriptor.sha256=sha(bytes);descriptor.length=String(bytes.length);f.stdout=out.map(r=>JSON.stringify(r)).join('\n');f.terminal[0].items[0].bytes=[...bytes];f.terminal[0].items[0].range.end=bytes.length;assert.throws(()=>priorCheck(f),/observed exact upstream overload/);});
test('historical and fresh immutable CLI paths may differ only with the exact same installed bytes',()=>{const digest='d08ff1069d6700a8aebc7ba668b510ce98867dc6ec2f68b7fed079b34bc5312e';assert(helper.sameExecutableIdentity({path:'old/vcp.exe',sha256:digest},{path:'new/vcp.exe',sha256:digest}));assert(!helper.sameExecutableIdentity({sha256:digest},{sha256:'0'.repeat(64)}));assert(!helper.sameExecutableIdentity({sha256:'0'.repeat(64)},{sha256:'0'.repeat(64)}));});
