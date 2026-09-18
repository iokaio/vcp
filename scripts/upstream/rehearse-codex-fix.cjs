// SPDX-License-Identifier: Apache-2.0
// Replays an already selected immutable upstream fix; does not advance the pin.
'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {execFileSync,spawnSync}=require('node:child_process');
const {outside}=require('../../src/tests/support/model-assets.cjs');
const {digest,writeManifest}=require('../../src/tests/support/harness.cjs');
const fix='9daa491f7c27a5513fec554473a7122d88fca367';
const parent='81b9bc210926b14b2af5c3300f13972909266aab';
function main(args){
  const options={};for(let i=0;i<args.length;i+=2){if(!['--source','--output-root'].includes(args[i])||!args[i+1]||options[args[i]])throw Error('Supply --source and --output-root');options[args[i]]=args[i+1];}
  if(Object.keys(options).length!==2)throw Error('Supply both paths');
  const root=path.resolve(__dirname,'../..'),source=path.resolve(options['--source']);
  const output=path.join(outside(options['--output-root'],[source,path.join(root,'src')]),crypto.randomUUID());fs.mkdirSync(output,{recursive:true});
  const record={schema_version:1,task_id:'P0-08',status:'running',started_at:new Date().toISOString(),fix,parent,conflicts:[],scope:'Retrospective six-file import at its parent with the current VCP Job Object overlay; unchanged dependency selection.'};
  const git=args=>execFileSync('git',['-c','safe.directory='+source,'-C',source,...args],{windowsHide:true,maxBuffer:16*1024*1024});
  const write=(base,file,bytes)=>{const target=path.join(output,base,file);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes);};
  try{
    if(git(['rev-parse',fix+'^']).toString().trim()!==parent)throw Error('Fix parent mismatch');
    const files=git(['diff','--name-only',parent,fix]).toString().trim().split('\n');
    if(files.length!==6||files.some(f=>!/^codex-rs\/(utils\/pty|rmcp-client)\//.test(f)))throw Error('Unexpected fix scope');
    const patch=git(['diff','--binary',parent,fix]);fs.writeFileSync(path.join(output,'upstream.patch'),patch);record.patch_sha256=digest(patch);record.numstat=git(['diff','--numstat',parent,fix]).toString().trim();
    for(const file of files)write('work',file,git(['show',parent+':'+file]));
    const job='codex-rs/utils/pty/src/win/job.rs';
    write('before',job,git(['show',fix+':'+job]));write('after',job,fs.readFileSync(path.join(root,'src/third_party/codex',job)));
    const delta=spawnSync('git',['diff','--no-index','--binary','--','before/'+job,'after/'+job],{cwd:output,windowsHide:true,maxBuffer:1024*1024});if(delta.error||delta.status!==1)throw Error('Expected explicit VCP job overlay');
    const overlay=delta.stdout;fs.writeFileSync(path.join(output,'vcp-overlay.patch'),overlay);record.overlay_sha256=digest(overlay);
    const metadata=path.join(output,'metadata.git'),work=path.join(output,'work');execFileSync('git',['init','--bare',metadata],{windowsHide:true,stdio:'pipe'});
    const apply=(bytes,strip)=>execFileSync('git',['--git-dir='+metadata,'--work-tree='+work,'-c','core.autocrlf=false','apply','--whitespace=error','-p'+strip,'-'],{cwd:work,input:bytes,windowsHide:true,stdio:'pipe'});
    // Apply the VCP delta first: the later upstream import must coexist with it.
    try { apply(overlay,2);apply(patch,1); }
    catch(error) {
      record.conflicts.push({path:job,reason:'The maintained VCP overlay expects imports introduced by this upstream fix.',resolution:'Reconstruct the six pristine parent files, import the immutable upstream fix, then reapply the unchanged VCP overlay.'});
      fs.writeFileSync(path.join(output,'initial-conflict.log'),String(error.message));
      for(const file of files)write('work',file,git(['show',parent+':'+file]));
      apply(patch,1);apply(overlay,2);
    }
    record.results=files.map(file=>{
      const actual=fs.readFileSync(path.join(work,file));const expected=file===job?fs.readFileSync(path.join(root,'src/third_party/codex',file)):git(['show',fix+':'+file]);
      if(!actual.equals(expected))throw Error('Reconstructed fix differs: '+file);
      return {path:file,sha256:digest(actual)};
    });
    record.status='pass';record.exit_code=0;
    record.test_requirement='Run retained contained_spawn_owns_immediate_descendant and rejected_job_assignment_resumes_existing_job_member, plus VCP native tree recovery. Reconstruction alone is not behavioral acceptance.';
  }catch(error){record.status='fail';record.reason=error.message;record.exit_code=1;}
  record.ended_at=new Date().toISOString();record.runner_sha256=digest(fs.readFileSync(__filename));writeManifest(path.join(output,'manifest.json'),record);console.log(JSON.stringify({status:record.status,reason:record.reason,manifest:path.join(output,'manifest.json')}));process.exitCode=record.exit_code;
}
try{main(process.argv.slice(2));}catch(e){console.error(e.message);process.exitCode=1;}
