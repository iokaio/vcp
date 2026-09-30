// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'), assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const {execFileSync}=require('node:child_process');
const evidence=require('../../../scripts/release/evidence.cjs');
const p=require('../../../scripts/release/provenance.cjs');
const commit='a'.repeat(40), digest=p.hash('fixture');
function fixture(t, artifacts=false) {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-beta-evidence-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const file=(name,bytes)=>{const target=path.join(root,name);fs.writeFileSync(target,bytes);return target};
  const log=file('stage.log','controlled synthetic evidence\n');
  const run={schema:'vcp-candidate-run/1',status:'pass',reviewed_commit:commit,environment:{os:'Windows test fixture',node:'24.10.0'},
    stages:evidence.stages.map(id=>({id,status:'pass',command:['synthetic',id],exit_code:0,expected:'fixture only',log})),receipts:{}};
  if(artifacts){
    const selected=p.channel(path.resolve(__dirname,'../../..'));
    const files=[{path:'src/third_party/codex/codex-rs/Cargo.lock',bytes:7,sha256:digest},
      {path:'src/third_party/codex/codex-rs/.cargo/config.toml',bytes:7,sha256:digest},
      {path:'release/internal-beta.json',bytes:7,sha256:selected.config_sha256}];
    const source={schema:'vcp-release-source/1',commit,dirty:false,files,content_sha256:p.hash(JSON.stringify(files))};
    const dependencies={schema:'vcp-release-dependencies/1',status:'verified',target:'x86_64-pc-windows-msvc',components:1,workspace_lock_sha256:digest,inventory_sha256:digest};
    file('source-before.json',JSON.stringify(source));file('source-after.json',JSON.stringify(source));
    const dependencyFile=file('dependencies-before.json',JSON.stringify(dependencies));file('dependencies-after.json',JSON.stringify(dependencies));
    const release=p.releaseIdentity(selected,source,digest);
    const artifact=name=>({reason:'compiler-artifact',target:{name},features:[],profile:{test:false,opt_level:'3'},package_id:'path+file:///source#vcp-cli@'+selected.native_version});
    const compiler=artifact('vcp'),launcher=artifact('vcp-launch');
    const buildLog=file('build.log',JSON.stringify(compiler)+'\n'+JSON.stringify(launcher)+'\n');
    const upstream=file('upstream-verification.log',JSON.stringify({status:'pass',component:'codex',files:1,files_sha256:digest}));
    file('upstream-verification-after.log',fs.readFileSync(upstream));file('vcp.exe','fixture');file('vcp-launch.exe','fixture');
    const build=file('build.json',JSON.stringify({schema:'vcp-local-build/1',exit_code:0,cargo_exit_code:0,source_commit:commit,source_content_sha256:source.content_sha256,inputs:files,
      source_dirty:false,source_stable:true,toolchain_stable:true,qualification_build:false,profile:'release',target:dependencies.target,
      executable_sha256:digest,executable_version:selected.native_version,executable_target:selected.target,release,
      launcher_sha256:digest,launcher_version:selected.native_version,launcher_target:selected.target,
      compiler_artifact:compiler,launcher_compiler_artifact:launcher,vcp_features:[{target:'vcp',features:[]}],
      command:['cargo','+1.95.0','build','--locked','--offline','--release','--no-default-features','-p','vcp-cli','--bin','vcp','--bin','vcp-launch','--target',selected.target,'--target-dir','/output/cargo-target','-j','2','--message-format=json-render-diagnostics'],
      rustflags:['-C','link-arg=/STACK:8388608','-C','target-feature=+crt-static'],rustc:['release: 1.95.0'],
      native_tools:['cl','link','lib','cmake','ninja','rustc','cargo','node'].map(name=>({name,sha256:digest})),cargo_configs:[{sha256:digest}],
      log_sha256:p.fileHash(buildLog),upstream_before_sha256:p.fileHash(upstream),upstream_after_sha256:p.fileHash(upstream),
      dependency_sources_stable:true,dependency_source:dependencies,dependencies_before_sha256:p.fileHash(dependencyFile),dependencies_after_sha256:p.fileHash(dependencyFile)})),buildHash=p.fileHash(build);
    const native={schema:'vcp-distribution-result/1',status:'release-candidate',package:'native.zip',archive_sha256:digest,manifest:{release,source:{dirty:false},build:{status:'verified-release-build',receipt_sha256:buildHash},files:[{path:'vcp.exe',sha256:digest}]}};
    const nativeFile=file('native.json',JSON.stringify(native));
    const vsix={schema:'vcp-vsix-package/1',release,archive:{file:'editor.vsix',sha256:digest},extension:{version:'0.2.1',source:{git_commit:commit,dirty:false}},sdk:{version:'0.2.1'},engine:{native_archive_sha256:digest,executable_sha256:digest,build_receipt_sha256:buildHash,source_commit:commit,source_dirty:false,native_manifest_sha256:p.fileHash(nativeFile)}};
    const setup={schema:'vcp-setup-result/1',candidate_id:release.candidate_id,native_archive_sha256:digest,archive:{file:'setup.exe',sha256:digest},build_receipt_sha256:buildHash};
    for(const name of ['native.zip','editor.vsix','setup.exe'])file(name,'fixture');
    run.receipts={build,native:nativeFile,vsix:file('vsix.json',JSON.stringify(vsix)),setup:file('setup.json',JSON.stringify(setup))};
  }
  return {root,run,write:()=>file('run.json',JSON.stringify(run)),file};
}
test('pipeline success never fills installed/manual matrix or grants acceptance',t=>{
  const f=fixture(t,true),result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.pipeline_status,'pass');assert.equal(result.status,'qualification-required');
  assert.equal(result.matrix.find(row=>row.id==='production-identity').status,'pass');
  assert(result.matrix.filter(row=>row.id!=='production-identity').every(row=>row.status==='not run'));
  assert.equal(result.publication.authorized,false);assert.equal(result.publication.owner_acceptance,'not run');
  for(const row of result.files)assert.equal(p.fileHash(path.join(f.root,'packet',row.path)),row.sha256);
  assert(fs.existsSync(path.join(f.root,'packet/SHA256SUMS')));
});
test('stage success cannot replace complete production compiler and retained artifact evidence',t=>{
  const rewriteBuild=(f,mutate)=>{
    const build=JSON.parse(fs.readFileSync(f.run.receipts.build));mutate(build,f);
    f.file('build.json',JSON.stringify(build));const buildHash=p.fileHash(f.run.receipts.build);
    const native=JSON.parse(fs.readFileSync(f.run.receipts.native));native.manifest.build.receipt_sha256=buildHash;f.file('native.json',JSON.stringify(native));
    const vsix=JSON.parse(fs.readFileSync(f.run.receipts.vsix));vsix.engine.build_receipt_sha256=buildHash;vsix.engine.native_manifest_sha256=p.fileHash(f.run.receipts.native);f.file('vsix.json',JSON.stringify(vsix));
    const setup=JSON.parse(fs.readFileSync(f.run.receipts.setup));setup.build_receipt_sha256=buildHash;f.file('setup.json',JSON.stringify(setup));
  };
  for(const change of [
    b=>{delete b.compiler_artifact}, b=>{delete b.log_sha256}, b=>{b.qualification_build=true},
    b=>{b.cargo_exit_code=1}, b=>{for(const key of Object.keys(b))if(!['schema','exit_code'].includes(key))delete b[key]},
    (b,f)=>{fs.unlinkSync(path.join(f.root,'build.log'))},
    (b,f)=>{f.file('build.log',JSON.stringify(b.compiler_artifact)+'\n');b.log_sha256=p.fileHash(path.join(f.root,'build.log'))},
    (b,f)=>{f.file('vcp-launch.exe','changed executable')},
  ]){
    const f=fixture(t,true);rewriteBuild(f,change);
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.pipeline_status,'fail');assert.equal(result.matrix[0].status,'not run');
    assert(result.validation_failures.length>0);assert(result.files.some(row=>row.path==='receipts/build.json'));
  }
});
test('missing setup, interrupted stages and changed final bytes fail closed while retaining evidence',t=>{
  for(const change of [f=>{delete f.run.receipts.setup},f=>{f.run.stages[2].status='running'},f=>{f.file('editor.vsix','changed')},f=>{f.run.reviewed_commit='b'.repeat(40)},f=>{delete f.run.receipts.build}]){
    const f=fixture(t,true);change(f);const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.pipeline_status,'fail');assert.equal(result.publication.authorized,false);
    assert(result.files.some(row=>row.path==='logs/source-gate.log'));
  }
});
test('failure before construction records every unrun stage and redacts actual token bytes',t=>{
  const f=fixture(t);f.run.status='fail';f.run.stages=[];
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.pipeline_status,'fail');assert(result.observations.every(row=>row.status==='not run'));
  assert(result.matrix.every(row=>row.status==='not run'));
  assert.equal(evidence.sanitize('secret-value-long Authorization: Bearer opaque-value',['secret-value-long']),'[REDACTED] Authorization: Bearer [REDACTED]');
  assert.throws(()=>evidence.packet(f.write(),path.join(f.root,'packet')),/new directory/);
  assert.throws(()=>execFileSync(process.execPath,[path.resolve(__dirname,'../../../scripts/release/evidence.cjs'),f.write(),path.join(f.root,'cli-packet')],{stdio:'pipe'}),error=>error.status===1);
  assert.equal(JSON.parse(fs.readFileSync(path.join(f.root,'cli-packet/evidence.json'))).pipeline_status,'fail');
});
test('fabricated successful stage without a log or with duplicate identity is rejected',t=>{
  const f=fixture(t);f.run.stages[0].exit_code=1;assert.throws(()=>evidence.validateRun(f.run),/Invalid/);
  f.run.stages[0].exit_code=0;delete f.run.stages[0].log;assert.throws(()=>evidence.validateRun(f.run),/Invalid/);
  f.run.stages=[];const row={id:'source-gate',status:'not run',command:[]};f.run.stages=[row,row];assert.throws(()=>evidence.validateRun(f.run),/duplicate/);
});
test('deleted or changed before/after inventories cannot retain production identity success',t=>{
  for(const name of ['source-before.json','source-after.json','dependencies-before.json','dependencies-after.json']){
    for(const remove of [true,false]){
      const f=fixture(t,true);
      if(remove)fs.unlinkSync(path.join(f.root,name));else f.file(name,JSON.stringify({unexpected:'changed retained evidence'}));
      const result=evidence.packet(f.write(),path.join(f.root,'packet'));
      assert.equal(result.pipeline_status,'fail');assert.equal(result.matrix[0].status,'not run');
      assert(result.validation_failures.some(row=>row.includes(name)));
      assert(result.files.some(row=>row.path==='receipts/build.json'));
      assert(result.files.some(row=>row.path==='logs/production-build.log'));
    }
  }
});
test('Windows candidate child capture preserves outputs, excludes credentials and enforces a deadline',{skip:process.platform!=='win32'},t=>{
  const f=fixture(t);
  const script=f.file('process-test.ps1',`param([string]$Helper,[string]$Node,[string]$Root)
$ErrorActionPreference='Stop'
. $Helper
$env:VCP_SECRET='must-not-enter-candidate'
$result=Invoke-BetaProcess $Node @('-e','if(process.env.VCP_SECRET)process.exit(9);process.stdout.write("hello");process.stderr.write("diagnostic")') $Root
if ($result.stdout -cne 'hello' -or $result.stderr -cne 'diagnostic') { throw 'Output differs' }
$refused=$false
try { $null=Invoke-BetaProcess $Node @('-e','setInterval(()=>{},1000)') $Root @{} 1 }
catch { if ($_.Exception.Message -notmatch 'exceeded time or output limit') { throw }; $refused=$true }
if (-not $refused) { throw 'Timeout accepted' }
`);
  execFileSync('pwsh',['-NoProfile','-File',script,'-Helper',path.resolve(__dirname,'../../../scripts/release/candidate-runtime.ps1'),'-Node',process.execPath,'-Root',f.root],{windowsHide:true,timeout:20000,stdio:'pipe'});
});
