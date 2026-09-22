// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {spawnSync}=require('node:child_process');
const runner=require('../../../scripts/evals/delegation-live-runner.cjs');
const exploration=require('../../../scripts/evals/delegation-exploration.cjs');
const oracle=require('../../../scripts/evals/delegation-generation-oracle.cjs');
const fixture=path.resolve(__dirname,'../../evals/delegation/generation-v1');
function pinnedTestGit(){
 const searchPath=Object.entries(process.env).find(([key])=>key.toUpperCase()==='PATH')?.[1]||'';
 const candidates=[process.env.VCP_DELEGATION_TEST_GIT,...searchPath.split(path.delimiter).filter(Boolean).map(directory=>path.join(directory,process.platform==='win32'?'git.exe':'git')),'/usr/bin/git','/usr/local/bin/git'].filter(Boolean);
 const git=candidates.find(file=>fs.existsSync(file)&&fs.statSync(file).isFile());assert.ok(git,'A real, explicitly located Git executable is required for delegation Git contracts');return path.resolve(git);
}
function supportsPermissionRuntime(runtime){const help=spawnSync(runtime,['--help'],{env:{},encoding:'utf8',timeout:3000,windowsHide:true});return !help.error&&help.status===0&&help.stdout.includes('--allow-net');}
function writePassingCandidate(workspace){
 fs.writeFileSync(path.join(workspace,'src','cart.cjs'),`'use strict';\nconst {subtotalCents}=require('./cents.cjs');\nfunction quoteCart(items,options={}){if(!options||typeof options!=='object'||Array.isArray(options))throw TypeError();const bps=options.discountBps===undefined?0:options.discountBps;if(!Number.isInteger(bps)||bps<0||bps>10000)throw TypeError();const subtotal=BigInt(subtotalCents(items)),discount=(subtotal*BigInt(bps)+5000n)/10000n;return {subtotalCents:Number(subtotal),discountCents:Number(discount),totalCents:Number(subtotal-discount)};}module.exports={quoteCart};\n`);
 fs.writeFileSync(path.join(workspace,'src','cents.cjs'),`'use strict';\nfunction subtotalCents(items){if(!Array.isArray(items))throw TypeError();let subtotal=0n;for(const item of items){if(!item||!Number.isSafeInteger(item.unitCents)||item.unitCents<0||!Number.isSafeInteger(item.quantity)||item.quantity<0)throw TypeError();subtotal+=BigInt(item.unitCents)*BigInt(item.quantity);}if(subtotal>BigInt(Number.MAX_SAFE_INTEGER))throw TypeError();return Number(subtotal);}module.exports={subtotalCents};\n`);
}
const findings=()=>runner.rubric.defects.map(d=>({...d,kind:'defect',trigger:'The specified boundary input',consequence:'Returned value violates documented contract',uncertainty:'Demonstrated by comparison; caller assumptions are bounded',evidence:[d.path+':1','base/'+d.path+':1']}));
test('independent review rubric detects introduced and pre-existing bugs, rejects benign false positives',()=>{
 assert.equal(runner.gradeReview({findings:findings()}).pass,true);
 assert.equal(runner.gradeReview({findings:findings().slice(0,1)}).pass,false);
 const wrong=findings();wrong[1].introduced_by_change='introduced';assert.equal(runner.gradeReview({findings:wrong}).pass,false);
 const falsePositive={...findings()[0],path:'receipt.cjs'};
 assert.equal(runner.gradeReview({findings:[...findings(),falsePositive]}).false_positives,1);
 const ungrounded=findings();ungrounded[0].evidence=['shipping.cjs:1'];assert.equal(runner.gradeReview({findings:ungrounded}).pass,false);
 const duplicate=runner.gradeReview({findings:[...findings(),findings()[0]]});assert.equal(duplicate.pass,false);assert.equal(duplicate.duplicate_findings,1);assert.equal(duplicate.false_positives,0);
 assert.equal(runner.gradeReview({findings:[...findings(),null]}).pass,false);
 const alternate=findings();alternate[1].reproduction={arguments:[15,10],expected:2,actual:1};assert.equal(runner.gradeReview({findings:alternate}).pass,true);
 alternate[1].reproduction.actual=0;assert.equal(runner.gradeReview({findings:alternate}).pass,false);
 alternate[1].reproduction={arguments:[20,10],expected:2,actual:2};assert.equal(runner.gradeReview({findings:alternate}).pass,false);
});
test('review reproductions accept bounded explicit integer annotations without inferring prose',()=>{
 const historical=answer=>runner.gradeReview(answer,runner.rubric,null,{historical_annotations:true});
 for(const delimiter of ['—','–','-']){
  const answer={findings:findings()};
  answer.findings[0].reproduction={arguments:[100],expected:`0 ${delimiter} the documented threshold is free`,actual:`5 ${delimiter} the current comparison excludes equality`};
  answer.findings[1].reproduction={arguments:[150,5],expected:`8 ${delimiter} the exact half rounds up`,actual:`7 ${delimiter} floor rounds down`};
  assert.equal(historical(answer).pass,true);assert.equal(runner.gradeReview(answer).pass,false);
 }
 for(const invalid of ['0','zero — expected','0 or 5','0/5','0 —','0 —   ','0— explanation','00 — explanation','+0 — explanation','-0 — explanation','0.0 — explanation','0e0 — explanation','9007199254740992 — unsafe','0 — first\nsecond','0 — explanation\n','0 — '+'x'.repeat(2048)]){
  const answer={findings:findings()};answer.findings[0].reproduction={arguments:[100],expected:invalid,actual:5};
  assert.equal(historical(answer).pass,false,invalid);
 }
 const answer={findings:findings()};answer.findings[0].reproduction={arguments:[100],expected:'1 — wrong value',actual:'5 — current value'};assert.equal(historical(answer).pass,false);
 answer.findings[0].reproduction={arguments:[100],expected:'0 — correct expectation',actual:'0 — falsely equal result'};assert.equal(historical(answer).pass,false);
 answer.findings[0].reproduction={arguments:['100'],expected:'0 — expected',actual:'5 — actual'};assert.equal(historical(answer).pass,false);
 answer.findings[0].reproduction={arguments:[101],expected:'0 — expected',actual:'0 — actual'};assert.equal(historical(answer).pass,false);
 answer.findings[0].reproduction={arguments:[100],expected:'0 — expected',actual:'5 — actual'};answer.findings[0].introduced_by_change='unknown';assert.equal(historical(answer).pass,false);
 answer.findings[0].introduced_by_change='introduced';answer.findings[0].evidence=['shipping.cjs:1'];assert.equal(historical(answer).pass,false);
});
function reviewEvidenceFixture(child=false){
 const sha=bytes=>require('node:crypto').createHash('sha256').update(bytes).digest('hex');
 const scope={workspace:'workspace',session:'session',task:child?'child':'root'},sourceRoot=child?'isolated':'workspace';
 const records={task:{collection:'task',id:scope.task,value:{scope,root:'root',parent:child?'root':null}},workspace:{collection:'workspace',id:'workspace',value:{binding:{revision:'0'}}}};
 if(child)records.graph={collection:'projection',id:'root',value:{document_type:'vcp_task_graph_v1',children:{child:{parent:'root',isolated_root:'isolated',binding:'0'}}}};
 const capture=(id,text,channel,schema)=>{const digest=sha(text),length=String(Buffer.byteLength(text));records[id]={collection:'artifact',id,value:{state:'complete',length,retained:[{start:'0',end:length}],sha256:digest,spec:{id,scope:{...scope},channel,schema}}};return {artifact:id,sha256:digest,text};};
 const answer={findings:findings()},files=Object.fromEntries(Object.entries(runner.sources).map(([name,text])=>[name,sha(text)]));
 const evidence=Object.entries(runner.sources).map(([name,text],index)=>capture('evidence-'+index,JSON.stringify({complete:true,text,version:{root:sourceRoot,binding:'0',path:name,bytes:String(Buffer.byteLength(text)),sha256:files[name]}}),'evidence','vcp-tool-result-v1'));
 for(const finding of answer.findings)finding.evidence=[evidence.find(item=>JSON.parse(item.text).version.path==='base/'+finding.path).artifact];
 const transcript=capture('transcript',JSON.stringify(answer),'child_transcript','retained-full-output/1');
 return {answer,state:{records},transcript,evidence,files,task:scope.task,capture};
}
const evidenceContext=f=>runner.reviewEvidence(f.state,f.transcript,f.evidence,f.files,f.task);
test('canonical evidence IDs resolve for the exact baseline or isolated child source revision',()=>{
 for(const child of [false,true]){
  const f=reviewEvidenceFixture(child),quality=runner.gradeReview(f.answer,runner.rubric,evidenceContext(f));
  assert.equal(quality.pass,true);assert.equal(quality.seeded_detected,2);assert.equal(quality.false_positives,0);
  f.answer.findings[0].introduced_by_change='unknown';const unqualified=runner.gradeReview(f.answer,runner.rubric,evidenceContext(f));
  assert.equal(unqualified.pass,false);assert.equal(unqualified.seeded_found,1);assert.equal(unqualified.seeded_detected,2);assert.equal(unqualified.unqualified_findings,1);assert.equal(unqualified.false_positives,0);
 }
});
test('review evidence rejects forged, foreign, missing, partial and stale canonical citations',()=>{
 const mutations=[
  f=>f.answer.findings[0].evidence=['unknown-evidence'],
  f=>f.evidence.splice(2,1),
  f=>f.state.records['evidence-2'].value.spec.scope.task='foreign',
  f=>f.state.records['evidence-2'].value.spec.scope.session='foreign',
  f=>f.state.records['evidence-2'].value.spec.scope.workspace='foreign',
  f=>f.state.records['evidence-2'].value.state='aborted',
  f=>f.state.records['evidence-2'].value.retained=[{start:'1',end:f.state.records['evidence-2'].value.length}],
  f=>f.state.records['evidence-2'].value.sha256='0'.repeat(64),
  f=>f.evidence[2].text+=' ',
  f=>f.evidence.push({...f.evidence[2]}),
  f=>f.files['base/shipping.cjs']='0'.repeat(64),
  ...[v=>v.version.root='foreign',v=>v.version.binding='1',v=>v.version.sha256='0'.repeat(64),v=>v.version.bytes='0',v=>v.complete=false,v=>v.text='unrelated',v=>v.returned_range={start_line:2,end_line:2}].map(mutate=>f=>{const value=JSON.parse(f.evidence[2].text);mutate(value);f.evidence[2]=f.capture('evidence-2',JSON.stringify(value),'evidence','vcp-tool-result-v1');}),
 ];
 for(const mutate of mutations){const f=reviewEvidenceFixture();mutate(f);const quality=runner.gradeReview(f.answer,runner.rubric,evidenceContext(f));assert.equal(quality.pass,false,String(mutate));assert.equal(quality.seeded_found,1);assert.equal(quality.unqualified_findings,1);assert.equal(quality.false_positives,0);}
});
test('review transcript must belong to the canonical grading task and retain exact bytes',()=>{
 for(const mutate of [f=>f.transcript.text+=' ',f=>f.state.records.transcript.value.spec.scope.task='foreign',f=>f.state.records.transcript.value.spec.channel='evidence',f=>f.task='missing']){
  const f=reviewEvidenceFixture();mutate(f);assert.throws(()=>evidenceContext(f),/Canonical review/);
 }
});
test('literal review citations are exact bounded source references, never path substrings',()=>{
 for(const ref of ['unrelated/base/shipping.cjs:1','not base/shipping.cjs:1','base/shipping.cjs:99','base/shipping.cjs:1fake','../base/shipping.cjs:1','base/shipping.cjs:1-2','base/shipping.cjs:1-2, artifact UUID','base/shipping.cjs:1fake; artifact UUID','base/shipping.cjs:1/elsewhere']){
  const answer={findings:findings()};answer.findings[0].evidence=[ref];assert.equal(runner.gradeReview(answer).pass,false,ref);
 }
 const answer={findings:findings()};answer.findings[0].evidence=['base/shipping.cjs:1 uses the inclusive boundary'];assert.equal(runner.gradeReview(answer).pass,true);
 for(const delimiter of [',',';']){answer.findings[0].evidence=[`base/shipping.cjs:1${delimiter} artifact UUID — uses the inclusive boundary`];assert.equal(runner.gradeReview(answer).pass,true);}
 answer.findings[0].evidence=['base/shipping.cjs'];assert.equal(runner.gradeReview(answer).pass,true);
});
function state(){return {records:{ledger:{collection:'ledger',value:{currency:'USD',cap:'1000',active:'0',unresolved:'0',settled:'30',overrun:false}},root:{collection:'attempt',value:{id:'a',scope:{task:'root'},phase:'settled',role:'main',charged:'10'}},child:{collection:'attempt',value:{id:'b',scope:{task:'child'},phase:'settled',role:'child',charged:'20'}},sa:{collection:'settlement',value:{attempt:'a',applied:true,observation:{final_usage:true}}},sb:{collection:'settlement',value:{attempt:'b',applied:true,observation:{final_usage:true}}}}};}
test('root accounting includes retained child and rejects unknown, missing or foreign support cost',()=>{
 assert.equal(runner.accounting(state(),1000,'root','child').actual_cost_micros,30);
 for(const mutate of [s=>s.records.ledger.value.unresolved='1',s=>delete s.records.sb,s=>s.records.child.value.scope.task='foreign',s=>s.records.child.value.charged='1',s=>s.records.child.value.previous='retry']){
  const s=state();mutate(s);assert.throws(()=>runner.accounting(s,1000,'root','child'));
 }
});

test('exploration requires observed source ranges, complete active flow and exact failure locations',()=>{
 const f=reviewEvidenceFixture(true),sha=bytes=>require('node:crypto').createHash('sha256').update(bytes).digest('hex');
 const files=Object.fromEntries(Object.entries(exploration.sources).map(([name,text])=>[name,sha(text)]));
 const evidence=Object.entries(exploration.sources).map(([name,text],index)=>f.capture('explore-'+index,JSON.stringify({complete:true,text,version:{root:'isolated',binding:'0',path:name,bytes:String(Buffer.byteLength(text)),sha256:files[name]}}),'evidence','vcp-tool-result-v1'));
 const context=runner.reviewEvidence(f.state,f.transcript,evidence,files,f.task,exploration.sources);
 const row=(path,line=1)=>({path,line,evidence:[`${path}:${line}`]});
 const valid={summary:'Decode and authorize before storing an event and enqueuing one notification.',flow:exploration.rubric.flow.map((path,index)=>({...row(path,exploration.rubric.anchors[index]),start_line:exploration.rubric.anchors[index],end_line:exploration.rubric.anchors[index],reason:'Observed active dependency'})),errors:[{code:'INVALID',status:400,...row('src/decode.cjs',2)},{code:'DENIED',status:403,...row('src/policy.cjs',2)}],duplicate:{...row('src/submit.cjs',4),behavior:'Return original result before write or notify',result:'existing',writes:0,notifications:0},inactive:['legacy/entry.cjs','ui/status.cjs'].map(path=>({...row(path),reason:'Not reachable from the active entry',active:false})),checks:{executed:false,limitation:'Static inspection only; no processes run.'}};
 assert.equal(exploration.grade(valid,context).pass,true);
 const returned=structuredClone(valid);returned.flow.splice(5,0,{path:'src/submit.cjs',start_line:5,end_line:7,reason:'Return to caller to notify after storing',evidence:['src/submit.cjs:5-7']});assert.equal(exploration.grade(returned,context).pass,true);
 for(const mutate of [a=>a.flow[5].path='ui/status.cjs',a=>a.flow[5].end_line=9,a=>a.flow[5].evidence=['src/submit.cjs:1-4'],a=>a.flow.push(...Array(30).fill(a.flow[5]))]){const bad=structuredClone(returned);mutate(bad);assert.equal(exploration.grade(bad,context).pass,false,String(mutate));}
 const gap={...context,references:new Map([...context.references].filter(([,ref])=>ref.path!=='src/submit.cjs'))};
 gap.references.set('branch',{path:'src/submit.cjs',start:4,end:5});gap.references.set('return',{path:'src/submit.cjs',start:7,end:7});
 const unread=structuredClone(returned);unread.flow[5].evidence=['branch','return'];assert.equal(exploration.grade(unread,gap).pass,false,'Endpoint citations cannot establish the unread notification between them');
 for(const mutate of [a=>a.flow.pop(),a=>a.flow.reverse(),a=>a.flow[0].path='legacy/entry.cjs',a=>a.flow[0].end_line=99,a=>a.errors[0].status=403,a=>a.errors[0].path='ui/status.cjs',a=>a.errors.push(a.errors[0]),a=>a.duplicate.line=5,a=>a.inactive[0].path='src/entry.cjs',a=>a.inactive.pop(),a=>a.checks.limitation='',a=>a.checks.executed=true,a=>a.duplicate.result='new',a=>a.duplicate.writes=1,a=>a.duplicate.notifications=1,a=>a.inactive[0].active=true,a=>{a.flow[0].start_line=1;a.flow[0].end_line=1;a.flow[0].evidence=['src/entry.cjs:1']},a=>a.flow[0].evidence=['not src/entry.cjs:1']]){
  const answer=structuredClone(valid);mutate(answer);assert.equal(exploration.grade(answer,context).pass,false,String(mutate));
 }
 const partial=JSON.parse(evidence.find(item=>JSON.parse(item.text).version.path==='src/submit.cjs').text);partial.returned_range={start_line:1,end_line:2};partial.text=exploration.sources['src/submit.cjs'].split('\n').slice(0,2).join('\n')+'\n';
 const replaced=evidence.map(item=>JSON.parse(item.text).version.path==='src/submit.cjs'?f.capture(item.artifact,JSON.stringify(partial),'evidence','vcp-tool-result-v1'):item);
 const bounded=runner.reviewEvidence(f.state,f.transcript,replaced,files,f.task,exploration.sources);assert.equal(exploration.grade(valid,bounded).pass,false,'Unread duplicate branch cannot qualify');
 partial.returned_range.end_line=4;
 const forged=replaced.map(item=>JSON.parse(item.text).version.path==='src/submit.cjs'?f.capture(item.artifact,JSON.stringify(partial),'evidence','vcp-tool-result-v1'):item);
 assert.equal(runner.reviewEvidence(f.state,f.transcript,forged,files,f.task,exploration.sources).references.size,evidence.length-1,'Claimed range must match actual returned bytes');
});

test('usage counts inclusive parent and child tokens only from exact canonical settled response receipts',()=>{
 function fixture(){
  const f=reviewEvidenceFixture(),evidence=[];
  for(const [id,task,role,input] of [['a','root','main',100],['b','child','child',200]]){
   const scope={workspace:'workspace',session:'session',task};f.state.records[id]={collection:'attempt',value:{id,scope,phase:'settled',role,provider_request:'response-'+id,charged:'10'}};
   const item=f.capture('usage-'+id,JSON.stringify({response_id:'response-'+id,usage:{cost:{currency:'USD',micros:'10'},tokens:{input:String(input),output:'30',reasoning:'20'}}}),'evidence','openrouter-normalized-response/1');f.state.records[item.artifact].value.spec.scope=scope;evidence.push(item);
  }
  return {...f,evidence};
 }
 const f=fixture(),usage=runner.usageAccounting(f.state,f.evidence,'root','child');assert.equal(usage.input_tokens,300);assert.equal(usage.parent_input_tokens,100);assert.equal(usage.child_input_tokens,200);assert.equal(usage.output_tokens,60);assert.equal(usage.reasoning_tokens,40);
 for(const mutate of [f=>f.evidence.pop(),f=>f.evidence.push(f.evidence[0]),f=>f.evidence[0].text+=' ',f=>f.state.records['usage-a'].value.spec.scope.task='foreign',f=>f.state.records.a.value.charged='11',f=>f.state.records.a.value.provider_request='different',f=>f.state.records['usage-a'].value.state='aborted',f=>f.state.records['usage-a'].value.retained=[]]){
  const bad=fixture();mutate(bad);assert.throws(()=>runner.usageAccounting(bad.state,bad.evidence,'root','child'));
 }
});
test('delegation adapter invocation pins the qualified Rust stack and forwards only the provider secret boundary',t=>{
 const oldProvider=process.env.OPENROUTER_API_KEY,oldUnrelated=process.env.VCP_UNRELATED_SECRET;process.env.OPENROUTER_API_KEY='provider-fixture';process.env.VCP_UNRELATED_SECRET='must-not-reach-adapter';
 t.after(()=>{if(oldProvider===undefined)delete process.env.OPENROUTER_API_KEY;else process.env.OPENROUTER_API_KEY=oldProvider;if(oldUnrelated===undefined)delete process.env.VCP_UNRELATED_SECRET;else process.env.VCP_UNRELATED_SECRET=oldUnrelated;});
 const environment=runner.adapterEnvironment();assert.equal(environment.RUST_MIN_STACK,'16777216');assert.equal(environment.OPENROUTER_API_KEY,'provider-fixture');assert.equal(environment.VCP_UNRELATED_SECRET,undefined);
 const result=runner.invokeAdapter(process.execPath,['-e',`process.stdout.write(JSON.stringify({stack:process.env.RUST_MIN_STACK,provider:process.env.OPENROUTER_API_KEY,unrelated:process.env.VCP_UNRELATED_SECRET??null}));process.stderr.write('tokio-rt-worker stack overflow sk-or-v1-secretfixture');process.exit(19)`],3000);
 assert.equal(result.status,19);assert.deepEqual(JSON.parse(result.stdout),{stack:'16777216',provider:'provider-fixture',unrelated:null});assert.equal(runner.stderrSummary(result.stderr),'tokio-rt-worker stack overflow [redacted-provider-key]');
});
test('preparation refuses aggregate exposure overflow before reading model config or creating trial',()=>{
 const temp=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-contract-')),spec=path.join(temp,'spec.json'),trial=path.join(temp,'trial');
 fs.writeFileSync(spec,JSON.stringify({stage:'generation',stage_cap_usd:'5.0',overall_cap_usd:'10.0',prior_exposure_micros:5000001}));
 assert.throws(()=>runner.prepare(spec,trial),/ceiling/);assert.equal(fs.existsSync(trial),false);
 fs.rmSync(temp,{recursive:true});
});

test('delegation generation freezes its independent two-source fixture and preserves staged inputs',()=>{
 const manifest=JSON.parse(fs.readFileSync(path.join(fixture,'manifest.json')));
 assert.deepEqual(manifest.editable,['src/cart.cjs','src/cents.cjs']);
 assert.deepEqual(manifest.files.map(file=>file.path).filter(file=>file.includes('note')),['notes.txt','staged-note.txt','unstaged-note.txt']);
 assert.ok(manifest.files.some(file=>file.path==='src/cents.cjs'));
 assert.equal(oracle.preservationFailures(path.join(fixture,'project')).length,0);
 const workspace=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-preservation-'));
 try {
  fs.cpSync(path.join(fixture,'project'),workspace,{recursive:true});
  fs.writeFileSync(path.join(workspace,'staged-note.txt'),'changed');
  assert.deepEqual(oracle.preservationFailures(workspace).filter(file=>file==='staged-note.txt'),['staged-note.txt']);
 } finally { fs.rmSync(workspace,{recursive:true,force:true}); }
});

test('delegation oracle runs on the current Node when it provides network-denying permissions',t=>{
 if(!supportsPermissionRuntime(process.execPath)){t.skip(`Current Node ${process.version} does not expose network-denying permissions`);return;}
 const workspace=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-current-runtime-'));
 try {fs.cpSync(path.join(fixture,'project'),workspace,{recursive:true});writePassingCandidate(workspace);const result=oracle.grade(workspace);assert.equal(result.pass,true,JSON.stringify(result));}
 finally {fs.rmSync(workspace,{recursive:true,force:true});}
});

test('delegation oracle accepts an explicitly supplied qualified runtime',t=>{
 if(!process.env.VCP_DELEGATION_ORACLE_RUNTIME){t.skip('VCP_DELEGATION_ORACLE_RUNTIME was not supplied');return;}
 const runtime=path.resolve(process.env.VCP_DELEGATION_ORACLE_RUNTIME),workspace=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-runtime-'));
 try {
  assert.equal(supportsPermissionRuntime(runtime),true);
  fs.cpSync(path.join(fixture,'project'),workspace,{recursive:true});
  writePassingCandidate(workspace);
  const result=oracle.grade(workspace,runtime);assert.equal(result.pass,true,JSON.stringify(result));
 } finally {fs.rmSync(workspace,{recursive:true,force:true});}
});

function reviewPreparation(t,overall='100.000000',stageCap='100.000000',priorExposure=0){
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-cap-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 const packaged=path.join(root,'adapter-bin'),assets=path.join(packaged,'skills','builtin');fs.mkdirSync(packaged,{recursive:true});
 fs.cpSync(path.resolve(__dirname,'../../skills/builtin'),assets,{recursive:true});
 const catalog=path.join(root,'catalog.json');fs.writeFileSync(catalog,'{}');
 const adapter=path.join(packaged,'adapter.exe'),catalogBytes=fs.readFileSync(path.join(assets,'catalog.json'));fs.writeFileSync(adapter,Buffer.concat([Buffer.from('fixture'),catalogBytes]));
 const now=String(Date.now()+3600000),profile={version:1,trust_workspace:true,maximum_autonomy:'workspace',automatic_effects:['read','write'],workspace:'rebound',sync_roots:[],provider:{valid_until:now,max_output:'4096',price:{currency:'USD',valid_until:now},compatibility:{valid_until:now,responses_text_tools:true,provider_preferences_qualified:true}},catalog,routing:null,skills:null,decisions:null,processes:[],checks:[],mcp:[],mcp_http:[],output_tokens:'1',max_transport_retries:0,max_requests:1,deadline_seconds:1};
 const profileFile=path.join(root,'profile.json'),git=path.join(root,'git.txt'),spec=path.join(root,'spec.json');fs.writeFileSync(profileFile,JSON.stringify(profile));fs.writeFileSync(git,'git fixture');fs.writeFileSync(spec,JSON.stringify({stage:'review',stage_cap_usd:stageCap,overall_cap_usd:overall,prior_exposure_micros:priorExposure,profile:profileFile,adapter,git}));
 return {root,spec,profileFile,trial:path.join(root,'trial')};
}

test('new delegation campaign accepts an explicit $100 ceiling and rejects missing or overspent arithmetic',t=>{
 const valid=reviewPreparation(t);const prepared=runner.prepare(valid.spec,valid.trial),plan=JSON.parse(fs.readFileSync(prepared.plan));
 assert.equal(plan.overall_cap_micros,100000000);assert.equal(plan.stage_cap_micros,100000000);assert.equal(plan.generation_fixture,null);assert.deepEqual(plan.adapter_environment,{RUST_MIN_STACK:'16777216'});
 assert.throws(()=>runner.validate({...plan,adapter_environment:{RUST_MIN_STACK:'8388608'}},prepared.plan),/Frozen stage identity changed/);
 const missing=reviewPreparation(t);const missingSpec=JSON.parse(fs.readFileSync(missing.spec));delete missingSpec.prior_exposure_micros;fs.writeFileSync(missing.spec,JSON.stringify(missingSpec));assert.throws(()=>runner.prepare(missing.spec,missing.trial),/prior exposure/);
 const overspent=reviewPreparation(t,'100.000000','1.000000',99000001);assert.throws(()=>runner.prepare(overspent.spec,overspent.trial),/campaign ceiling/);
});

test('exploration freezes the real helper template, read-only source and identical baseline instructions',t=>{
 const f=reviewPreparation(t),spec=JSON.parse(fs.readFileSync(f.spec));spec.stage='exploration';fs.writeFileSync(f.spec,JSON.stringify(spec));
 const prepared=runner.prepare(f.spec,f.trial),plan=JSON.parse(fs.readFileSync(prepared.plan));runner.validate(plan,prepared.plan);
 assert.deepEqual(plan.arms.map(arm=>arm.name),['baseline','explore_child']);
 for(const arm of plan.arms){const dir=path.join(plan.directory,arm.name),driver=JSON.parse(fs.readFileSync(path.join(dir,'adapter-spec.json'))),profile=JSON.parse(fs.readFileSync(path.join(dir,'profile.json')));assert.deepEqual(driver.helper,{name:'explore',revision:2});assert.equal(driver.generation,false);assert.equal(profile.maximum_autonomy,'plan');assert.deepEqual(profile.automatic_effects,[]);assert.equal(fs.readFileSync(path.join(dir,'prompt.txt'),'utf8'),exploration.prompt);assert.equal(Object.keys(arm.files).length,11);}
 const child=JSON.parse(fs.readFileSync(path.join(plan.directory,'explore_child','delegation.json')));assert.deepEqual(child.helper,{name:'explore',revision:2});assert.equal(child.role,'explore');assert.equal(child.mode,'read_only');assert.deepEqual(child.write_paths,[]);
});

test('dirty Git preparation preserves the index and permits only declared source edits during grading',t=>{
 const priorSecret=process.env.OPENROUTER_API_KEY;process.env.OPENROUTER_API_KEY='must-not-reach-git';t.after(()=>{if(priorSecret===undefined)delete process.env.OPENROUTER_API_KEY;else process.env.OPENROUTER_API_KEY=priorSecret;});
 const gitEnv=runner.gitEnvironment();assert.equal(gitEnv.OPENROUTER_API_KEY,undefined);assert.equal(gitEnv.GIT_CONFIG_NOSYSTEM,'1');assert.equal(gitEnv.GIT_OPTIONAL_LOCKS,'0');assert.ok(['NUL','/dev/null'].includes(gitEnv.GIT_CONFIG_GLOBAL));
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-git-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 const workspace=path.join(root,'workspace');fs.mkdirSync(workspace);fs.cpSync(path.join(fixture,'project'),workspace,{recursive:true});
 const manifest=JSON.parse(fs.readFileSync(path.join(fixture,'manifest.json'))),files=Object.fromEntries(manifest.files.map(file=>[file.path,fs.readFileSync(path.join(fixture,'project',...file.path.split('/')))])),git=pinnedTestGit();
 const frozen=runner.prepareWorkspaceGit(workspace,files,git),index=path.join(workspace,'.git','index'),before=fs.readFileSync(index);
 runner.workspaceGitState(workspace,frozen,{git});assert.deepEqual(fs.readFileSync(index),before,'status must not refresh the frozen index');
 fs.appendFileSync(path.join(workspace,'src','cart.cjs'),'// integrated cart edit\n');fs.appendFileSync(path.join(workspace,'src','cents.cjs'),'// integrated cents edit\n');fs.writeFileSync(path.join(workspace,'notes.txt'),runner.humanNote);
 const integrated={...frozen,worktree:{...frozen.worktree,'notes.txt':require('node:crypto').createHash('sha256').update(runner.humanNote).digest('hex')}};
 runner.workspaceGitState(workspace,integrated,{git,editable:manifest.editable});assert.deepEqual(fs.readFileSync(index),before);
 const packageFile=path.join(workspace,'package.json'),packageBytes=fs.readFileSync(packageFile);fs.appendFileSync(packageFile,' ');assert.throws(()=>runner.workspaceGitState(workspace,integrated,{git,editable:manifest.editable}),/dirty worktree/);fs.writeFileSync(packageFile,packageBytes);
 runner.gitRun(workspace,git,['add','src/cart.cjs']);assert.throws(()=>runner.workspaceGitState(workspace,integrated,{git,editable:manifest.editable}),/dirty worktree|Git identity/);
});

test('generation preparation creates and freezes a dirty Git arm outside the source fixture',t=>{
 if(!process.env.VCP_DELEGATION_QUALIFIED_RUNTIME_JSON){t.skip('VCP_DELEGATION_QUALIFIED_RUNTIME_JSON was not supplied');return;}
 const prepared=reviewPreparation(t,'100.000000','2.000000');const profile=JSON.parse(fs.readFileSync(prepared.profileFile));profile.deadline_seconds=600;fs.writeFileSync(prepared.profileFile,JSON.stringify(profile));
 const runtimeFile=JSON.parse(fs.readFileSync(path.resolve(process.env.VCP_DELEGATION_QUALIFIED_RUNTIME_JSON))),qualified=runtimeFile.runtime??runtimeFile,runtime={node:qualified.node,launcher:qualified.launcher,...(qualified.build_receipt?{build_receipt:qualified.build_receipt}:{})};
 const spec=JSON.parse(fs.readFileSync(prepared.spec));spec.stage='generation';spec.git=pinnedTestGit();spec.propose_opaque_launcher_effects=true;spec.runtime=runtime;fs.writeFileSync(prepared.spec,JSON.stringify(spec));
 const result=runner.prepare(prepared.spec,prepared.trial),plan=JSON.parse(fs.readFileSync(result.plan)),arm=plan.arms[0],workspace=path.join(plan.directory,arm.name,'workspace');
 assert.equal(plan.overall_cap_micros,100000000);assert.deepEqual(JSON.parse(fs.readFileSync(path.join(plan.directory,arm.name,'profile.json'))).affected_paths,['src/cart.cjs','src/cents.cjs']);assert.deepEqual(JSON.parse(fs.readFileSync(path.join(plan.directory,arm.name,'delegation.json'))).write_paths,['src/cart.cjs','src/cents.cjs']);assert.equal(fs.existsSync(path.join(workspace,'.git','index')),true);assert.equal(Object.keys(arm.files).some(file=>file.startsWith('.git/')),false);runner.validate(plan,result.plan);
 const staged=path.join(workspace,'staged-note.txt'),original=fs.readFileSync(staged);fs.writeFileSync(staged,'tampered');assert.throws(()=>runner.validate(plan,result.plan),/Prepared fixture changed|Git identity changed/);fs.writeFileSync(staged,original);runner.validate(plan,result.plan);
});

test('failed runner CLI exits nonzero',()=>{
 const missing=path.join(os.tmpdir(),'vcp-delegation-no-such-plan.json');
 const result=require('node:child_process').spawnSync(process.execPath,['scripts/evals/delegation-live-runner.cjs','run',missing,'0'.repeat(64)],{cwd:path.resolve(__dirname,'../../..'),encoding:'utf8',windowsHide:true});
 assert.equal(result.status,1);assert.match(result.stderr,/ENOENT|no such file/i);
});
