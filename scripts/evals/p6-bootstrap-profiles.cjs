// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs=require('node:fs'), path=require('node:path'), crypto=require('node:crypto');
const quality=require('./p6-task-quality.cjs');
const qualification=require('./p6-profile-qualification.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const json=v=>JSON.stringify(v,null,2)+'\n';
function read(file) {
  const fd=fs.openSync(file,'r');
  try {
    const st=fs.fstatSync(fd);if(!st.isFile() || st.size>16*1024*1024) throw Error('Bounded regular input required');
    const b=Buffer.alloc(st.size+1);let n=0;
    while(n<b.length) {const k=fs.readSync(fd,b,n,b.length-n,null);if(!k)break;n+=k;}
    if(n>st.size)throw Error('Input changed during read');return b.subarray(0,n);
  } finally {fs.closeSync(fd);}
}
function canonical(v) {
  if(Array.isArray(v))return v.map(canonical);
  if(v && typeof v==='object')return Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])]));
  return v;
}
function seal(v){const copy={...v,id:''};copy.id=sha(JSON.stringify(canonical(copy)));return copy;}
function counter(v) {if(typeof v!=='string'||!/^(0|[1-9][0-9]*)$/.test(v)||BigInt(v)>BigInt(Number.MAX_SAFE_INTEGER))throw Error('Canonical decimal counter required');return Number(v);}
function bound(ref) {if(!ref || !/^[a-f0-9]{64}$/.test(ref.sha256))throw Error('Explicit input hash required');const b=read(ref.path);if(sha(b)!==ref.sha256)throw Error('Source hash changed');return b;}
const zeroUsage=()=>({input:'0',output:'0',cache_read:'0',cache_write:'0',reasoning:'0',requests:'0',provider_tools:'0'});
const zeroMoney=()=>({currency:'USD',micros:'0'});
const unique=xs=>[...new Set(xs)].sort();
function build(spec) {
  const planBytes=read(path.join(spec.bootstrap_directory,'plan.json')), resultBytes=read(path.join(spec.bootstrap_directory,'result.json'));
  const plan=JSON.parse(planBytes), result=JSON.parse(resultBytes), pool=quality.load(plan.manifest_revision);
  const expectedTuning=pool.cases.filter(c=>c.partition==='tuning').length;
  const now=counter(spec.observed_at), until=counter(spec.valid_until), deadline=counter(spec.deadline);
  if(until<=now || until-now>86400000 || deadline<=now || deadline>until)throw Error('Fresh bounded explicit validity and deadline required');
  const validated=qualification.report(spec.bootstrap_directory);
  if(!['p6-task-quality-v1','p6-task-quality-v3'].includes(pool.manifest.revision) || plan.manifest_sha256!==pool.manifest_sha256 || result.stopped || (pool.manifest.revision==='p6-task-quality-v3' && plan.partition!=='tuning'))throw Error('Completed frozen tuning bootstrap identity required');
  const graded=quality.grade({revision:pool.manifest.revision,manifest_sha256:pool.manifest_sha256,runs:result.runs.map(({case_id,strategy,start_state_sha256,status,answer})=>({case_id,strategy,start_state_sha256,status,answer}))},pool);
  const ids=['fixed_economical','fixed_stronger'], entries=[], raw_catalogs={}, bases={}, training=[];
  for(const id of ids) {
    const source=spec.strategies?.[id], profileBytes=bound(source?.profile), probeBytes=bound(source?.probe), profile=JSON.parse(profileBytes), snapshot=profile.provider;
    if(profile.routing || !snapshot || snapshot.compatibility?.responses_text_tools!==true || snapshot.compatibility.provider_preferences_qualified!==true || snapshot.compatibility.byte_ceiling_qualified!==false || counter(snapshot.valid_until)<until || counter(snapshot.compatibility.valid_until)<until)throw Error('Authentic current fixed snapshot with full-input liability required');
    const probe=JSON.parse(probeBytes);
    if(probe.schema!=='p6-provider-qualification/1' || !/^[a-f0-9]{64}$/.test(probe.authorized_sources_sha256) || snapshot.compatibility.id!=='p6-generation-qualified/'+probe.authorized_sources_sha256 || !Array.isArray(probe.attribution) || probe.attribution.length!==2 || probe.attribution.some(a=>a.requested_model!==snapshot.compatibility.model || a.catalog_endpoint!==snapshot.compatibility.endpoint || !a.response_id || !a.observed_model_revision || !a.provider_name || !a.observed_endpoint_id || !a.method || !/^[a-f0-9]{64}$/.test(a.generation_sha256)))throw Error('Exact two-response qualified attribution claim required');
    const raw=read(profile.catalog);
    if(sha(raw)!==snapshot.raw_sha256 || sha(raw)!==plan.strategies[id]?.catalog_sha256 || spec.strategies[id].profile.sha256!==plan.strategies[id]?.profile_sha256)throw Error('Bootstrap profile/catalog identity differs');
    const named=plan.strategies[id]?.provider;
    if(named?.id!==snapshot.compatibility.id || named.model!==snapshot.compatibility.model || named.endpoint!==snapshot.compatibility.endpoint)throw Error('Bootstrap provider binding mismatch');
    const rows=graded.runs.filter(r=>r.strategy===id && r.partition==='tuning');
    if(rows.length!==expectedTuning || unique(rows.map(r=>r.task_class)).length!==3 || ['analysis','review','generation'].some(cls=>rows.filter(r=>r.task_class===cls).length!==expectedTuning/3))throw Error('Complete balanced tuning cohort required');
    const receipts=rows.map(r=>result.runs.find(x=>x.case_id===r.case_id && x.strategy===id));
    if(receipts.some(r=>!r || r.status==='not_run' || !Number.isSafeInteger(r.actual_cost_micros) || r.actual_cost_micros<0 || !Number.isSafeInteger(r.latency_ms) || r.latency_ms<0))throw Error('Every tuning attempt needs known costs and latency');
    const quality_bps=Math.floor(rows.filter(r=>r.task_success).length*10000/expectedTuning), eligible=quality_bps>=8000;
    const provenance=[{source:'sha256://bootstrap-result/'+sha(resultBytes),sha256:sha(resultBytes),observed_at:spec.observed_at,effective_at:spec.observed_at,limitations:['Only the balanced frozen tuning cohort informs this mixed task-class evidence; no held-out result influences membership or ordering.','Experimental p6-synthetic main-role membership only, not a shipping model rank.','Cost estimate is conservative full input capacity, not a measured token mean.']},
      {source:'sha256://provider-probe/'+sha(probeBytes),sha256:sha(probeBytes),observed_at:spec.observed_at,effective_at:spec.observed_at,limitations:['Exact prequalified snapshot supplied by owner; probe provenance must be reviewed independently.']}];
    const identity={model:snapshot.compatibility.model,endpoint:snapshot.compatibility.endpoint};
    entries.push({identity,availability:eligible?'supported':'unsupported',reasons:eligible?[]:['Frozen tuning quality below predeclared 8000 bps floor'],provenance,capabilities:{tools:'supported'},snapshot,compatibility:[{id:'probe-'+sha(probeBytes),compatibility:snapshot.compatibility.id,kind:'live',state:'supported',observed_at:spec.observed_at,valid_until:spec.valid_until,provenance:[provenance[1]]}],memberships:[{version:'p6-frozen-tuning-'+sha(resultBytes),group:'low',roles:[{id:'quality-'+id+'-'+sha(resultBytes),role:'main',task_class:'p6-synthetic',kind:'live',observed_at:spec.observed_at,valid_until:spec.valid_until,samples:expectedTuning,quality_bps,latency_p50_ms:qualification.percentile(receipts.map(r=>r.latency_ms),0.5),latency_p95_ms:qualification.percentile(receipts.map(r=>r.latency_ms),0.95),usage_p50:null,usage_p95:null,provenance:[provenance[0]]}]}]});
    raw_catalogs[snapshot.id]=raw.toString('utf8');bases[id]=profile;
    training.push({strategy:id,case_ids:rows.map(r=>r.case_id),samples:expectedTuning,quality_bps,eligible,profile_sha256:sha(profileBytes),probe_sha256:sha(probeBytes)});
  }
  if(entries[0].identity.model===entries[1].identity.model && entries[0].identity.endpoint===entries[1].identity.endpoint)throw Error('Distinct fixed candidates required');
  entries.sort((a,b)=>a.identity.model<b.identity.model?-1:a.identity.model>b.identity.model?1:a.identity.endpoint.localeCompare(b.identity.endpoint));
  const catalog=seal({schema_version:1,id:'',parent:null,observed_at:spec.observed_at,effective_at:spec.observed_at,entries});
  const template={schema_version:1,id:'',parent:null,profile:'low',allowed_models:unique(entries.map(e=>e.identity.model)),allowed_endpoints:unique(entries.map(e=>e.identity.endpoint)),allowed_groups:['low'],quality_floor_bps:8000,minimum_samples:expectedTuning,maximum_evidence_age_ms:until-now,deny_data_collection:true,require_zdr:entries.every(e=>e.snapshot.compatibility.require_zdr),ordering:['total_cost','latency','quality','capability'],pin:null,broader_task_class:null,output_tokens:'512'};
  const estimates=entries.map(e=>({candidate:e.identity,first_attempt:{...zeroUsage(),input:e.snapshot.max_input,output:'512',requests:'1'},retries:zeroUsage(),handoff:zeroUsage(),support:zeroMoney(),children:zeroMoney(),verification:zeroMoney(),assumptions:['Full endpoint max_input bounds assembled first request; estimate is not a dispatch reservation.','No retries, quality switches, decomposition, helper, child, compaction, optimizer or remote evaluator.','Offline deterministic grader has zero provider verification charge.'],evidence_refs:['sha256://bootstrap-result/'+sha(resultBytes)]}));
  const profiles={};
  for(const id of [...ids,'routed']) {
    const base=bases[id==='routed'?'fixed_economical':id];
    // Fixed baselines remain observable even when their tuning outcome excludes
    // them from routing. They are measurements, never recommendations.
    if(id!=='routed') {profiles[id]={...base,max_requests:4,max_transport_retries:0,output_tokens:'512',routing:null};continue;}
    const pin=id==='routed'?null:{candidate:{model:base.provider.compatibility.model,endpoint:base.provider.compatibility.endpoint},fallback_candidates:[]};
    profiles[id]={...base,max_requests:4,max_transport_retries:0,output_tokens:'512',routing:{catalog,policy:seal({...template,pin}),task_class:'p6-synthetic',estimates,raw_catalogs,escalation:{max_transport_retries:0,max_quality_switches:0,max_decompositions:0,max_total_attempts:4,minimum_repeated_failures:1,deadline:spec.deadline}}};
  }
  return {schema:'p6-bootstrap-profiles/1',bootstrap_plan_sha256:sha(planBytes),bootstrap_result_sha256:sha(resultBytes),manifest_sha256:pool.manifest_sha256,grader_sha256:validated.grader_sha256,builder_sha256:sha(read(__filename)),training,shipping_defaults:false,profiles};
}
module.exports={build,canonical,seal};
if(require.main===module) {
  try {
    if(process.argv.length!==4)throw Error('Usage: node scripts/evals/p6-bootstrap-profiles.cjs <spec.json> <new-output-directory>');
    const spec=JSON.parse(read(process.argv[2])), bundle=build(spec), destination=path.resolve(process.argv[3]);
    fs.mkdirSync(destination,{recursive:false,mode:0o700});
    for(const [id,profile] of Object.entries(bundle.profiles))fs.writeFileSync(path.join(destination,id+'.json'),json(profile),{flag:'wx',mode:0o600});
    fs.writeFileSync(path.join(destination,'bundle.json'),json(bundle),{flag:'wx',mode:0o600});
    process.stdout.write(json({directory:destination,training:bundle.training,shipping_defaults:false}));
  }catch(error){console.error(error.message);process.exitCode=1;}
}
