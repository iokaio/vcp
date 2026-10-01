// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const helper=path.resolve(__dirname,'../../../scripts/release/build-progress.ps1');
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');

function fixture(t,mode){
  const root=fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(),'vcp-build-services-')));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const node=fs.realpathSync.native(process.execPath),telemetry=path.join(root,'vctip.exe');
  // A copied Node executable is a synthetic service, never the installed MSVC
  // service or a process outside this fixture's inherited, non-breakaway Job.
  fs.copyFileSync(node,telemetry);fs.writeFileSync(path.join(root,'cl.exe'),'synthetic compiler identity; not executed');
  const parent=path.join(root,'parent.ps1'),runner=path.join(root,'run.ps1'),input=path.join(root,'input.json');
  fs.writeFileSync(parent,`param([string]$InputFile)
$ErrorActionPreference='Stop'
$settings=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
Write-Output (@{endpoint=$env:_MSPDBSRV_ENDPOINT_;options=$env:_MSPDBSRV_} | ConvertTo-Json -Compress)
if($settings.mode -eq 'environment'){exit 0}
Add-Type @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class ServiceFixture {
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct Startup {
  public uint size; public string reserved,desktop,title; public uint x,y,width,height,xChars,yChars,fill,flags;
  public ushort show,reservedCount; public IntPtr reservedBytes,input,output,error;
 }
 [StructLayout(LayoutKind.Sequential)] struct Created { public IntPtr process,thread; public uint processId,threadId; }
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcessW(string image,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string directory,ref Startup start,out Created child);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 public static uint Start(string image,string source,string directory) {
  var start=new Startup();start.size=(uint)Marshal.SizeOf<Startup>(); Created child;
  // DETACHED_PROCESS supplies no console (CREATE_NO_WINDOW would create an
  // additional owned conhost). No inherited pipes or BREAKAWAY: the synthetic
  // service inherits the Job before execution and cannot retain broker output.
  if(!CreateProcessW(image,new StringBuilder("\\\""+image+"\\\" -e \\\""+source+"\\\""),IntPtr.Zero,IntPtr.Zero,false,0x00000008,IntPtr.Zero,directory,ref start,out child))throw new Win32Exception(Marshal.GetLastWin32Error());
  try{return child.processId;}finally{CloseHandle(child.thread);CloseHandle(child.process);}
 }
}
'@
$image=if($settings.mode -in @('unknown','transient')){$settings.node}else{$settings.telemetry}
$source=if($settings.mode -eq 'transient'){'setTimeout(()=>{},1800)'}else{'setInterval(()=>{},1000)'}
$ids=@([ServiceFixture]::Start($image,$source,$settings.root))
if($settings.mode -eq 'extra'){$ids+= [ServiceFixture]::Start($settings.node,'setInterval(()=>{},1000)',$settings.root)}
$ids | ConvertTo-Json -AsArray | Set-Content -LiteralPath (Join-Path $settings.root 'pids.json')
if($settings.mode -eq 'nonzero'){exit 7}else{exit 0}
`);
  fs.writeFileSync(input,JSON.stringify({root,node,telemetry,mode}));
  fs.writeFileSync(runner,`param([string]$Helper,[string]$InputFile)
$ErrorActionPreference='Stop'
. $Helper
$settings=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
$env:_MSPDBSRV_ENDPOINT_='owner-endpoint-sentinel';$env:_MSPDBSRV_='owner-options-sentinel'
$options=@{}
if($settings.mode -ne 'environment'){$options.MsvcTelemetryExecutable=$settings.telemetry}
$result=Invoke-VcpBuildProcess -Executable (Join-Path $PSHOME 'pwsh.exe') -Arguments @('-NoProfile','-NonInteractive','-File',(Join-Path $settings.root 'parent.ps1'),'-InputFile',$InputFile) -WorkingDirectory $settings.root -LogPath (Join-Path $settings.root 'build.log') -TimeoutSeconds 20 -ProgressSeconds 1 @options
Write-Output ('VCP_SERVICE_RESULT '+(@{result=$result;owner_endpoint=$env:_MSPDBSRV_ENDPOINT_;owner_options=$env:_MSPDBSRV_} | ConvertTo-Json -Depth 12 -Compress))
`);
  return {root,telemetry,runner,input};
}
function run(f){
  const child=spawnSync('pwsh',['-NoProfile','-NonInteractive','-File',f.runner,'-Helper',helper,'-InputFile',f.input],
    {encoding:'utf8',windowsHide:true,timeout:28000,maxBuffer:4*1024*1024});
  assert.ifError(child.error);assert.equal(child.status,0,child.stderr+'\n'+child.stdout);
  const line=child.stdout.split(/\r?\n/).find(line=>line.startsWith('VCP_SERVICE_RESULT '));assert(line,child.stdout);
  const observation=JSON.parse(line.slice('VCP_SERVICE_RESULT '.length)),report=observation.result;
  assert.equal(observation.owner_endpoint,'owner-endpoint-sentinel');assert.equal(observation.owner_options,'owner-options-sentinel');
  assert.equal(report.child_exit_observation_removed,true);assert.equal(report.job_active_processes_zero,true);
  const snapshot=JSON.parse(fs.readFileSync(path.join(f.root,'build-progress.json'),'utf8'));
  if(report.msvc_service_policy)assert.deepEqual(snapshot.msvc_services,{policy:report.msvc_service_policy,planned_cleanup:report.planned_service_cleanup,completion:report.completion});
  const log=fs.readFileSync(path.join(f.root,'build.log'),'utf8');
  const environment=JSON.parse(log.split(/\r?\n/).find(line=>line.startsWith('{')));
  if(fs.existsSync(path.join(f.root,'pids.json')))for(const id of JSON.parse(fs.readFileSync(path.join(f.root,'pids.json'),'utf8'))){
    assert.throws(()=>process.kill(id,0),{code:'ESRCH'},'Owned synthetic service must not survive cleanup');
  }
  return {report,snapshot,environment};
}

test('explicit pinned service cleanup records the intentional termination and isolates PDB settings to children',{skip:process.platform!=='win32'},t=>{
  const f=fixture(t,'service'),{report,snapshot,environment}=run(f);
  assert.equal(report.exit_code,0,JSON.stringify(report));assert.equal(report.process_exit_code,0);assert.equal(report.broker_exit_code,0);
  assert.equal(report.forced_cleanup,false);assert.equal(report.timed_out,false);assert.equal(report.completion,'planned-service-cleanup');
  assert.equal(snapshot.status,'pass');assert.equal(report.planned_service_cleanup.length,1);
  const row=report.planned_service_cleanup[0];assert.equal(row.result,'terminated');assert.equal(row.termination_exit_code,1);
  assert.equal(row.sha256,hash(f.telemetry));assert.equal(row.image.toLowerCase(),f.telemetry.toLowerCase());
  assert.equal(row.name.toLowerCase(),'vctip.exe');assert(row.process_id>0);assert(Number.isFinite(Date.parse(row.started_at)));
  assert.equal(report.msvc_service_policy.compiler_sha256,hash(path.join(f.root,'cl.exe')));
  assert.equal(report.msvc_service_policy.telemetry_sha256,row.sha256);assert.equal(report.msvc_service_policy.maximum_survivors,1);
  assert.equal(environment.options,'-shutdowntime 0');assert.match(environment.endpoint,/^vcp-build-[a-f0-9]{32}$/);
  assert.equal(environment.endpoint,report.msvc_service_policy.pdb_endpoint);
});

test('an ordinary transient descendant keeps the natural drain grace with compiler policy enabled',{skip:process.platform!=='win32'},t=>{
  const {report}=run(fixture(t,'transient'));
  assert.equal(report.exit_code,0,report.failure);assert.equal(report.completion,'natural');assert.equal(report.forced_cleanup,false);
  assert.deepEqual(report.planned_service_cleanup,[]);assert(report.elapsed_seconds>=1);
});

test('a surviving image mismatch is never eligible for planned cleanup',{skip:process.platform!=='win32'},t=>{
  const {report,snapshot}=run(fixture(t,'unknown'));
  assert.equal(report.process_exit_code,0);assert.equal(report.broker_exit_code,0);assert.equal(report.exit_code,1);
  assert.equal(report.completion,'failed');assert.equal(report.forced_cleanup,true);assert.equal(snapshot.status,'fail');
  assert.deepEqual(report.planned_service_cleanup,[]);assert.match(report.failure,/within 10 seconds/);
  assert.equal(report.before_cleanup.job.processes.length,1);assert.equal(report.before_cleanup.job.processes[0].name.toLowerCase(),'node.exe');
});

test('an extra owned survivor refuses the single-service policy and retains the original failure bound',{skip:process.platform!=='win32'},t=>{
  const {report}=run(fixture(t,'extra'));
  assert.equal(report.exit_code,1);assert.equal(report.forced_cleanup,true);assert.equal(report.completion,'failed');
  assert.deepEqual(report.planned_service_cleanup,[]);assert.match(report.failure,/within 10 seconds/);
  assert.equal(report.before_cleanup.job.processes.length,2);
});

test('generic supervision never changes inherited PDB environment or activates a service policy',{skip:process.platform!=='win32'},t=>{
  const {report,environment}=run(fixture(t,'environment'));
  assert.equal(report.exit_code,0,report.failure);assert.equal(report.completion,'natural');assert.equal(report.msvc_service_policy,null);
  assert.deepEqual(report.planned_service_cleanup,[]);
  assert.deepEqual(environment,{endpoint:'owner-endpoint-sentinel',options:'owner-options-sentinel'});
});

test('a declared service cannot turn a failed child into planned cleanup success',{skip:process.platform!=='win32'},t=>{
  const {report}=run(fixture(t,'nonzero'));
  assert.equal(report.process_exit_code,7);assert.equal(report.broker_exit_code,7);assert.equal(report.exit_code,1);
  assert.equal(report.completion,'failed');assert.equal(report.forced_cleanup,true);assert.deepEqual(report.planned_service_cleanup,[]);
  assert.match(report.failure,/within 10 seconds/);
});
