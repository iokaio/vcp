// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'), assert=require('node:assert/strict');
const fs=require('node:fs'), os=require('node:os'), path=require('node:path'), crypto=require('node:crypto');
const {spawn,spawnSync}=require('node:child_process');
const candidate=path.resolve(__dirname,'../../../scripts/release/candidate.ps1');
const state=path.resolve(__dirname,'../../../scripts/release/candidate-state.ps1');
const ids=['source-gate','provision','portable-contracts','production-build','native-package','setup-package','vsix-package','pair','native-boundaries','installed-native','installed-editor'];
const windows={skip:process.platform!=='win32'};
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const extract=String.raw`
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$tokens=$null;$errors=$null;$ast=[Management.Automation.Language.Parser]::ParseFile($Candidate,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Candidate parser failed'}
function Import-Actual([string[]]$Names){
 foreach($name in $Names){
  $found=@($ast.FindAll({param($item)$item -is [Management.Automation.Language.FunctionDefinitionAst] -and $item.Name -ceq $name},$true))
  if($found.Count -ne 1){throw ('Expected unique actual function '+$name)}
  $definition=$found[0].Extent.Text.Replace('function '+$name+'(', 'function script:'+$name+'(').Replace('function '+$name+' {','function script:'+$name+' {')
  $file=Join-Path $Root ($name+'.ps1')
  [IO.File]::WriteAllText($file,$definition,[Text.UTF8Encoding]::new($false))
  . $file
 }
}
function Require([bool]$Okay,[string]$Why){if(-not $Okay){throw $Why}}
function Refuse([scriptblock]$Action,[string]$Pattern){
 $message=$null;try{& $Action | Out-Null}catch{$message=$_.Exception.Message}
 Require ($message -match $Pattern) ('Expected refusal '+$Pattern+'; observed '+$message)
}
`;
function fixture(t,body){
 const root=fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(),'vcp-candidate-stages-')));
 t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 const runner=path.join(root,'exercise.ps1');fs.writeFileSync(runner,'param([string]$Candidate,[string]$State,[string]$Root,[string]$Node)\n'+extract+'\n'+body);
 const args=['-NoProfile','-NonInteractive','-File',runner,'-Candidate',candidate,'-State',state,'-Root',root,'-Node',fs.realpathSync.native(process.execPath)];
 const run=()=>{const result=spawnSync('pwsh',args,{encoding:'utf8',windowsHide:true,timeout:15000,maxBuffer:1024*1024});assert.ifError(result.error);assert.equal(result.status,0,result.stderr+result.stdout);return result;};
 return {root,runner,args,run};
}

test('actual Stage filters console progress, binds pass/failure evidence and rechecks source before the body',windows,t=>{
 const f=fixture(t,String.raw`
. $State
Import-Actual @('Save-Run','Stage')
function Get-CandidateDiskEvidence {return @{fixture_only=$true}}
$out=Join-Path $Root 'candidate';New-Item -ItemType Directory -Path (Join-Path $out 'logs') | Out-Null
$runPath=Join-Path $out 'run.json';$repository=$Root;$ReviewedCommit='a'*40;$node=$Node
$run=@{schema='vcp-candidate-run/1';output_root=$out;stages=@();receipts=@{};receipt_sha256=@{};status='running'}
Stage source-gate @('synthetic') 'fixture boundary' {
 'private full diagnostic, not console'
 'VCP_BUILD_PROGRESS elapsed_seconds=12 output_bytes=34 idle_seconds=5 compiler_artifacts=6'
 'VCP_BUILD_PROGRESS elapsed_seconds=12 output_bytes=34 idle_seconds=5 compiler_artifacts=6 trailing-private'
 'VCP_BUILD_PHASE phase=cargo status=running'
 'VCP_BUILD_PHASE phase=arbitrary status=running'
 $run.receipts.native=Join-Path $out 'native.json';[IO.File]::WriteAllText($run.receipts.native,'retained native fixture')
}
Require ($run.stages[0].status -ceq 'pass') 'Pass row missing'
Refuse {Stage provision @('synthetic') 'fixture failure' {
 $run.receipts.build=Join-Path $out 'build.json';[IO.File]::WriteAllText($run.receipts.build,'retained failed build fixture')
 'retained partial stage diagnostic'
 throw 'expected synthetic stage failure'
}} 'expected synthetic stage failure'
[IO.File]::WriteAllText((Join-Path $Root 'reviewed-source.txt'),'changed after earlier stage')
Refuse {Stage production-build @('synthetic') 'must not enter body' {[IO.File]::WriteAllText((Join-Path $out 'unexpected-body'),'not allowed')}} 'Reviewed source changed between candidate stages'
Require (-not (Test-Path -LiteralPath (Join-Path $out 'unexpected-body'))) 'Source refusal executed body'
Require ($run.stages.Count -eq 3 -and $run.stages[1].status -ceq 'fail' -and $run.stages[2].status -ceq 'fail') 'Failed rows missing'
Write-Output 'CONTRACT_COMPLETE'
`);
 const source=path.join(f.root,'reviewed-source.txt');fs.writeFileSync(source,'reviewed bytes');
 fs.writeFileSync(path.join(f.root,'provenance.cjs'),"const fs=require('node:fs'),crypto=require('node:crypto');fs.appendFileSync("+JSON.stringify(path.join(f.root,'source-calls.jsonl'))+",JSON.stringify(process.argv.slice(2))+'\\n');const digest=crypto.createHash('sha256').update(fs.readFileSync("+JSON.stringify(source)+")).digest('hex');process.exit(digest==="+JSON.stringify(hash('reviewed bytes'))+"?0:1);");
 const result=f.run();assert.match(result.stdout,/CONTRACT_COMPLETE/);
 assert.match(result.stdout,/^VCP_BUILD_PROGRESS elapsed_seconds=12 output_bytes=34 idle_seconds=5 compiler_artifacts=6$/m);
 assert.match(result.stdout,/^VCP_BUILD_PHASE phase=cargo status=running$/m);
 for(const hidden of ['private full diagnostic','trailing-private','phase=arbitrary','retained partial stage diagnostic'])assert(!result.stdout.includes(hidden),hidden);
 const output=path.join(f.root,'candidate'),run=JSON.parse(fs.readFileSync(path.join(output,'run.json'),'utf8'));
 assert.deepEqual(run.stages.map(row=>[row.id,row.status,row.exit_code]),[['source-gate','pass',0],['provision','fail',1],['production-build','fail',1]]);
 for(const row of run.stages)assert.equal(row.log_sha256,hash(fs.readFileSync(row.log)));
 for(const [name,file] of Object.entries(run.receipts))assert.equal(run.receipt_sha256[name],hash(fs.readFileSync(file)));
 assert.match(fs.readFileSync(run.stages[0].log,'utf8'),/trailing-private/);
 assert.match(fs.readFileSync(run.stages[1].log,'utf8'),/retained partial stage diagnostic/);
 assert.match(fs.readFileSync(run.stages[2].log,'utf8'),/Reviewed source changed/);
 const calls=fs.readFileSync(path.join(f.root,'source-calls.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
 assert.deepEqual(calls,[['source',f.root,'a'.repeat(40)],['source',f.root,'a'.repeat(40)]]);
});

test('actual selection guards do not evaluate future command arguments or stage bodies',windows,t=>{
 const f=fixture(t,String.raw`
. $State
Import-Actual @('Selected-Stage')
Set-StrictMode -Version Latest
$stageIds=@(Get-CandidateStageIds);$StopAfter='portable-contracts';$Stage='all';$repository=$Root;$ReviewedCommit='a'*40
$guards=@($ast.FindAll({param($item)$item -is [Management.Automation.Language.IfStatementAst] -and $item.Clauses[0].Item1.Extent.Text -match "^Selected-Stage '[a-z-]+'$"},$true))
Require ($guards.Count -eq 11) 'Each stage needs an outer selection guard'
$script:called=@()
function Stage([string]$Id,[string[]]$Command,[string]$Expected,[scriptblock]$Body){$script:called+=$Id}
foreach($guard in $guards){& ([scriptblock]::Create($guard.Extent.Text))}
Require (($called -join ',') -ceq 'source-gate,provision,portable-contracts') 'Bounded all-stage selection differs'
$script:called=@();$Stage='provision'
foreach($guard in $guards){& ([scriptblock]::Create($guard.Extent.Text))}
Require (($called -join ',') -ceq 'provision') 'Single-stage selection evaluated later dependencies'
@{status='pass';future_variables='intentionally undefined under StrictMode'} | ConvertTo-Json -Compress
`);
 assert.equal(JSON.parse(f.run().stdout).status,'pass');
});

test('environment restoration uses recorded tool hashes and physical temp, refusing mutations',windows,t=>{
 const f=fixture(t,String.raw`
Import-Actual @('Assert-CandidateOrdinaryPath','Restore-CandidateEnvironment')
$out=Join-Path $Root 'output';$temporary=Join-Path $Root 'temporary';$other=Join-Path $Root 'other-temp'
foreach($dir in @($out,$temporary,$other)){New-Item -ItemType Directory -Path $dir | Out-Null}
$run=@{environment=@{tools=@{};node='v24.10.0';temporary_directory=@{selected=$temporary};qualification_root=(Join-Path $temporary ('vcp-beta-private-'+[guid]::NewGuid()));editor_executable_sha256=('b'*64)}}
foreach($name in @('node','pwsh')){$file=Join-Path $Root ($name+'.exe');[IO.File]::WriteAllText($file,('owned tool '+$name));$run.environment.tools[$name]=@{path=$file;sha256=(Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant()}}
$tools=@{node='24.10.0';editor=@{version='1.138.0';commit=('c'*40)}};$Stage='provision'
Restore-CandidateEnvironment
Require ($node -ceq $run.environment.tools.node.path -and $pwsh -ceq $run.environment.tools.pwsh.path -and $env:TEMP -ceq $temporary -and $env:TMP -ceq $temporary) 'Recorded environment was not restored'
[IO.File]::WriteAllText($run.environment.tools.node.path,'changed tool')
Refuse {Restore-CandidateEnvironment} 'Selected candidate tool changed'
[IO.File]::WriteAllText($run.environment.tools.node.path,'owned tool node')
$run.environment.temporary_directory.selected=$other
Refuse {Restore-CandidateEnvironment} 'Private qualification selection changed'
$run.environment.temporary_directory.selected=$temporary;$run.environment.node='v0.0.0'
Refuse {Restore-CandidateEnvironment} 'Recorded Node differs'
$run.environment.node='v24.10.0';$Stage='portable-contracts';$script:layoutHash='b'*64
function Resolve-BetaEditor([string]$Code){Require ($Code -ceq (Join-Path $out 'editor/Code.exe')) 'Wrong selected editor path';return @{code=$Code;code_sha256=$script:layoutHash;version='1.138.0';commit=('c'*40)}}
Restore-CandidateEnvironment
Require ($env:VCP_TEST_BETA_EDITOR_ARCHIVE -ceq (Join-Path $out 'vscode.zip')) 'Editor archive selection missing'
$script:layoutHash='d'*64
Refuse {Restore-CandidateEnvironment} 'Prepared editor changed'
@{status='pass';refusals=4} | ConvertTo-Json -Compress
`);
 assert.deepEqual(JSON.parse(f.run().stdout),{status:'pass',refusals:4});
});

test('actual candidate refuses an out-of-scope stage before creating output',windows,t=>{
 const f=fixture(t,'throw "Unused fixture body"'),output=path.join(f.root,'candidate');
 const result=spawnSync('pwsh',['-NoProfile','-File',candidate,'-ReviewedCommit','a'.repeat(40),'-OutputRoot',output,'-Stage','PAIR','-StopAfter','PORTABLE-CONTRACTS'],{encoding:'utf8',windowsHide:true,timeout:10000});
 assert.ifError(result.error);assert.notEqual(result.status,0);assert.match(result.stderr,/beyond the selected stopping point/);assert.equal(fs.existsSync(output),false);
});

test('actual candidate refuses another owner of the same output directory',windows,async t=>{
 const f=fixture(t,String.raw`
$out=Join-Path $Root 'candidate'
foreach($name in @('lockIdentity','lockDigest','runMutex')){
 $found=@($ast.FindAll({param($item)$item -is [Management.Automation.Language.AssignmentStatementAst] -and $item.Left.Extent.Text -ceq ('$'+$name)},$false))
 Require ($found.Count -eq 1) ('One actual lock assignment required: '+$name)
 . ([scriptblock]::Create($found[0].Extent.Text))
}
try{
 Require $runMutex.WaitOne(0) 'Could not hold fixture mutex'
 [Console]::WriteLine('OWNED_MUTEX_READY');[Console]::Out.Flush()
 $null=[Console]::ReadLine()
}finally{$runMutex.ReleaseMutex();$runMutex.Dispose()}
`);
 const owner=spawn('pwsh',f.args,{windowsHide:true,stdio:['pipe','pipe','pipe']});let stdout='',stderr='';
 owner.stdout.on('data',chunk=>{stdout+=chunk});owner.stderr.on('data',chunk=>{stderr+=chunk});
 t.after(()=>{if(owner.exitCode===null)owner.kill();});
 const exited=new Promise((resolve,reject)=>{owner.once('exit',resolve);owner.once('error',reject);});
 const end=Date.now()+5000;while(!stdout.includes('OWNED_MUTEX_READY')&&owner.exitCode===null&&Date.now()<end)await new Promise(resolve=>setTimeout(resolve,20));
 assert(stdout.includes('OWNED_MUTEX_READY'),stdout+stderr);
 try {
  const output=path.join(f.root,'candidate');
  const result=spawnSync('pwsh',['-NoProfile','-File',candidate,'-ReviewedCommit','a'.repeat(40),'-OutputRoot',output,'-Stage','source-gate','-StopAfter','portable-contracts'],{encoding:'utf8',windowsHide:true,timeout:10000});
  assert.ifError(result.error);assert.notEqual(result.status,0);assert.match(result.stderr,/Another candidate stage owns this output directory/);assert.equal(fs.existsSync(output),false);
 } finally {owner.stdin.end('\n');await exited;}
 assert.equal(owner.exitCode,0,stderr);
});

test('workflow exposes ordered same-job stages with bounded default selection and always-retained evidence',()=>{
 const workflow=fs.readFileSync(path.resolve(__dirname,'../../../.github/workflows/beta-candidate.yml'),'utf8');
 assert.match(workflow,/default: portable-contracts/);assert.equal((workflow.match(/^  candidate:$/gm)||[]).length,1);
 const blocks=workflow.split(/\r?\n(?=      - )/),stages=blocks.filter(block=>/run: .*candidate\.ps1.* -Stage /.test(block));
 assert.deepEqual(stages.map(block=>block.match(/ -Stage ([a-z-]+)/)[1]),ids);
 for(const [index,block] of stages.entries()){
  assert.match(block,/-ReviewedCommit \$env:REVIEWED_COMMIT/);assert.match(block,/-StopAfter \$env:STOP_AFTER/);assert.match(block,/timeout-minutes: [1-9][0-9]*/);
  if(index<3)assert(!/^\s+if:/m.test(block));
  else if(index===3)assert.match(block,/if: success\(\) && inputs\.stop_after != 'portable-contracts'/);
  else if(index<8)assert.match(block,/if: success\(\) && contains\(fromJSON\('\["pair","installed-editor"\]'\), inputs\.stop_after\)/);
  else assert.match(block,/if: success\(\) && inputs\.stop_after == 'installed-editor'/);
 }
 for(const name of ['Collect sanitized evidence including failures and unrun cases','Retain complete candidate packet for review']){
  const block=blocks.find(value=>value.includes('- name: '+name));assert(block);assert.match(block,/if: always\(\)/);
 }
});
