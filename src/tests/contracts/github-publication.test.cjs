// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const {createHash} = require('node:crypto');
const publication = require('../../../scripts/release/github-publication.cjs');
const {renderDownloadPage} = require('../../../scripts/release/download-page.cjs');
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const commit = 'b'.repeat(40), pairId = 'a'.repeat(64), tag = 'v0.2.0-beta.1-' + pairId.slice(0,12);
const apiRoot = 'https://api.github.com/repos/iokaio/vcp/';
function environment(t, values = {}) {
  const selected = {GH_TOKEN:'synthetic-token-not-a-credential',GITHUB_REPOSITORY:'iokaio/vcp',GITHUB_REF:'refs/heads/main',
    EXPECTED_PAIR_ID:pairId,EXPECTED_COMMIT:commit,GITHUB_STEP_SUMMARY:undefined,GITHUB_OUTPUT:undefined,...values};
  const before = Object.fromEntries(Object.keys(selected).map(key => [key,process.env[key]]));
  for (const [key,value] of Object.entries(selected)) { if (value === undefined) delete process.env[key]; else process.env[key] = value; }
  t.after(() => {for (const [key,value] of Object.entries(before)) {if (value === undefined) delete process.env[key]; else process.env[key] = value;}});
  t.mock.method(console,'log',()=>{});
}
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(),'vcp-github-publication-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const assets = path.join(root,'assets');fs.mkdirSync(assets);
  const base = `https://github.com/iokaio/vcp/releases/download/${tag}/`;
  const release = {version:'0.2.0-beta.1',vsixVersion:'0.2.1',pairId,commit,tag,candidateAt:'2026-10-01T15:00:00.000Z',
    runUrl:'https://github.com/iokaio/vcp/actions/runs/123',manifestHref:base+'release.json',checksumHref:base+'SHA256SUMS',artifacts:[]};
  for (const [kind,name] of [['setup','vcp-setup.exe'],['zip','vcp.zip'],['vsix','vcp.vsix']]) {
    const bytes=Buffer.from(`synthetic ${kind} bytes; never executable`);fs.writeFileSync(path.join(assets,name),bytes);
    release.artifacts.push({kind,name,bytes:bytes.length,sha256:digest(bytes),href:base+name});
  }
  fs.writeFileSync(path.join(assets,'release.json'),JSON.stringify(release,null,2)+'\n');
  fs.writeFileSync(path.join(assets,'SHA256SUMS'),fs.readdirSync(assets).sort().map(name=>`${digest(fs.readFileSync(path.join(assets,name)))}  ${name}\n`).join(''));
  fs.writeFileSync(path.join(root,'notes.md'),'Unsigned beta; full qualification incomplete. Synthetic publication test only.\n');
  const expected=fs.readdirSync(assets).sort().map(name=>{const bytes=fs.readFileSync(path.join(assets,name));return {name,bytes:bytes.length,sha256:digest(bytes)}});
  return {root,assets,release,expected};
}
function remoteAsset(row) {return {name:row.name,state:'uploaded',size:row.bytes,digest:`sha256:${row.sha256}`};}
function response(value,status=200) {return new Response(status===204?null:JSON.stringify(value),{status,headers:{'Content-Type':'application/json'}});}
function github(t,f,{existing=false,draft=true,differing=false,failUpload=0,corruptReread=false}={}) {
  const calls=[],state={ref:existing,uploads:0,remote:existing?{
    id:7,tag_name:tag,draft,prerelease:true,html_url:`https://github.com/iokaio/vcp/releases/tag/${tag}`,
    upload_url:'https://uploads.github.com/repos/iokaio/vcp/releases/7/assets{?name,label}',
    assets:f.expected.map(remoteAsset),
  }:null};
  if(differing)state.remote.assets[0].digest='sha256:'+'0'.repeat(64);
  t.mock.method(globalThis,'fetch',async (input,options={})=>{
    const url=new URL(String(input)),method=options.method||'GET';
    assert.equal(options.headers.Authorization,'Bearer synthetic-token-not-a-credential');
    assert(options.signal instanceof AbortSignal,'Every network operation needs a bound');
    calls.push({method,url:url.href,body:Buffer.isBuffer(options.body)?Buffer.from(options.body):options.body});
    if(url.hostname==='uploads.github.com') {
      assert.equal(url.pathname,'/repos/iokaio/vcp/releases/7/assets');assert.equal(method,'POST');
      assert.equal(state.remote.draft,true,'No upload may target a published release');
      const name=url.searchParams.get('name'),expected=f.expected.find(row=>row.name===name);
      assert(expected);assert(Buffer.isBuffer(options.body));assert.equal(digest(options.body),expected.sha256);
      assert.equal(options.headers['Content-Length'],String(expected.bytes));assert(!state.remote.assets.some(row=>row.name===name),'No asset overwrite');
      state.uploads++;if(state.uploads===failUpload)return response({message:'synthetic upload failure'},503);
      const asset=remoteAsset(expected);state.remote.assets.push(asset);return response(asset,201);
    }
    assert(url.href.startsWith(apiRoot),'Unexpected request origin');
    const route=url.href.slice(apiRoot.length);
    if(method==='GET'&&route==='releases?per_page=100&page=1')return response(state.remote?[state.remote]:[]);
    if(method==='GET'&&route===`git/ref/tags/${tag}`)return state.ref?response({object:{type:'commit',sha:commit}}):response({},404);
    if(method==='POST'&&route==='git/refs') {
      assert.deepEqual(JSON.parse(options.body),{ref:`refs/tags/${tag}`,sha:commit});state.ref=true;return response({},201);
    }
    if(method==='POST'&&route==='releases') {
      const body=JSON.parse(options.body);assert.equal(body.draft,true);assert.equal(body.prerelease,true);assert.equal(body.make_latest,'false');
      assert.equal(body.tag_name,tag);assert.equal(body.target_commitish,commit);assert.equal(body.body,fs.readFileSync(path.join(f.root,'notes.md'),'utf8'));
      state.remote={...body,id:7,assets:[],html_url:`https://github.com/iokaio/vcp/releases/tag/${tag}`,upload_url:'https://uploads.github.com/repos/iokaio/vcp/releases/7/assets{?name,label}'};
      return response(state.remote,201);
    }
    if(method==='GET'&&route==='releases/7') {
      if(corruptReread)state.remote.assets[0].digest='sha256:'+'0'.repeat(64);
      return response(state.remote);
    }
    if(method==='PATCH'&&route==='releases/7') {
      assert.deepEqual(JSON.parse(options.body),{draft:false,prerelease:true,make_latest:'false'});
      assert.equal(state.remote.assets.length,5);assert.equal(calls.at(-2).url,apiRoot+'releases/7');assert.equal(calls.at(-2).method,'GET','Re-read uploaded assets before publishing');
      publication.validateRemoteAssets(state.remote.assets,f.expected);state.remote.draft=false;return response(state.remote);
    }
    throw Error(`Unexpected mocked GitHub operation: ${method} ${route}`);
  });
  return {calls,state};
}
test('GitHub publishes only after all five draft uploads have matching remote digests',async t=>{
  environment(t);const f=fixture(t),server=github(t,f);
  await publication.publish(f.root);
  assert.equal(server.state.uploads,5);assert.equal(server.state.remote.draft,false);
  assert.equal(server.calls.filter(call=>call.method==='PATCH').length,1);
  assert.equal(server.calls.filter(call=>call.method==='DELETE').length,0);
  assert.deepEqual(fs.readFileSync(path.join(f.root,'site/latest.json')),fs.readFileSync(path.join(f.assets,'release.json')));
  assert.equal(fs.readFileSync(path.join(f.root,'site/.nojekyll'),'utf8'),'');
  assert.equal(fs.readFileSync(path.join(f.root,'site/index.html'),'utf8'),renderDownloadPage(f.release));
});
test('an interrupted upload preserves an unpublished draft without generating a download page',async t=>{
  environment(t);const f=fixture(t),server=github(t,f,{failUpload:2});
  await assert.rejects(publication.publish(f.root),/upload returned 503/);
  assert.equal(server.state.remote.draft,true);assert.equal(server.state.remote.assets.length,1);
  assert(!server.calls.some(call=>['PATCH','DELETE'].includes(call.method)));
  assert(!fs.existsSync(path.join(f.root,'site')));
});
test('a changed remote digest at final verification leaves all uploads in the draft',async t=>{
  environment(t);const f=fixture(t),server=github(t,f,{corruptReread:true});
  await assert.rejects(publication.publish(f.root),/differs; refusing overwrite/);
  assert.equal(server.state.uploads,5);assert.equal(server.state.remote.draft,true);
  assert(!server.calls.some(call=>['PATCH','DELETE'].includes(call.method)));
  assert(!fs.existsSync(path.join(f.root,'site')));
});
test('a staged binary mutation is refused before any GitHub write',async t=>{
  environment(t);const f=fixture(t),server=github(t,f);
  const file=path.join(f.assets,'vcp.zip'),changed=fs.readFileSync(file);
  changed[0]^=0xff;fs.writeFileSync(file,changed); // Same length; only the digest differs.
  await assert.rejects(publication.publish(f.root));
  assert(server.calls.every(call=>call.method==='GET'),'No tag, draft, upload or publication write is permitted');
  assert.equal(server.state.remote,null);assert.equal(server.state.ref,false);assert.equal(server.state.uploads,0);
  assert(!fs.existsSync(path.join(f.root,'site')));
});
test('an existing differing asset is refused without overwrite or release mutation',async t=>{
  environment(t);const f=fixture(t),server=github(t,f,{existing:true,differing:true});
  await assert.rejects(publication.publish(f.root),/differs; refusing overwrite/);
  assert(server.calls.every(call=>call.method==='GET'));assert.equal(server.state.uploads,0);assert.equal(server.state.remote.draft,true);
  assert(!fs.existsSync(path.join(f.root,'site')));
});
test('an identical already-published prerelease retry recreates Pages files with no GitHub writes',async t=>{
  environment(t);const f=fixture(t),server=github(t,f,{existing:true,draft:false});
  await publication.publish(f.root);
  assert(server.calls.every(call=>call.method==='GET'));assert.equal(server.state.uploads,0);
  assert.equal(fs.readFileSync(path.join(f.root,'site/index.html'),'utf8'),renderDownloadPage(f.release));
});
function candidateRun() {return {id:123,run_attempt:2,repository:{full_name:'iokaio/vcp'},head_repository:{full_name:'iokaio/vcp'},
  path:'.github/workflows/beta-candidate.yml',head_branch:'main',event:'workflow_dispatch',status:'completed',conclusion:'success',head_sha:commit};}
test('candidate admission rejects a fork, other workflow, wrong or failed attempt and unfinished run',()=>{
  const valid=candidateRun();assert.equal(publication.validateRun(valid,'123','2'),commit);
  for(const patch of [{repository:{full_name:'fork/vcp'}},{head_repository:{full_name:'fork/vcp'}},{path:'.github/workflows/ci.yml'},
    {run_attempt:1},{id:124},{conclusion:'failure'},{status:'in_progress'},{event:'push'},{head_branch:'feature'},{head_sha:'not-a-commit'}]) {
    assert.throws(()=>publication.validateRun({...valid,...patch},'123','2'),/exact successful main candidate attempt/);
  }
});
test('selection binds the exact admitted attempt to its unique retained candidate packet',async t=>{
  const f=fixture(t),output=path.join(f.root,'github-output');
  environment(t,{CANDIDATE_RUN_ID:'123',CANDIDATE_ATTEMPT:'2',EXPECTED_PAIR_ID:pairId,GITHUB_OUTPUT:output});
  const expectedName=`internal-beta-${commit}-123-2`,routes=[];
  t.mock.method(globalThis,'fetch',async (input,options)=>{
    assert.equal(options.method,'GET');const route=String(input).slice(apiRoot.length);routes.push(route);
    if(route==='actions/runs/123/attempts/2')return response(candidateRun());
    if(route===`compare/${commit}...main`)return response({status:'ahead'});
    if(route===`actions/workflows/ci.yml/runs?head_sha=${commit}&status=success&event=push&branch=main&per_page=100`)return response({workflow_runs:[{head_sha:commit,head_branch:'main',event:'push',conclusion:'success'}]});
    if(route===`actions/runs/123/artifacts?name=${expectedName}&per_page=100`)return response({artifacts:[{id:42,name:expectedName,expired:false},{id:43,name:expectedName,expired:true}]});
    throw Error('Unexpected selection request');
  });
  await publication.select();assert.equal(routes.length,4);
  assert.equal(fs.readFileSync(output,'utf8'),`commit=${commit}\nartifact_id=42\n`);
});
test('remote asset verification rejects missing, duplicate, unexpected or incorrect bytes',()=>{
  const expected=[{name:'vcp.zip',bytes:17,sha256:'d'.repeat(64)}],valid=remoteAsset(expected[0]);
  publication.validateRemoteAssets([valid],expected);
  publication.validateRemoteAssets([],expected,false);
  for(const assets of [[],[valid,valid],[{...valid,name:'unexpected.zip'}],[{...valid,size:18}],
    [{...valid,digest:'sha256:'+'c'.repeat(64)}],[{...valid,state:'starter'}]])assert.throws(()=>publication.validateRemoteAssets(assets,expected));
});
test('download renderer escapes hostile content and refuses active URL schemes',t=>{
  const f=fixture(t),attack='\"><img src=x onerror="alert(1)">&\'';
  const changed=structuredClone(f.release);changed.version=attack;changed.vsixVersion=attack;changed.pairId=attack;changed.commit=attack;
  changed.candidateAt=attack;changed.artifacts[0].name=attack;changed.artifacts[0].sha256=attack;
  const html=renderDownloadPage(changed);
  assert(!html.includes('<img'));assert(!html.includes('<script'));assert(!html.includes('<link'));
  assert(html.includes('&quot;&gt;&lt;img src=x onerror=&quot;alert(1)&quot;&gt;&amp;&#39;'));
  assert(html.includes('download="&quot;&gt;&lt;img'));
  assert(html.includes('/blob/%22%3E%3Cimg'));
  assert.equal((html.match(/<details class=/g)||[]).length,3);
  assert(html.includes('Unsigned beta for manual testing.'));assert(html.includes('Qualification is incomplete.'));
  for(const unsafe of ['javascript:alert(1)','data:text/html,evil','//evil.invalid','https://user:secret@host.invalid','https:\\host.invalid','https://host.invalid/bad\npath']) {
    for(const field of ['manifestHref','checksumHref','runUrl'])assert.throws(()=>renderDownloadPage({...f.release,[field]:unsafe}));
    const release=structuredClone(f.release);release.artifacts[0].href=unsafe;assert.throws(()=>renderDownloadPage(release));
  }
});
