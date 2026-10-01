// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const {spawn, spawnSync} = require('node:child_process');
const helper = path.resolve(__dirname, '../../../scripts/release/build-progress.ps1');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

function fixture(t, mode, seconds = 20, mirror = false) {
  const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-build-progress-')));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  const child = path.join(root, 'child.cjs'), runner = path.join(root, 'run.ps1'), input = path.join(root, 'input.json');
  fs.writeFileSync(child, `const fs=require('node:fs'),{spawn}=require('node:child_process');
const mode=process.argv[2];
if(mode==='quiet'){setTimeout(()=>{},2500);}
else if(mode==='nonzero'){console.log(JSON.stringify({args:process.argv.slice(3),cwd:process.cwd()}));console.error('retained diagnostic');process.exitCode=7;}
else if(mode==='noise'){
 for(let id=0;id<128;id++){
  const row=Buffer.from(JSON.stringify({reason:'compiler-artifact',id,text:'é🙂'.repeat(id===50?16000:10)}));
  const half=Math.floor(row.length/2);process.stdout.write(row.subarray(0,half));
  process.stderr.write('diagnostic '+id+' café\\n');process.stdout.write(row.subarray(half));process.stdout.write('\\n');
 }
 process.stderr.write('final partial diagnostic');
}
else if(mode==='descendants'||mode==='retained-pipe'){
 // Detachment avoids Node's own parent-exit cleanup for this one fixture;
 // Windows still inherits the supervisor Job, which disallows breakaway.
 const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'inherit',windowsHide:true,detached:mode==='retained-pipe'});
 fs.writeFileSync(process.argv[3],JSON.stringify({parent:process.pid,child:child.pid}));
 if(mode==='retained-pipe')setTimeout(()=>process.exit(0),50);else setInterval(()=>{},1000);
}
`);
  const args = ['descendants','retained-pipe'].includes(mode) ? [path.join(root, 'pids.json')] : ['space café', 'embedded"quote', 'trailing\\'];
  const executable = mode === 'missing' ? path.join(root, 'missing-program.exe') : fs.realpathSync.native(process.execPath);
  fs.writeFileSync(input, JSON.stringify({node: executable, args: [child, mode, ...args], root, seconds, mirror}));
  fs.writeFileSync(runner, `param([string]$Helper,[string]$InputFile)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
. $Helper
$settings=Get-Content -LiteralPath $InputFile -Raw | ConvertFrom-Json
$result=Invoke-VcpBuildProcess -Executable $settings.node -Arguments $settings.args -WorkingDirectory $settings.root -LogPath (Join-Path $settings.root 'build.log') -ProgressSeconds 1 -TimeoutSeconds $settings.seconds -MirrorOutput:$settings.mirror
Write-Output ('VCP_TEST_RESULT '+($result | ConvertTo-Json -Compress))
`);
  return {root, args, command: ['-NoProfile', '-NonInteractive', '-File', runner, '-Helper', helper, '-InputFile', input]};
}
function run(f) {
  const child = spawnSync('pwsh', f.command, {encoding: 'utf8', windowsHide: true, timeout: 30000, maxBuffer: 4 * 1024 * 1024});
  assert.ifError(child.error); assert.equal(child.status, 0, child.stderr + child.stdout);
  const lines = child.stdout.split(/\r?\n/), result = lines.find(line => line.startsWith('VCP_TEST_RESULT '));
  assert(result, child.stdout); const report = JSON.parse(result.slice('VCP_TEST_RESULT '.length));
  const snapshot = JSON.parse(fs.readFileSync(path.join(f.root, 'build-progress.json'), 'utf8'));
  assert.equal(report.child_exit_observation_removed,true);
  assert.deepEqual(fs.readdirSync(f.root).filter(name=>name.startsWith('.vcp-child-exit-')),[]);
  assert.equal(snapshot.schema, 'vcp-build-progress/1'); assert.equal(snapshot.phase, 'cargo');
  assert.equal(snapshot.output_bytes, report.output_bytes); assert.equal(snapshot.compiler_artifacts, report.compiler_artifacts);
  assert.equal(typeof snapshot.job_cpu_seconds,'number'); assert(snapshot.job_cpu_seconds>0);
  assert.equal(typeof snapshot.job_peak_committed_memory_bytes,'number'); assert(snapshot.job_peak_committed_memory_bytes>0);
  assert.equal(snapshot.job_cpu_seconds,report.job_cpu_seconds); assert.equal(snapshot.job_peak_committed_memory_bytes,report.job_peak_committed_memory_bytes);
  assert(snapshot.logical_processor_count>=1); assert.match(snapshot.resource_measurement,/cumulative user\+kernel CPU/);
  for (const line of lines.filter(line => line.startsWith('VCP_BUILD_PROGRESS ')))
    assert.match(line, /^VCP_BUILD_PROGRESS elapsed_seconds=\d+ output_bytes=\d+ idle_seconds=\d+ compiler_artifacts=\d+$/);
  return {report, snapshot, lines, log: fs.readFileSync(path.join(f.root, 'build.log'), 'utf8')};
}
function running(pid) { try { process.kill(pid, 0); return true; } catch (error) { if(error.code === 'ESRCH') return false; throw error; } }
async function waitUntil(condition, milliseconds=10000) {
  const end=Date.now()+milliseconds;
  while(Date.now()<end){if(condition())return;await delay(30);}
  assert(condition(), 'Owned child condition did not complete before deadline');
}

test('build supervisor preserves nonzero exit, argument boundaries and diagnostic text', {skip:process.platform!=='win32'}, t => {
  const f=fixture(t,'nonzero'), {report,snapshot,log}=run(f);
  assert.equal(report.exit_code,7); assert.equal(report.process_exit_code,7); assert.equal(report.broker_exit_code,7);
  assert.equal(report.forced_cleanup,false); assert.equal(report.job_active_processes_zero,true); assert.equal(snapshot.status,'fail');
  const row=log.split('\n').find(line=>line.startsWith('{'));
  assert.deepEqual(JSON.parse(row),{args:f.args,cwd:f.root}); assert.match(log,/^retained diagnostic$/m);
  assert.equal(log.trimEnd().split('\n').length,2); assert.equal(report.output_bytes,Buffer.byteLength(log));
});

test('quiet active builds emit truthful zero-output heartbeats', {skip:process.platform!=='win32'}, t => {
  const {report,snapshot,lines,log}=run(fixture(t,'quiet'));
  assert.equal(report.exit_code,0); assert.equal(report.output_bytes,0); assert.equal(report.compiler_artifacts,0);
  assert.equal(report.process_exit_code,0); assert.equal(report.broker_exit_code,0);
  assert.equal(log,''); assert.equal(snapshot.status,'pass');
  assert(lines.some(line=>/^VCP_BUILD_PROGRESS elapsed_seconds=[2-9]\d* output_bytes=0 idle_seconds=[2-9]\d* compiler_artifacts=0$/.test(line)));
});

test('simultaneous split stdout JSON and stderr stay completely framed and mirror only when selected', {skip:process.platform!=='win32'}, t => {
  const {report,snapshot,lines,log}=run(fixture(t,'noise',20,true));
  assert.equal(report.exit_code,0); assert.equal(report.compiler_artifacts,128); assert.equal(snapshot.status,'pass');
  const rows=log.trimEnd().split('\n'), json=rows.filter(line=>line.startsWith('{')).map(line=>JSON.parse(line));
  assert.equal(rows.length,257,JSON.stringify(rows.filter(line=>!line.startsWith('{')&&!line.startsWith('diagnostic ')&&line!=='final partial diagnostic'))); assert.equal(json.length,128);
  assert.deepEqual(json.map(row=>row.id),Array.from({length:128},(_,id)=>id));
  for(let id=0;id<128;id++){assert(rows.includes('diagnostic '+id+' café'));assert.equal(json[id].text,'é🙂'.repeat(id===50?16000:10));}
  assert(rows.includes('final partial diagnostic')); assert(lines.includes('diagnostic 127 café'),JSON.stringify(lines.filter(line=>line.includes('diagnostic 127'))));
  assert.equal(report.output_bytes,Buffer.byteLength(log)-1,'Only final unterminated diagnostic receives a framing newline');
});

test('deadline terminates the contained parent and descendant and records failure', {skip:process.platform!=='win32'}, t => {
  const f=fixture(t,'descendants',3), {report,snapshot}=run(f);
  assert.equal(report.exit_code,1);assert.equal(report.timed_out,true);assert.equal(report.forced_cleanup,true);
  assert.equal(report.process_exit_code,null,'Termination must not invent an observed child exit');
  assert.equal(report.job_active_processes_zero,true);assert.equal(snapshot.status,'fail');
  const pids=JSON.parse(fs.readFileSync(path.join(f.root,'pids.json'),'utf8'));
  assert.equal(running(pids.parent),false);assert.equal(running(pids.child),false);
});

test('a successful child exit stays distinct from a broker pipe-drain failure', {skip:process.platform!=='win32'}, t => {
  const f=fixture(t,'retained-pipe',25), {report,snapshot}=run(f);
  assert.equal(report.process_exit_code,0); assert.notEqual(report.broker_exit_code,0); assert.notEqual(report.broker_exit_code,null);
  assert.equal(report.exit_code,1); assert.equal(report.forced_cleanup,true); assert.equal(report.timed_out,false);
  assert.equal(report.job_active_processes_zero,true); assert.equal(snapshot.status,'fail');
  const pids=JSON.parse(fs.readFileSync(path.join(f.root,'pids.json'),'utf8'));
  assert.equal(running(pids.parent),false); assert.equal(running(pids.child),false);
});

test('a failed spawn records an unknown child exit and the observed broker failure', {skip:process.platform!=='win32'}, t => {
  const {report,snapshot,log}=run(fixture(t,'missing'));
  assert.equal(report.process_exit_code,null); assert.notEqual(report.broker_exit_code,0); assert.notEqual(report.broker_exit_code,null);
  assert.equal(report.exit_code,1); assert.equal(report.forced_cleanup,false);
  assert.equal(report.job_active_processes_zero,true); assert.equal(snapshot.status,'fail');
  assert.match(log,/missing-program\.exe/);
});

test('abrupt owner interruption closes its Job and removes only owned fake descendants', {skip:process.platform!=='win32'}, async t => {
  const f=fixture(t,'descendants',30);
  const owner=spawn('pwsh',f.command,{windowsHide:true,stdio:['ignore','pipe','pipe']});
  owner.stdout.resume();owner.stderr.resume();
  t.after(()=>{if(owner.exitCode===null)owner.kill();});
  const exited=new Promise((resolve,reject)=>{owner.once('exit',resolve);owner.once('error',reject);});
  await waitUntil(()=>fs.existsSync(path.join(f.root,'pids.json')));
  const pids=JSON.parse(fs.readFileSync(path.join(f.root,'pids.json'),'utf8'));
  assert(running(pids.parent));assert(running(pids.child));owner.kill();await exited;
  await waitUntil(()=>!running(pids.parent)&&!running(pids.child));
  await waitUntil(()=>!fs.readdirSync(f.root).some(name=>name.startsWith('.vcp-child-exit-')));
  const snapshot=JSON.parse(fs.readFileSync(path.join(f.root,'build-progress.json'),'utf8'));
  assert.equal(snapshot.status,'running','Abrupt interruption must not invent successful completion');
});
