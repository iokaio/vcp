// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { isDeepStrictEqual } = require('node:util');
const { plain, read, safeChild, filesUnder } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../../src/evals/skills/builtin/generation-v1');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
// This adapter receives inputs only. Expected answers stay in the parent process,
// outside the candidate's read grant; no provider environment is inherited.
const adapter = `
for(const key of Object.keys(process.env))if(key.toUpperCase()!=='SYSTEMROOT')delete process.env[key];
const fs=require('node:fs');
const parse=JSON.parse.bind(JSON), encode=JSON.stringify.bind(JSON);
const write=process.stdout.write.bind(process.stdout);
if(!process.permission || process.permission.has('net')!==false) throw Error('Network denial required');
const cases=parse(fs.readFileSync(0,'utf8'));
const quote=require(process.argv[1]).quoteCart;
const results=cases.map(args=>{const before=encode(args);try { const value=quote(...args);return {value,unchanged:before===encode(args)}; }catch(error){return {error:error.name,unchanged:before===encode(args)};}});
write(encode(results));
`;
function cases() {
  const valid = [
    [[], {}], [[{unitCents:250,quantity:2}], {}], [[{unitCents:5,quantity:1}], {discountBps:1000}],
    [[{unitCents:199,quantity:3},{unitCents:101,quantity:2}], {discountBps:1750}],
    [[{unitCents:0,quantity:9}], {discountBps:9999}], [[{unitCents:17,quantity:2}], {discountBps:10000}],
    [[{unitCents:Number.MAX_SAFE_INTEGER,quantity:1}], {discountBps:5000}],
  ];
  for(let i=1;i<=17;i++) valid.push([[{unitCents:i*37,quantity:i%5},{unitCents:13,quantity:2}],{discountBps:i*431}]);
  const invalid = [ [null], [{}], [[null]], [[{unitCents:-1,quantity:1}]], [[{unitCents:1.5,quantity:1}]],
    [[{unitCents:2,quantity:-1}]], [[{unitCents:2,quantity:0.5}]], [[{unitCents:'2',quantity:1}]],
    [[{unitCents:2,quantity:1}],{discountBps:-1}], [[{unitCents:2,quantity:1}],{discountBps:10001}],
    [[{unitCents:2,quantity:1}],{discountBps:0.5}], [[{unitCents:2,quantity:1}],null],
    [[{unitCents:Number.MAX_SAFE_INTEGER,quantity:2}]],
    [[{unitCents:Number.MAX_SAFE_INTEGER,quantity:1},{unitCents:1,quantity:1}]],
  ];
  return [...valid.map(args=>{
    const subtotal=args[0].reduce((n,item)=>n+BigInt(item.unitCents)*BigInt(item.quantity),0n);
    const discount=(subtotal*BigInt(args[1].discountBps??0)+5000n)/10000n;
    return {args,expected:{value:{subtotalCents:Number(subtotal),discountCents:Number(discount),totalCents:Number(subtotal-discount)},unchanged:true}};
  }),...invalid.map(args=>({args,expected:{error:'TypeError',unchanged:true}}))];
}
function preservationFailures(workspace,manifest) {
  const expectedNames=manifest.files.map(file=>file.path).sort();
  const names=filesUnder(workspace).sort();
  const preservation=[];
  if(JSON.stringify(names)!==JSON.stringify(expectedNames)) preservation.push('workspace inventory changed');
  for(const file of manifest.files.filter(file=>file.path!=='src/cart.cjs')) {
    try {const bytes=read(safeChild(workspace,file.path));if(bytes.length!==file.bytes||sha(bytes)!==file.sha256)preservation.push(file.path);}
    catch {preservation.push(file.path);}
  }
  return preservation;
}
function grade(workspace) {
  const help=spawnSync(process.execPath,['--help'],{env:{},encoding:'utf8',timeout:3000,maxBuffer:131072,windowsHide:true});
  if(help.error||help.status!==0||!help.stdout.includes('--allow-net')) throw Error('Network-denying Node permission runtime unavailable; candidate was not loaded');
  workspace=plain(path.resolve(workspace));
  const manifest=JSON.parse(read(path.join(root,'manifest.json')));
  const preservation=preservationFailures(workspace,manifest);
  const candidate=safeChild(workspace,'src/cart.cjs'); read(candidate,128*1024);
  const trials=cases(), environment={};
  if(process.platform==='win32'&&process.env.SystemRoot) environment.SystemRoot=process.env.SystemRoot;
  const result=spawnSync(process.execPath,['--permission',`--allow-fs-read=${workspace}`,'--max-old-space-size=64','-e',adapter,candidate],{
    cwd:workspace,env:environment,input:JSON.stringify(trials.map(t=>t.args)),encoding:'utf8',timeout:3000,maxBuffer:65536,windowsHide:true,
  });
  let observed;
  try {if(result.error||result.status!==0) throw Error('candidate process failed');observed=JSON.parse(result.stdout);}
  catch {return {schema:'p7-u03-oracle/1',pass:false,preservation,passed:0,total:trials.length,reason:'bounded candidate observation unavailable'};}
  return evaluate(observed,[...new Set([...preservation,...preservationFailures(workspace,manifest)])]);
}
function evaluate(observed,preservation=[]) {
  const trials=cases();
  const failures=trials.flatMap((trial,index)=>isDeepStrictEqual(observed?.[index],trial.expected)?[]:[index]);
  const complete=Array.isArray(observed)&&observed.length===trials.length;
  return {schema:'p7-u03-oracle/1',pass:complete&&!failures.length&&!preservation.length,preservation,passed:trials.length-failures.length,total:trials.length,failed_cases:failures};
}
module.exports={grade,cases,evaluate};
if(require.main===module){try {if(process.argv.length!==3)throw Error('Usage: builtin-generation-oracle.cjs <candidate-workspace>');const result=grade(process.argv[2]);console.log(JSON.stringify(result));if(!result.pass)process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
