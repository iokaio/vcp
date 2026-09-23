// SPDX-License-Identifier: Apache-2.0
'use strict';
// Offline binding only: established fixture materializer + production CLI commands.
// This does not reserve spend, launch a candidate/model or record owner approval.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const repo=path.resolve(__dirname,'../..');
const prior=require(path.join(repo,'scripts/evals/p6-live-runner.cjs'));
const builtin=require(path.join(repo,'scripts/evals/builtin-live-runner.cjs'));
const {plain,read,privateDirectory,noSecrets}=prior.boundaries;
const digest=file=>crypto.createHash('sha256').update(read(plain(path.resolve(file)),1024*1024*1024)).digest('hex');
const parse=file=>JSON.parse(read(plain(path.resolve(file))));
const write=(file,value)=>fs.writeFileSync(file,JSON.stringify(value,null,2)+'\n',{flag:'wx',mode:0o600});
const REVISION='p8-owner-v3-node-verification-1-spdx-1';
const MANIFEST='6a27284e55f440ffbc8580562b415f8cab1157e54b569dde88fbe07ec5c8969b';
function validateControlEvidence(controls,inputs){
  if(controls.schema!=='p805-page-launcher-controls/2'||controls.pass!==true||controls.inputs_unchanged!==true||controls.paid_authorization!==false||controls.model_calls!==0||controls.fixture_manifest_sha256!==MANIFEST)throw Error('Passing offline v2 launcher controls required');
  for(const [file,hash]of Object.entries(inputs))if(controls.identities?.[file]!==hash)throw Error('V2 controls identity mismatch');
  const cases=[['reference-visible',0],['reference-canonical',0],['reference-oracle',0],...Array.from({length:18},(_,i)=>['hostile-'+(i+1),1]),['permissions-visible',0],['permissions-canonical',0]];
  if(!Array.isArray(controls.cases)||controls.cases.length!==cases.length)throw Error('Complete v2 controls required');
  for(const [index,[name,status]]of cases.entries()){
    const row=controls.cases[index];
    if(row.name!==name||row.status!==status||row.signal!==null||row.error)throw Error('V2 control outcome changed');
  }
}
// This binds prior offline controls; it never launches the launcher or a model.
function validateLauncherV2(launcherFile,controlsFile){
  const launcher=parse(launcherFile),controls=parse(controlsFile);
  const source=path.join(__dirname,'p805-page-check-launcher-v2.rs'),builder=path.join(__dirname,'p805-page-launcher-v2-build.ps1'),controlRunner=path.join(__dirname,'p805-page-launcher-v2-controls.cjs');
  if(launcher.schema!=='p805-page-launcher-build/2'||launcher.exit_code!==0||launcher.parser_test_exit_code!==0||launcher.inputs_unchanged!==true||launcher.paid_authorization!==false||launcher.model_calls!==0)throw Error('Successful offline v2 launcher build required');
  if(path.resolve(launcher.source)!==source||path.resolve(launcher.builder)!==builder||launcher.node_sha256!=='8490398f5e0082772dfb0ae5a6ebdff98a97696a20cb9778b4f82eec79b6d0a1'||launcher.compiler_sha256!=='e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a'||launcher.embedded.VCP_P805_NODE!==launcher.node||launcher.embedded.VCP_P805_SYSTEMROOT!==process.env.SystemRoot)throw Error('Pinned v2 launcher inputs required');
  const expectedArgs=['--edition=2021','--crate-name','vcp_p805_page_launcher_v2',launcher.source,'-o',launcher.launcher];
  if(JSON.stringify(launcher.arguments)!==JSON.stringify(expectedArgs))throw Error('V2 launcher recipe changed');
  const inputs=Object.fromEntries(['source','builder','node','compiler','launcher'].map(key=>[path.resolve(launcher[key]),launcher[key+'_sha256']]));
  inputs[path.resolve(launcherFile)]=digest(launcherFile);inputs[controlRunner]=digest(controlRunner);
  for(const [file,hash]of Object.entries(inputs))if(digest(file)!==hash)throw Error('V2 launcher input changed');
  validateControlEvidence(controls,inputs);
  for(const row of controls.cases){
    const name=row.name;
    for(const stream of ['stdout','stderr']){
      const file=path.join(controls.root,name+'.'+stream+'.txt');
      if(digest(file)!==row[stream+'_sha256'])throw Error('V2 control output changed');
      inputs[file]=row[stream+'_sha256'];
    }
  }
  inputs[path.resolve(controlsFile)]=digest(controlsFile);
  return inputs;
}
function prepare(preparationFile,packageFile,sourceProfile,launcherFile,controlsFile){
  const preparation=parse(preparationFile),packageResult=parse(packageFile),original=parse(sourceProfile),launcher=parse(launcherFile);
  const directory=plain(path.dirname(path.resolve(preparationFile)));privateDirectory(directory);noSecrets(original);
  const fixture=path.join(repo,'src/evals/release/p8-owner-v3'),manifestFile=path.join(fixture,'manifest.json'),manifest=parse(manifestFile);
  if(manifest.revision!==REVISION||digest(manifestFile)!==MANIFEST||preparation.fixture_manifest_sha256!==MANIFEST||preparation.source_revision!==REVISION||preparation.runs.length!==6)throw Error('Revised frozen fixture/preparation identity required');
  for(const row of manifest.files)if(digest(path.join(fixture,row.path))!==row.sha256)throw Error('Frozen fixture input changed');
  const reasons=builtin.fixedProfileReasons(original,Date.now()+900000);if(reasons.length)throw Error(reasons.join('; '));
  if(original.provider.compatibility.model!=='qwen/qwen3.8-max-0902'||original.output_tokens!=='16384')throw Error('Reviewed qualified Qwen model and unchanged output allowance required');
  const packageRoot=path.join(path.dirname(path.resolve(packageFile)),'package'),exe=path.join(packageRoot,'vcp.exe'),buildFile=path.join(packageRoot,'build-receipt.json'),build=parse(buildFile);
  const archive=path.join(path.dirname(path.resolve(packageFile)),packageResult.package),entry=packageResult.manifest.files.filter(row=>row.path==='vcp.exe');
  if(entry.length!==1||digest(archive)!==packageResult.archive_sha256||digest(exe)!==entry[0].sha256||digest(buildFile)!==packageResult.manifest.build.receipt_sha256||build.profile!=='release'||build.qualification_build!==false||build.source_stable!==true||build.exit_code!==0||build.executable_sha256!==entry[0].sha256)throw Error('Exact frozen production artifact/build required');
  const launcherInputs=validateLauncherV2(launcherFile,controlsFile);
  const runnerFiles=['p805-owner-runner.cjs','p805-owner-prepare.cjs','p6-live-runner.cjs','builtin-live-runner.cjs','package-inventory.cjs'].map(name=>path.join(repo,name==='package-inventory.cjs'?'scripts':'scripts/evals',name));
  const plan={schema:'p805-owner-execution-binding/2',launcher_revision:2,spend_authorized:false,prior_cohort:'preserved-failed-no-reclassification',disposition:'prepared-for-explicit-bounded-execution',runnable:true,model_calls:0,reservations:0,fixture_revision:REVISION,fixture_manifest_sha256:MANIFEST,directory,executable:exe,executable_sha256:entry[0].sha256,package_sha256:packageResult.archive_sha256,aggregate_cap_micros:48000000,limits:manifest.proposed_limits,
    preparation_file:path.resolve(preparationFile),package_file:path.resolve(packageFile),source_profile:path.resolve(sourceProfile),launcher_file:path.resolve(launcherFile),launcher_controls_file:path.resolve(controlsFile),campaign:path.join(repo,'artifacts/p7-p8-owner-campaign.json'),runner_hashes:Object.fromEntries(runnerFiles.map(file=>[file,digest(file)])),
    input_hashes:{...launcherInputs,...Object.fromEntries([preparationFile,packageFile,archive,exe,buildFile,sourceProfile,original.catalog,__filename].map(file=>[path.resolve(file),digest(file)]))},runs:[],
    permission_review:{u03_sole_process:launcher.launcher,automatic_effects:['read','write','execute','network','install','publish','opaque'],reason:'Existing broker conservatively classifies native execution as opaque. Only the argument-restricted pagination launcher is configured; its Node permission fence has no network/child-process/write grant. Exact proposal requires owner approval.'},
    accounting:{per_request_bound_micros:6492808,next_request_available_after_other_liabilities_at_most_micros:1507192,unknown_charge_stops_later_owner_dispatch:true,explanation:'Sequential settled responses release unused liability; $8 is not a promise that sixteen requests fit. No bound/qualification reduction and no repeats.'},
    limitations:['Three scenarios repeated across two backends, not six independent tasks.','U01 separately populated memory isolation and U02 visible child review are not exercised by these direct tasks.','Human finding/usefulness/architecture rubrics require separate owner judgments.','U04 handoff skipped; U05/U06/U07/U08/U09 require their separate integrated evidence.','Physical full-volume exhaustion remains open.']};
  const schedule=['u01-sqlite','u01-files','u02-sqlite','u02-files','u03-sqlite','u03-files'];
  for(const [index,row]of preparation.runs.entries()){
    if(row.id!==schedule[index]||row.status!=='prepared-not-run'||row.hidden_files_copied!==false)throw Error('Frozen schedule or isolation changed');
    for(const item of row.workspace_files)if(digest(path.join(row.workspace,item.path))!==item.sha256)throw Error('Prepared workspace changed');
    if(digest(row.prompt)!==row.prompt_sha256)throw Error('Frozen prompt changed');
    const base=path.dirname(row.workspace),data=path.join(base,'data'),profileFile=path.join(base,'owner-profile.json'),generation=row.case==='U03';
    if(fs.existsSync(data)||fs.existsSync(profileFile))throw Error('New data/profile locations required');
    const profile={...original,workspace:row.workspace,budget_usd:'8.000000',max_requests:16,deadline_seconds:900,provider_timeout_seconds:360,max_transport_retries:0,output_tokens:original.output_tokens,maximum_autonomy:generation?'autonomous':'plan',automatic_effects:generation?plan.permission_review.automatic_effects:[],affected_paths:generation?['src/domain/window.cjs','src/api/page.cjs']:row.workspace_files.map(file=>file.path),processes:[],checks:[]};
    if(generation){profile.processes=[{name:'p805-page-node',executable:launcher.launcher,environment:{SystemRoot:process.env.SystemRoot},required_isolation:[],reduced_isolation:true,inputs:[]}];profile.checks=[{manifest:'package.json',runner:'node',profile:'p805-page-node',expected_tests:['empty array','small default page is a copy','invalid array'],rationale:'Frozen P8-05 pagination integrated-parent check'}];}
    fs.mkdirSync(data);write(profileFile,profile);
    const common=['--format','jsonl','--non-interactive','--workspace',row.workspace,'--data-dir',data];
    plan.runs.push({...row,data,profile:profileFile,profile_sha256:digest(profileFile),cap_micros:8000000,backend_command:{program:exe,args:[...common,'storage','configure','--backend',row.backend]},paid_command:{program:exe,args:[...common,'--config',profileFile,'run','--file',row.prompt,'--budget-usd','8.000000','--autonomy',generation?'autonomous':'plan']},hidden_grader:generation?{program:launcher.node,args:['--permission','--max-old-space-size=64','--allow-fs-read='+row.workspace,'--allow-fs-read='+path.join(fixture,'u03/hidden/oracle.cjs'),path.join(fixture,'u03/hidden/oracle.cjs'),row.workspace],clear_environment:true,timeout_ms:10000,expected_exit:0}:null});
  }
  const output=path.join(directory,'owner-execution-plan.json');write(output,plan);return {plan:output,sha256:digest(output),runs:6,model_calls:0,reservations:0,runnable:true};
}
module.exports={prepare,validateLauncherV2,validateControlEvidence};
if(require.main===module){try{if(process.argv.length!==7)throw Error('Usage: prepare-owner-execution.cjs <private-preparation.json> <package-result.json> <private-source-profile.json> <v2-launcher-build-receipt.json> <v2-launcher-controls-receipt.json>');console.log(JSON.stringify(prepare(...process.argv.slice(2))));}catch(error){console.error(error.message);process.exitCode=1;}}
