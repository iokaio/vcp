// SPDX-License-Identifier: Apache-2.0
'use strict';
// Preparation only. No command in this module dispatches a model or candidate.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const prior=require('./p6-live-runner.cjs');
const {plain,read,write,within,safeChild,filesUnder,noParentInstructions,privateDirectory,noSecrets,usd}=prior.boundaries;
const repo=path.resolve(__dirname,'../..'),fixtures=path.join(repo,'src/evals/skills/builtin/generation-v1');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const nodeSha='8490398f5e0082772dfb0ae5a6ebdff98a97696a20cb9778b4f82eec79b6d0a1';
function qualifiedProfile(profile,workspace,catalog,allocation,runtime){
  const derived={...profile,workspace,catalog,budget_usd:usd(allocation),affected_paths:['src/cart.cjs']};
  if(runtime){
    derived.maximum_autonomy='autonomous';derived.automatic_effects=['read','write','execute'];
    derived.processes=[{name:'u03-node',executable:runtime.launcher,environment:{SystemRoot:process.env.SystemRoot},required_isolation:[],reduced_isolation:true,inputs:[]}];
    derived.checks=[{manifest:'package.json',runner:'node',profile:'u03-node',expected_tests:['existing subtotal behavior','basis point discount rounds the discount half up'],rationale:'Frozen U03 generation acceptance'}];
  }
  return derived;
}
function qualifyRuntime(input){
  if(!input)return null;
  if(process.platform!=='win32'||Object.keys(input).sort().join(',')!=='launcher,node')throw Error('Explicit Windows pinned Node and qualification launcher required');
  const node=plain(path.resolve(input.node)),launcher=plain(path.resolve(input.launcher));
  if(sha(read(node,128*1024*1024))!==nodeSha)throw Error('Exact qualified portable Node26.9.0 required');
  // These hashes pin independent inputs. They do not prove that an owner-supplied
  // executable was compiled from this reference source or embeds this Node path.
  // The owner must review the launcher before authorizing the exact live plan.
  return {node,node_sha256:nodeSha,launcher,launcher_sha256:sha(read(launcher,128*1024*1024)),launcher_reference_source_sha256:sha(read(path.join(__dirname,'builtin-generation-launcher.rs'))),launcher_build_provenance:'owner_supplied_unverified'};
}
function inventory(directory){return Object.fromEntries(filesUnder(directory).map(file=>[file,sha(read(safeChild(directory,file)))]));}
function prepare(specFile,destination){
  const specBytes=read(specFile),spec=JSON.parse(specBytes);noSecrets(spec);
  if(!['aggregate_cap_usd,executable,profile','aggregate_cap_usd,executable,profile,runtime'].includes(Object.keys(spec).sort().join(',')))throw Error('Spec requires executable, profile, aggregate_cap_usd and optional qualified runtime');
  const runtime=qualifyRuntime(spec.runtime);
  destination=plain(path.resolve(destination));
  if(within(repo,destination)||within(destination,repo)||fs.existsSync(destination))throw Error('New private directory outside repository required');
  noParentInstructions(path.dirname(destination));privateDirectory(destination);
  const executable=plain(path.resolve(spec.executable)),assets=inventory(path.join(path.dirname(executable),'skills/builtin'));
  if(JSON.stringify(assets)!==JSON.stringify(inventory(path.join(repo,'src/skills/builtin'))))throw Error('Exact current packaged skill assets required');
  const profileBytes=read(plain(path.resolve(spec.profile))),profile=JSON.parse(profileBytes);noSecrets(profile);
  const reasons=prior.profileReasons(profile,'fixed_economical');
  if(profile.routing||profile.maximum_autonomy!=='workspace'||JSON.stringify([...profile.automatic_effects||[]].sort())!==JSON.stringify(['read','write']))reasons.push('Fixed workspace read/write profile required; no execution authority');
  if(reasons.length)throw Error(reasons.join('; '));
  const manifestBytes=read(path.join(fixtures,'manifest.json')),manifest=JSON.parse(manifestBytes);
  if(manifest.revision!=='p7-02-u03-generation-v1'||manifest.editable.length!==1||manifest.editable[0]!=='src/cart.cjs')throw Error('Frozen generation fixture changed');
  const files={};for(const file of manifest.files){const bytes=read(safeChild(path.join(fixtures,'project'),file.path));if(bytes.length!==file.bytes||sha(bytes)!==file.sha256)throw Error('Frozen fixture bytes changed');files[file.path]=bytes;}
  const cap=prior.micros(spec.aggregate_cap_usd),allocation=Math.floor(cap/2);if(allocation<1)throw Error('Positive per-arm allocations required');
  const catalog=plain(path.resolve(profile.catalog));
  const plan={schema:'p7-u03-generation-preparation/1',runnable:!!runtime,authorization:false,runtime,
    blockers:runtime?[]:['qualified_parent_verification_profile','generation_live_executor','network_denying_oracle_runtime'],
    directory:destination,executable,executable_sha256:sha(read(executable,1024*1024*1024)),assets,
    fixture_revision:manifest.revision,fixture_sha256:sha(manifestBytes),runner_sha256:sha(read(__filename)),oracle_sha256:sha(read(path.join(__dirname,'builtin-generation-oracle.cjs'))),
    shared_runner_sha256:sha(read(path.join(__dirname,'p6-live-runner.cjs'))),live_runner_sha256:sha(read(path.join(__dirname,'builtin-generation-runner.cjs'))),skill_runner_sha256:sha(read(path.join(__dirname,'builtin-live-runner.cjs'))),spec_sha256:sha(specBytes),profile_source:plain(path.resolve(spec.profile)),profile_sha256:sha(profileBytes),catalog,catalog_sha256:sha(read(catalog)),
    aggregate_cap_micros:cap,allocated_cap_micros:allocation*2,model_calls:0,runs:[]};
  fs.mkdirSync(destination,{mode:0o700});
  for(const arm of ['baseline','skill']){
    const base=safeChild(destination,arm),workspace=path.join(base,'workspace');fs.mkdirSync(workspace,{recursive:true,mode:0o700});fs.mkdirSync(path.join(base,'data'));
    for(const [file,bytes] of Object.entries(files)){const target=safeChild(workspace,file);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes,{flag:'wx'});}
    write(path.join(base,'prompt.txt'),manifest.prompt);
    write(path.join(base,'profile.json'),qualifiedProfile(profile,workspace,catalog,allocation,runtime));
    plan.runs.push({arm,skill:arm==='skill'?'vcp-builtin::javascript-typescript::javascript-typescript':null,cap_micros:allocation,
      files:Object.fromEntries(manifest.files.map(file=>[file.path,file.sha256])),editable:manifest.editable,prompt_sha256:sha(Buffer.from(manifest.prompt)),profile_sha256:sha(read(path.join(base,'profile.json')))});
  }
  write(path.join(destination,'plan.json'),plan);
  return {plan:path.join(destination,'plan.json'),sha256:sha(read(path.join(destination,'plan.json'))),runnable:!!runtime,launcher_build_provenance:runtime?.launcher_build_provenance??null,runs:2,model_calls:0,aggregate_cap_micros:cap};
}
module.exports={prepare,qualifiedProfile,qualifyRuntime,inventory};
if(require.main===module){try{const [command,spec,destination,...rest]=process.argv.slice(2);if(command!=='prepare'||!spec||!destination||rest.length)throw Error('Usage: builtin-generation-prepare.cjs prepare <spec.json> <new-private-directory>');console.log(JSON.stringify(prepare(spec,destination)));}catch(error){console.error(error.message);process.exitCode=1;}}
