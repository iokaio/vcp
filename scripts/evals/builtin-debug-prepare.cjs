// SPDX-License-Identifier: Apache-2.0
'use strict';
// Preparation never dispatches a model or candidate.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const prior=require('./p6-live-runner.cjs'),paired=require('./builtin-live-runner.cjs'),generation=require('./builtin-generation-prepare.cjs');
const oracle=require('./builtin-debug-v2-oracle.cjs');
const {plain,read,write,within,safeChild,noParentInstructions,privateDirectory,noSecrets,usd}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),fixture=path.join(repo,'src/evals/skills/builtin/debug-v2/manifest.json');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const nodeSha='8490398f5e0082772dfb0ae5a6ebdff98a97696a20cb9778b4f82eec79b6d0a1';
const ids=['seeded-failure','interrupted-instrumentation','concurrent-human-edit','missing-reproduction-access'];
const bindings={prepare_sha256:'builtin-debug-prepare.cjs',runner_sha256:'builtin-debug-runner.cjs',oracle_sha256:'builtin-debug-v2-oracle.cjs',shared_runner_sha256:'p6-live-runner.cjs',skill_runner_sha256:'builtin-live-runner.cjs',generation_prepare_sha256:'builtin-generation-prepare.cjs',generation_runner_sha256:'builtin-generation-runner.cjs'};
function pool(){const bytes=read(fixture),manifest=JSON.parse(bytes);if(manifest.revision!=='p7-02-cr06-debug-v2'||JSON.stringify(manifest.cases.map(c=>c.id))!==JSON.stringify(ids))throw Error('Frozen debug cohort changed');return {manifest,sha256:sha(bytes)};}
function sourceReasons(profile){const reasons=paired.fixedProfileReasons(profile);if(profile.maximum_autonomy!=='workspace'||JSON.stringify([...(profile.automatic_effects||[])].sort())!==JSON.stringify(['read','write']))reasons.push('Fixed workspace read/write source profile required; no execution authority');return reasons;}
function permissionReview(runtime,proposal){return {proposal_requested:proposal,approval:'pending_exact_plan_authorization',available_reproduction_effects:proposal?[...generation.opaqueEffects]:['read','write'],missing_reproduction_effects:['read','write'],sole_process:proposal?runtime.launcher:null,restriction:'Only the pinned CR06 shipping.cjs launcher in three available-reproduction cases; missing access has no process or check authority. Conservative opaque effects require exact-plan authorization.'};}
function qualifyRuntime(input){
  if(!input)return null;
  if(process.platform!=='win32'||Object.keys(input).sort().join(',')!=='build_receipt,launcher,node')throw Error('Explicit Windows pinned runtime and recorded local build receipt required');
  const node=plain(path.resolve(input.node)),launcher=plain(path.resolve(input.launcher)),file=plain(path.resolve(input.build_receipt));
  const source=plain(path.join(__dirname,'builtin-debug-launcher.rs')),builder=plain(path.join(__dirname,'builtin-debug-build.ps1'));
  const bytes=read(file),receipt=JSON.parse(bytes),runtime={node,node_sha256:sha(read(node,128*1024*1024)),launcher,launcher_sha256:sha(read(launcher,128*1024*1024)),launcher_reference_source_sha256:sha(read(source)),build_receipt:file,build_receipt_sha256:sha(bytes),launcher_build_provenance:'recorded_local_build'};
  if(runtime.node_sha256!==nodeSha||receipt.schema!=='p7-cr06-launcher-build/1'||receipt.exit_code!==0||receipt.source!==source||receipt.source_sha256!==runtime.launcher_reference_source_sha256||receipt.node!==node||receipt.node_sha256!==nodeSha||receipt.launcher!==launcher||receipt.launcher_sha256!==runtime.launcher_sha256||receipt.embedded?.VCP_CR06_NODE!==node||receipt.embedded?.VCP_CR06_SYSTEMROOT!==process.env.SystemRoot||receipt.inputs_unchanged!==true||receipt.builder!==builder||receipt.builder_sha256!==sha(read(builder))||receipt.compiler_sha256!==sha(read(plain(receipt.compiler),128*1024*1024))||JSON.stringify(receipt.arguments)!==JSON.stringify(['--edition=2021','--crate-name','vcp_cr06_launcher',source,'-o',launcher]))throw Error('Debug launcher build receipt does not bind exact current inputs and executable');
  return runtime;
}
function promptFor(scenario){return scenario.prompt+'\nRead current source before editing. Only shipping.cjs may change. '+(scenario.reproduction==='available'?'Use vcp_exec with profile "cr06-check", arguments ["--test","--test-reporter=tap","--test-concurrency=1","shipping.test.cjs"], directory "", timeout_ms 10000, output_bytes 65536 and input null before any edit to retain the failing reproduction. After the fix, run vcp_verify for the current result; its configured check uses the same frozen threshold test. The check asserts shipping fees for subtotals 49, 50 and 51.':'No executable profile or check is authorized; execution must remain not-run. Run vcp_verify with current source evidence and report that modified-source acceptance remains incomplete because no reproduction check is authorized.')+' Give a concise final answer describing the fix, actual check receipts, cleanup and any not-run checks. Artifact IDs are evidence references, not workspace file paths.\n';}
function qualifiedProfile(profile,workspace,catalog,allocation,runtime,proposal,scenario){
  const derived={...profile,workspace,catalog,budget_usd:usd(allocation),affected_paths:['shipping.cjs']};
  if(runtime&&proposal&&scenario.reproduction==='available'){
    derived.maximum_autonomy='autonomous';derived.automatic_effects=[...generation.opaqueEffects];
    derived.processes=[{name:'cr06-check',executable:runtime.launcher,environment:{SystemRoot:process.env.SystemRoot},required_isolation:[],reduced_isolation:true,inputs:[]}];
    derived.checks=[{manifest:'package.json',runner:'node',profile:'cr06-check',timeout_ms:10000,expected_tests:['shipping fee threshold includes 50'],rationale:'Frozen CR06 v2 threshold reproduction and current-source verification'}];
  }
  return derived;
}
function filesFor(manifest,scenario){return Object.fromEntries(Object.entries({...manifest.files,'shipping.cjs':scenario.source}).sort(([a],[b])=>a.localeCompare(b)).map(([file,bytes])=>[file,sha(Buffer.from(bytes))]));}
function prepare(specFile,destination){
  const specBytes=read(specFile),spec=JSON.parse(specBytes);noSecrets(spec);
  if(!['aggregate_cap_usd,executable,profile','aggregate_cap_usd,executable,profile,runtime','aggregate_cap_usd,executable,profile,propose_opaque_launcher_effects,runtime'].includes(Object.keys(spec).sort().join(',')))throw Error('Spec requires executable, profile, aggregate_cap_usd and optional runtime/explicit opaque proposal');
  if(spec.propose_opaque_launcher_effects!==undefined&&typeof spec.propose_opaque_launcher_effects!=='boolean')throw Error('Opaque launcher proposal must be boolean');
  const proposal=spec.propose_opaque_launcher_effects===true,runtime=qualifyRuntime(spec.runtime);if(proposal&&!runtime)throw Error('Opaque proposal requires qualified runtime');
  destination=plain(path.resolve(destination));if(within(repo,destination)||within(destination,repo)||fs.existsSync(destination))throw Error('New private directory outside repository required');noParentInstructions(path.dirname(destination));privateDirectory(destination);
  const executable=plain(path.resolve(spec.executable)),assets=generation.inventory(path.join(path.dirname(executable),'skills/builtin'));
  if(JSON.stringify(assets)!==JSON.stringify(generation.inventory(path.join(repo,'src/skills/builtin'))))throw Error('Exact current packaged skills required');
  const profileFile=plain(path.resolve(spec.profile)),profileBytes=read(profileFile),profile=JSON.parse(profileBytes);noSecrets(profile);const reasons=sourceReasons(profile);if(reasons.length)throw Error(reasons.join('; '));
  const selected=pool(),cap=prior.micros(spec.aggregate_cap_usd),allocation=Math.floor(cap/4);if(allocation<1)throw Error('Positive per-scenario allocations required');
  const catalog=plain(path.resolve(profile.catalog));
  const plan={schema:'p7-cr06-debug-preparation/1',runnable:!!runtime&&proposal,authorization:false,runtime,permission_review:permissionReview(runtime,proposal),blockers:!runtime?['qualified_debug_launcher_and_build_receipt']:proposal?[]:['explicit_opaque_launcher_permission_proposal_required'],directory:destination,executable,executable_sha256:sha(read(executable,1024*1024*1024)),assets,fixture_revision:selected.manifest.revision,fixture_sha256:selected.sha256,...Object.fromEntries(Object.entries(bindings).map(([field,name])=>[field,sha(read(path.join(__dirname,name)))])),spec_source:plain(path.resolve(specFile)),spec_sha256:sha(specBytes),profile_source:profileFile,profile_sha256:sha(profileBytes),catalog,catalog_sha256:sha(read(catalog)),aggregate_cap_micros:cap,allocated_cap_micros:allocation*4,model_calls:0,runs:[]};
  fs.mkdirSync(destination,{mode:0o700});
  for(const scenario of selected.manifest.cases){
    const base=safeChild(destination,scenario.id),workspace=path.join(base,'workspace');oracle.prepare(scenario.id,workspace);fs.mkdirSync(path.join(base,'data'));write(path.join(base,'prompt.txt'),promptFor(scenario));write(path.join(base,'profile.json'),qualifiedProfile(profile,workspace,catalog,allocation,runtime,proposal,scenario));
    plan.runs.push({id:scenario.id,arm:'skill',skill:'review-debug',reproduction:scenario.reproduction,cap_micros:allocation,files:filesFor(selected.manifest,scenario),editable:['shipping.cjs'],prompt_sha256:sha(Buffer.from(promptFor(scenario))),profile_sha256:sha(read(path.join(base,'profile.json')))});
  }
  write(path.join(destination,'plan.json'),plan);return {plan:path.join(destination,'plan.json'),sha256:sha(read(path.join(destination,'plan.json'))),runnable:plan.runnable,runs:4,model_calls:0,aggregate_cap_micros:cap};
}
module.exports={prepare,qualifyRuntime,qualifiedProfile,permissionReview,promptFor,filesFor,pool,bindings,sourceReasons};
if(require.main===module){try{const [command,spec,destination,...rest]=process.argv.slice(2);if(command!=='prepare'||!spec||!destination||rest.length)throw Error('Usage: builtin-debug-prepare.cjs prepare <spec.json> <new-private-directory>');console.log(JSON.stringify(prepare(spec,destination)));}catch(error){console.error(error.message);process.exitCode=1;}}
