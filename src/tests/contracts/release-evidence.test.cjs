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
    file('upstream-verification-after.log',fs.readFileSync(upstream));file('vcp.exe','fixture');file('vcp-launch.exe','launcher fixture');
    const build=file('build.json',JSON.stringify({schema:'vcp-local-build/1',exit_code:0,cargo_exit_code:0,source_commit:commit,source_content_sha256:source.content_sha256,inputs:files,
      source_dirty:false,source_stable:true,toolchain_stable:true,qualification_build:false,profile:'release',target:dependencies.target,
      executable:path.join(root,'vcp.exe'),executable_sha256:digest,executable_version:selected.native_version,executable_target:selected.target,release,
      launcher:path.join(root,'vcp-launch.exe'),launcher_sha256:p.hash('launcher fixture'),launcher_version:selected.native_version,launcher_target:selected.target,
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
  assert.equal(result.selection_status,'pass');assert.equal(result.stop_after,'installed-editor');
  assert.equal(result.matrix.find(row=>row.id==='production-identity').status,'pass');
  assert(result.matrix.filter(row=>row.id!=='production-identity').every(row=>row.status==='not run'));
  assert.equal(result.publication.authorized,false);assert.equal(result.publication.owner_acceptance,'not run');
  for(const row of result.files)assert.equal(p.fileHash(path.join(f.root,'packet',row.path)),row.sha256);
  assert(fs.existsSync(path.join(f.root,'packet/SHA256SUMS')));
});
function selectPrefix(f,stop){
  f.run.stop_after=stop;f.run.stages=f.run.stages.slice(0,evidence.stages.indexOf(stop)+1);
  if(stop==='portable-contracts')f.run.receipts={};
  else if(stop==='production-build')f.run.receipts={build:f.run.receipts.build};
  return f;
}
test('explicit successful prefixes pass their selection without claiming an unfinished pipeline or manual qualification',t=>{
  for(const stop of ['portable-contracts','production-build','pair','installed-editor']){
    const f=selectPrefix(fixture(t,stop!=='portable-contracts'),stop);
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.stop_after,stop);assert.equal(result.selection_status,'pass');
    assert.equal(result.pipeline_status,stop==='installed-editor'?'pass':'incomplete');
    assert.deepEqual(result.validation_failures,[]);
    assert.equal(result.matrix[0].status,['pair','installed-editor'].includes(stop)?'pass':'not run');
    assert(result.matrix.slice(1).every(row=>row.status==='not run'));
    assert.equal(result.publication.authorized,false);assert.equal(result.publication.owner_acceptance,'not run');
    assert(result.observations.slice(f.run.stages.length).every(row=>row.status==='not run'&&row.reason.includes('selected stage prefix')));
    const summary=JSON.parse(execFileSync(process.execPath,[path.resolve(__dirname,'../../../scripts/release/evidence.cjs'),f.write(),path.join(f.root,'cli-packet')],{stdio:'pipe',encoding:'utf8'}));
    assert.equal(summary.status,'pass');assert.equal(summary.selection_status,'pass');assert.equal(summary.pipeline_status,result.pipeline_status);
  }
});
test('selected prefixes refuse missing receipts or stage logs, wrong order, extra stages and interruptions',t=>{
  for(const [stop,change] of [
    ['production-build',f=>{delete f.run.receipts.build}],
    ...['native','vsix','setup'].map(name=>['pair',f=>{delete f.run.receipts[name]}]),
    ['portable-contracts',f=>{f.run.stages[1].log=path.join(f.root,'absent.log')}],
    ['portable-contracts',f=>{[f.run.stages[0],f.run.stages[1]]=[f.run.stages[1],f.run.stages[0]]}],
    ['portable-contracts',f=>{f.run.stages.push({...f.run.stages[0],id:'production-build'})}],
    ['portable-contracts',f=>{f.run.stages.pop()}],
    ['production-build',f=>{f.run.stages.at(-1).status='running'}],
    ['portable-contracts',f=>{f.run.status='running'}],
    ['portable-contracts',f=>{f.run.status='fail'}],
  ]){
    const f=selectPrefix(fixture(t,true),stop);change(f);
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
    assert.equal(result.publication.authorized,false);
  }
  const invalid=fixture(t);invalid.run.stop_after='native-package';assert.throws(()=>evidence.validateRun(invalid.run),/Invalid candidate run/);
});
test('selection success requires validated build evidence and cannot hide validation failures',t=>{
  for(const stop of ['production-build','pair','installed-editor']){
    const f=selectPrefix(fixture(t,true),stop);fs.writeFileSync(path.join(f.root,'vcp-launch.exe'),'changed binary');
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
    assert(result.validation_failures.length>0);assert.equal(result.matrix[0].status,'not run');
    assert.throws(()=>execFileSync(process.execPath,[path.resolve(__dirname,'../../../scripts/release/evidence.cjs'),f.write(),path.join(f.root,'cli-packet')],{stdio:'pipe'}),error=>error.status===1);
  }
  const legacy=fixture(t,true);legacy.run.stages.reverse();
  const result=evidence.packet(legacy.write(),path.join(legacy.root,'packet'));
  assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
});
test('completed stage log digests bind final-stage bytes while legacy logs remain readable',t=>{
  for(const change of [
    row=>fs.appendFileSync(row.log,'changed after completion\n'),
    row=>{row.log_sha256=row.log_sha256.toUpperCase()},
    row=>{row.log_sha256=null},
  ]){
    const f=selectPrefix(fixture(t),'portable-contracts'),last=f.run.stages.at(-1);
    last.log=f.file('final-stage.log','completed final stage\n');last.log_sha256=p.fileHash(last.log);
    change(last);
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
    assert.equal(result.observations.find(row=>row.id===last.id).status,'fail');
    assert(result.validation_failures.includes('Changed or invalid stage log digest: portable-contracts'));
    assert.equal(fs.readFileSync(path.join(f.root,'packet/logs/portable-contracts.log'),'utf8'),fs.readFileSync(last.log,'utf8'));
    assert.throws(()=>execFileSync(process.execPath,[path.resolve(__dirname,'../../../scripts/release/evidence.cjs'),f.write(),path.join(f.root,'cli-packet')],{stdio:'pipe'}),error=>error.status===1);
  }
  for(const bound of [false,true]){
    const f=selectPrefix(fixture(t),'portable-contracts');
    if(bound)for(const row of f.run.stages)row.log_sha256=p.fileHash(row.log);
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.selection_status,'pass');assert.deepEqual(result.validation_failures,[]);
  }
});
function failedPackaging(t) {
  const f=fixture(t,true);
  f.run.status='fail';f.run.receipts={build:f.run.receipts.build};
  f.run.stages=f.run.stages.slice(0,evidence.stages.indexOf('native-package')+1);
  Object.assign(f.run.stages.at(-1),{status:'fail',exit_code:1,reason:'Synthetic native packaging failure before result publication'});
  return f;
}
test('successful build retains verified diagnostic executables when packaging produces no final receipts',t=>{
  const f=failedPackaging(t);
  f.file('vcp.pdb','unrequested symbols');f.file('private-profile.json','private fixture');
  const build=JSON.parse(fs.readFileSync(f.run.receipts.build));build.symbols_sha256=p.hash('unrequested symbols');
  fs.writeFileSync(f.run.receipts.build,JSON.stringify(build));
  for(const name of ['cargo-target','cache']){fs.mkdirSync(path.join(f.root,name));f.file(name+'/vcp.exe','unrequested build tree');}
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.deepEqual(result.validation_failures,[]);
  assert.equal(result.pipeline_status,'fail');assert.equal(result.pair_id,null);
  assert(result.matrix.every(row=>row.status==='not run'));
  assert.equal(result.publication.authorized,false);assert.equal(result.publication.owner_acceptance,'not run');
  assert.equal(result.observations.find(row=>row.id==='production-build').status,'pass');
  assert.equal(result.observations.find(row=>row.id==='native-package').status,'fail');
  assert.equal(result.observations.find(row=>row.id==='setup-package').status,'not run');
  const binaries=result.files.filter(row=>row.path.startsWith('build-output/'));
  assert.deepEqual(binaries.map(row=>row.path).sort(),['build-output/vcp-launch.exe','build-output/vcp.exe']);
  const receipt=JSON.parse(fs.readFileSync(path.join(f.root,'packet/receipts/build.json')));
  const sums=fs.readFileSync(path.join(f.root,'packet/SHA256SUMS'),'utf8');
  for(const [name,key] of [['vcp.exe','executable_sha256'],['vcp-launch.exe','launcher_sha256']]){
    const retained=path.join(f.root,'packet/build-output',name),row=binaries.find(row=>row.path.endsWith('/'+name));
    assert.deepEqual(fs.readFileSync(retained),fs.readFileSync(path.join(f.root,name)));
    assert.equal(row.sha256,receipt[key]);assert.equal(p.fileHash(retained),receipt[key]);
    assert(sums.includes(`${receipt[key]}  build-output/${name}\n`));
  }
  assert(!result.files.some(row=>/\.pdb$|private-profile|cargo-target|cache|^artifacts\//.test(row.path)));
  assert(!fs.existsSync(path.join(f.root,'packet/pair.json')));
  assert(result.limitations.some(row=>row.includes('unpackaged diagnostic executables')));
});
test('missing or changed original executables cannot become retained diagnostic build outputs',t=>{
  for(const name of ['vcp.exe','vcp-launch.exe'])for(const action of ['missing','changed']){
    const f=failedPackaging(t),file=path.join(f.root,name);
    if(action==='missing')fs.unlinkSync(file);else fs.writeFileSync(file,'changed executable');
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.pipeline_status,'fail');assert.equal(result.pair_id,null);
    assert(result.validation_failures.some(row=>row.includes(`Original build artifact ${action==='missing'?'unavailable':'differs'}: ${name}`)));
    assert(!result.validation_failures.some(row=>row.includes('Received undefined')));
    assert(!result.files.some(row=>row.path.startsWith('build-output/')));
    assert(result.matrix.every(row=>row.status==='not run'));
  }
});
test('redirected build receipt directory cannot supply hash-matching diagnostic executable copies',t=>{
  const f=failedPackaging(t),originals=path.join(f.root,'originals'),redirected=path.join(f.root,'redirected');
  fs.mkdirSync(originals);
  for(const name of ['build.json','vcp.exe','vcp-launch.exe','source-before.json','source-after.json','dependencies-before.json','dependencies-after.json','build.log','upstream-verification.log','upstream-verification-after.log']){
    fs.copyFileSync(path.join(f.root,name),path.join(originals,name));
  }
  fs.symlinkSync(originals,redirected,process.platform==='win32'?'junction':'dir');
  t.after(()=>{if(fs.existsSync(redirected))fs.unlinkSync(redirected)});
  // Receipt path fields still point at ordinary matching files. The collector
  // must validate fixed siblings and their ancestors, not follow those fields.
  f.run.receipts.build=path.join(redirected,'build.json');
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.pipeline_status,'fail');
  assert(result.validation_failures.some(row=>row.includes('Redirected build output or ancestor')));
  assert(!result.files.some(row=>row.path.startsWith('build-output/')));
  assert(result.matrix.every(row=>row.status==='not run'));
});
test('a present native result must bind the independently validated build even without a final pair',t=>{
  const f=failedPackaging(t),nativeFile=path.join(f.root,'native.json');
  const native=JSON.parse(fs.readFileSync(nativeFile));
  native.manifest.files[0].sha256=p.hash('different executable');fs.writeFileSync(nativeFile,JSON.stringify(native));
  f.run.receipts.native=nativeFile;
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.pipeline_status,'fail');assert.equal(result.pair_id,null);
  assert(result.validation_failures.some(row=>row.includes('Native result differs from validated production build')));
  assert(!result.files.some(row=>row.path.startsWith('build-output/')));
  assert(result.matrix.every(row=>row.status==='not run'));
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
    assert(!result.files.some(row=>row.path.startsWith('build-output/')));
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
test('failed builders retain exact public log names from their actual GUID directory formats only',t=>{
  const f=fixture(t);f.run.status='fail';f.run.stages=[];
  const buildId='01234567-89ab-cdef-0123-456789abcdef',setupId='0123456789abcdef0123456789abcdef';
  const expected=[];
  for(const [stage,id,names] of [
    ['build',buildId,['build.log','upstream-verification.log','upstream-verification-after.log']],
    ['setup',setupId,['setup-build.log','compiler-provision.log']],
  ]){
    const directory=path.join(f.root,stage,id);fs.mkdirSync(directory,{recursive:true});
    for(const name of names){fs.writeFileSync(path.join(directory,name),'public builder failure');expected.push(`logs/${stage}-${id}-${name}`)}
    fs.writeFileSync(path.join(directory,'private-profile.json'),'must remain private');
    fs.mkdirSync(path.join(directory,'private-state'));fs.writeFileSync(path.join(directory,'private-state',names[0]),'must not recurse');
  }
  for(const [stage,id,name] of [
    ['build',setupId,'build.log'],['setup',buildId,'setup-build.log'],
    ['build','-'.repeat(36),'build.log'],['setup','g'.repeat(32),'compiler-provision.log'],
  ]){
    const directory=path.join(f.root,stage,id);fs.mkdirSync(directory,{recursive:true});fs.writeFileSync(path.join(directory,name),'unrecognized directory');
  }
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.pipeline_status,'fail');assert(result.matrix.every(row=>row.status==='not run'));
  assert.deepEqual(result.files.map(row=>row.path).sort(),expected.sort());
});
test('interrupted builds retain only whitelisted structured pre-build diagnostics without granting success',t=>{
  const f=selectPrefix(fixture(t),'production-build');f.run.status='running';f.run.receipts={};
  f.run.stages.at(-1).status='running';delete f.run.stages.at(-1).exit_code;
  const id='01234567-89ab-cdef-0123-456789abcdef',directory=path.join(f.root,'build',id);
  fs.mkdirSync(directory,{recursive:true});
  const value='Authorization: Bearer synthetic-sensitive-value';
  const names=['source-before.json','dependencies-before.json','build-progress.json'];
  for(const name of names)fs.writeFileSync(path.join(directory,name),JSON.stringify({schema:'diagnostic-fixture',nested:{text:value},quoted:'a "quoted" string',status:'pass'}));
  for(const name of ['source-after.json','dependencies-after.json','private-profile.json'])fs.writeFileSync(path.join(directory,name),'private fixture');
  fs.mkdirSync(path.join(directory,'private-state'));fs.writeFileSync(path.join(directory,'private-state','build-progress.json'),'private nested contents');
  const wrong=path.join(f.root,'build','unrecognized');fs.mkdirSync(wrong);fs.writeFileSync(path.join(wrong,'build-progress.json'),'private wrong directory');
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
  assert.deepEqual(result.validation_failures,[]);assert(result.matrix.every(row=>row.status==='not run'));
  assert.equal(result.build_diagnostics.length,3);
  for(const name of names){
    const relative=`diagnostics/build-${id}-${name}`,diagnostic=result.build_diagnostics.find(row=>row.path===relative);
    assert.equal(diagnostic.status,'unverified diagnostic');assert.equal(diagnostic.contributes_to_success,false);
    const bytes=fs.readFileSync(path.join(f.root,'packet',relative),'utf8'),retained=JSON.parse(bytes);
    assert.equal(retained.nested.text,'Authorization: Bearer [REDACTED]');assert.equal(retained.quoted,'a "quoted" string');
    assert(!bytes.includes('synthetic-sensitive-value'));
    assert(result.log_transformations.some(row=>row.path===relative&&row.sanitized));
  }
  assert(!result.files.some(row=>/private|unrecognized|source-after|dependencies-after/.test(row.path)));
  assert(!result.files.some(row=>row.path.startsWith('build-output/')||row.path==='receipts/build.json'));
});
test('malformed or redirected build diagnostics cannot make a selected prefix green',t=>{
  for(const redirected of [false,true]){
    const f=selectPrefix(fixture(t),'portable-contracts'),id='01234567-89ab-cdef-0123-456789abcdef';
    const root=path.join(f.root,redirected?'outside':'build'),directory=path.join(root,id);fs.mkdirSync(directory,{recursive:true});
    fs.writeFileSync(path.join(directory,'build-progress.json'),redirected?'{}':'malformed JSON');
    if(redirected){
      const link=path.join(f.root,'build');fs.symlinkSync(root,link,process.platform==='win32'?'junction':'dir');
      t.after(()=>{if(fs.existsSync(link))fs.unlinkSync(link)});
    }
    const result=evidence.packet(f.write(),path.join(f.root,'packet'));
    assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
    assert(result.validation_failures.some(row=>row.startsWith('build diagnostics:')));
    assert.equal(result.build_diagnostics.length,0);
    assert(!result.files.some(row=>row.path.startsWith('diagnostics/')));
  }
});
test('a partially written input diagnostic does not discard the other interrupted build observations',t=>{
  const f=selectPrefix(fixture(t),'production-build');f.run.status='running';f.run.receipts={};f.run.stages.at(-1).status='running';
  const id='01234567-89ab-cdef-0123-456789abcdef',directory=path.join(f.root,'build',id);fs.mkdirSync(directory,{recursive:true});
  fs.writeFileSync(path.join(directory,'source-before.json'),'{"incomplete":');
  fs.writeFileSync(path.join(directory,'dependencies-before.json'),JSON.stringify({status:'verified',schema:'diagnostic-only'}));
  fs.writeFileSync(path.join(directory,'build-progress.json'),JSON.stringify({status:'running',phase:'cargo',elapsed_seconds:30}));
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
  assert(result.validation_failures.some(row=>row.includes('source-before.json')));
  assert.equal(result.build_diagnostics.length,2);
  assert(result.build_diagnostics.some(row=>row.path.endsWith('build-progress.json')));
  assert(result.matrix.every(row=>row.status==='not run'));
});
test('native qualification progress is sanitized diagnostic evidence with bounded ordinary input',t=>{
  const f=fixture(t);f.run.status='running';f.run.stages=f.run.stages.slice(0,9);f.run.stages.at(-1).status='running';
  f.file('native-qualification-progress.json',JSON.stringify({schema:'vcp-build-progress/1',phase:'cargo',status:'running',job_cpu_seconds:12,note:'Authorization: Bearer synthetic-sensitive-value'}));
  const lateLog='late cleanup diagnostic\nAuthorization: Bearer synthetic-sensitive-value\n';
  f.file('native-qualification.log',lateLog);f.file('unrecognized-progress.json','private unrelated contents');
  fs.mkdirSync(path.join(f.root,'private-fixture'));f.file('private-fixture/native-qualification.log','private nested output');
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.selection_status,'fail');assert.equal(result.pipeline_status,'fail');
  assert.deepEqual(result.build_diagnostics,[{path:'diagnostics/native-qualification-progress.json',status:'unverified diagnostic',contributes_to_success:false}]);
  const retained=JSON.parse(fs.readFileSync(path.join(f.root,'packet/diagnostics/native-qualification-progress.json'),'utf8'));
  assert.equal(retained.job_cpu_seconds,12);assert.equal(retained.note,'Authorization: Bearer [REDACTED]');
  assert.equal(fs.readFileSync(path.join(f.root,'packet/logs/native-qualification.log'),'utf8'),'late cleanup diagnostic\nAuthorization: Bearer [REDACTED]\n');
  assert(result.log_transformations.some(row=>row.path==='logs/native-qualification.log'&&row.original_sha256===p.hash(Buffer.from(lateLog))&&row.sanitized));
  assert(!result.files.some(row=>row.path.includes('private-fixture')||row.path.includes('unrecognized-progress')));
  assert(result.matrix.every(row=>row.status==='not run'));
  for(const mode of ['malformed','oversized','redirected']){
    const invalid=selectPrefix(fixture(t),'portable-contracts');let runFile=invalid.write();
    if(mode==='redirected'){
      const outside=path.join(invalid.root,'outside');fs.mkdirSync(outside);
      fs.copyFileSync(runFile,path.join(outside,'run.json'));fs.writeFileSync(path.join(outside,'native-qualification-progress.json'),'{}');
      const alias=path.join(invalid.root,'alias');fs.symlinkSync(outside,alias,process.platform==='win32'?'junction':'dir');
      t.after(()=>{if(fs.existsSync(alias))fs.unlinkSync(alias)});runFile=path.join(alias,'run.json');
    }else invalid.file('native-qualification-progress.json',mode==='malformed'?'incomplete':Buffer.alloc(32*1024*1024+1,32));
    const refused=evidence.packet(runFile,path.join(invalid.root,'packet'));
    assert.equal(refused.selection_status,'fail');assert.equal(refused.pipeline_status,'fail');
    assert(refused.validation_failures.some(row=>row.startsWith('native qualification diagnostics:')));
    assert.deepEqual(refused.build_diagnostics,[]);
  }
});

test('redirected native qualification log is rejected without retaining its target',t=>{
  const f=selectPrefix(fixture(t),'portable-contracts'),runFile=f.write();
  const outside=path.join(f.root,'outside');fs.mkdirSync(outside);
  fs.copyFileSync(runFile,path.join(outside,'run.json'));fs.writeFileSync(path.join(outside,'native-qualification.log'),'private redirected output');
  const alias=path.join(f.root,'alias');fs.symlinkSync(outside,alias,process.platform==='win32'?'junction':'dir');
  t.after(()=>{if(fs.existsSync(alias))fs.unlinkSync(alias)});
  const result=evidence.packet(path.join(alias,'run.json'),path.join(f.root,'packet'));
  assert.equal(result.selection_status,'fail');
  assert(result.validation_failures.some(row=>row.startsWith('native qualification log:')));
  assert(!result.files.some(row=>row.path==='logs/native-qualification.log'));
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

test('failed real harness retains sanitized child diagnostics without copying private fixtures',async t=>{
  const f=fixture(t);f.run.status='fail';f.run.stages=[];
  const {runSuite}=require('../support/harness.cjs');
  const registry={schema_version:1,suites:{fast:['diagnostic']},cases:{diagnostic:{
    task_ids:['BETA-08'],backends:['none'],requires:[],timeout_ms:10000,max_output_bytes:4096,
    args:['-e','console.log("synthetic stdout");console.error("Authorization: Bearer synthetic-sensitive-value");process.exitCode=1'],
  }}};
  const actual=await runSuite({root:f.root,registry,selection:{suite:'fast',ids:['diagnostic'],backends:['none']},
    outputRoot:path.join(f.root,'contracts'),source:{commit,dirty:false}});
  assert.equal(actual.manifest.status,'fail');
  const directory=path.dirname(actual.manifestPath), attempt=actual.manifest.attempts[0];
  fs.writeFileSync(path.join(directory,'private-profile.json'),'private contents');
  fs.mkdirSync(path.join(directory,'private-state'));
  fs.writeFileSync(path.join(directory,'private-state',attempt.attempt_id+'-stdout.log'),'private nested contents');
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.pipeline_status,'fail');assert.deepEqual(result.validation_failures,[]);
  assert.equal(result.files.length,3);assert(result.files.every(row=>row.path.startsWith('contracts/'+actual.manifest.run_id+'/')));
  const retainedManifest=JSON.parse(fs.readFileSync(path.join(f.root,'packet/contracts',actual.manifest.run_id,'manifest.json'),'utf8'));
  assert.equal(retainedManifest.run_id,actual.manifest.run_id);
  assert.match(retainedManifest.attempts[0].command.at(-1),/Authorization: Bearer \[REDACTED\]/);
  assert(!JSON.stringify(retainedManifest).includes('synthetic-sensitive-value'));
  const retained=fs.readFileSync(path.join(f.root,'packet/contracts',actual.manifest.run_id,attempt.attempt_id+'-stderr.log'),'utf8');
  assert.match(retained,/Authorization: Bearer \[REDACTED\]/);assert(!retained.includes('synthetic-sensitive-value'));
  assert(result.log_transformations.some(row=>row.sanitized));
  fs.appendFileSync(path.join(directory,attempt.attempt_id+'-stdout.log'),'changed after capture');
  const changed=evidence.packet(f.write(),path.join(f.root,'changed'));
  assert(changed.validation_failures.some(row=>row.includes('Retained log changed')));
  fs.unlinkSync(path.join(directory,attempt.attempt_id+'-stderr.log'));
  const missing=evidence.packet(f.write(),path.join(f.root,'missing'));
  assert(missing.validation_failures.some(row=>row.includes('Missing contract log')));
});

test('interrupted harness logs survive while manifest paths cannot select other files',t=>{
  const f=fixture(t);f.run.status='fail';f.run.stages=[];
  const id='01234567-89ab-cdef-0123-456789abcdef';
  const directory=path.join(f.root,'contracts',id);fs.mkdirSync(directory,{recursive:true});
  const manifest={schema_version:1,run_id:id,suite:'fast',attempts:[{attempt_id:id,status:'running',
    artifacts:[{path:'../../private-profile.json',sha256:digest}]}]};
  fs.writeFileSync(path.join(directory,'manifest.json'),JSON.stringify(manifest));
  fs.writeFileSync(path.join(directory,id+'-stdout.log'),'partial interrupted output');
  f.file('private-profile.json','must remain private');
  const result=evidence.packet(f.write(),path.join(f.root,'packet'));
  assert.equal(result.pipeline_status,'fail');assert.equal(result.files.length,2);
  assert(result.files.some(row=>row.path.endsWith(id+'-stdout.log')));
  assert(result.files.every(row=>!row.path.includes('private-profile')));
  manifest.attempts[0].attempt_id='../invalid';
  fs.writeFileSync(path.join(directory,'manifest.json'),JSON.stringify(manifest));
  const invalid=evidence.packet(f.write(),path.join(f.root,'invalid'));
  assert(invalid.validation_failures.some(row=>row.includes('attempt identity')));
});
