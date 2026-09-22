// SPDX-License-Identifier: Apache-2.0
'use strict';
// Independent P7-05 oracle. It does not import the older U03 oracle.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process'),{isDeepStrictEqual}=require('node:util');
const {plain,read,safeChild,filesUnder}=require('./p6-live-runner.cjs').boundaries;
const fixtureRoot=path.resolve(__dirname,'../../src/evals/delegation/generation-v1');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const manifest=()=>JSON.parse(read(path.join(fixtureRoot,'manifest.json')));
const modelFiles=directory=>filesUnder(directory).filter(file=>file!=='.git'&&!file.startsWith('.git/'));
const adapter=`
for(const key of Object.keys(process.env))if(key.toUpperCase()!=='SYSTEMROOT')delete process.env[key];
const fs=require('node:fs'),parse=JSON.parse.bind(JSON),encode=JSON.stringify.bind(JSON),write=process.stdout.write.bind(process.stdout);
if(!process.permission||process.permission.has('net')!==false)throw Error('Network denial required');
const cases=parse(fs.readFileSync(0,'utf8')),quote=require(process.argv[1]).quoteCart;
write(encode(cases.map(args=>{const before=encode(args);try{return {value:quote(...args),unchanged:before===encode(args)};}catch(error){return {error:error.name,unchanged:before===encode(args)};}})));`;
const centsAdapter=`
for(const key of Object.keys(process.env))if(key.toUpperCase()!=='SYSTEMROOT')delete process.env[key];
const fs=require('node:fs'),parse=JSON.parse.bind(JSON),encode=JSON.stringify.bind(JSON),write=process.stdout.write.bind(process.stdout);
if(!process.permission||process.permission.has('net')!==false)throw Error('Network denial required');
const cases=parse(fs.readFileSync(0,'utf8')),subtotal=require(process.argv[1]).subtotalCents;
write(encode(cases.map(args=>{try{return {value:subtotal(...args)};}catch(error){return {error:error.name};}})));`;
function cases(){
 const valid=[[[],{}],[[{unitCents:250,quantity:2}],{}],[[{unitCents:5,quantity:1}]],[[{unitCents:5,quantity:1}],{discountBps:1000}],[[{unitCents:199,quantity:3},{unitCents:101,quantity:2}],{discountBps:1750}],[[{unitCents:0,quantity:9}],{discountBps:9999}],[[{unitCents:17,quantity:2}],{discountBps:10000}],[[{unitCents:Number.MAX_SAFE_INTEGER,quantity:1}],{discountBps:5000}]];
 for(let i=1;i<=17;i++)valid.push([[{unitCents:i*37,quantity:i%5},{unitCents:13,quantity:2}],{discountBps:i*431}]);
 const invalid=[[null],[{}],[[null]],[[{unitCents:-1,quantity:1}]],[[{unitCents:1.5,quantity:1}]],[[{unitCents:2,quantity:-1}]],[[{unitCents:2,quantity:0.5}]],[[{unitCents:'2',quantity:1}]],[[{unitCents:2,quantity:1}],{discountBps:-1}],[[{unitCents:2,quantity:1}],{discountBps:10001}],[[{unitCents:2,quantity:1}],{discountBps:0.5}],[[{unitCents:2,quantity:1}],null],[[{unitCents:2,quantity:1}],[]],[[{unitCents:2,quantity:1}],'invalid'],[[{unitCents:2,quantity:1}],{discountBps:'5000'}],[[{unitCents:2,quantity:Number.MAX_SAFE_INTEGER+1}],{}],[[{unitCents:Number.MAX_SAFE_INTEGER+1,quantity:0}],{}],[[{unitCents:Number.MAX_SAFE_INTEGER,quantity:2}],{}],[[{unitCents:Number.MAX_SAFE_INTEGER,quantity:1},{unitCents:1,quantity:1}],{}]];
 return [...valid.map(args=>{const subtotal=args[0].reduce((n,item)=>n+BigInt(item.unitCents)*BigInt(item.quantity),0n),options=args[1]??{},discount=(subtotal*BigInt(options.discountBps??0)+5000n)/10000n;return {args,expected:{value:{subtotalCents:Number(subtotal),discountCents:Number(discount),totalCents:Number(subtotal-discount)},unchanged:true}};}),...invalid.map(args=>({args,expected:{error:'TypeError',unchanged:true}}))];
}
function preservationFailures(workspace,frozen=manifest()){
 const expected=frozen.files.map(file=>file.path).sort(),names=modelFiles(workspace).sort(),failures=[];
 if(JSON.stringify(names)!==JSON.stringify(expected))failures.push('workspace inventory changed');
 for(const file of frozen.files.filter(file=>!frozen.editable.includes(file.path)))try{const bytes=read(safeChild(workspace,file.path));if(bytes.length!==file.bytes||sha(bytes)!==file.sha256)failures.push(file.path);}catch{failures.push(file.path);}
 return failures;
}
function moduleBoundaryFailures(workspace,runtime=process.execPath){
 const failures=[],candidate=safeChild(workspace,'src/cart.cjs'),helper=safeChild(workspace,'src/cents.cjs'),boundary=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-boundary-'));
 try {
  fs.mkdirSync(path.join(boundary,'src'),{recursive:true});fs.copyFileSync(candidate,path.join(boundary,'src/cart.cjs'));fs.writeFileSync(path.join(boundary,'src/cents.cjs'),"module.exports={subtotalCents(){return 7;}};\n");
  const probe=spawnSync(runtime,['--permission',`--allow-fs-read=${boundary}`,'--max-old-space-size=64','-e',adapter,path.join(boundary,'src/cart.cjs')],{cwd:boundary,env:process.platform==='win32'&&process.env.SystemRoot?{SystemRoot:process.env.SystemRoot}:{},input:JSON.stringify([[[],{}]]),encoding:'utf8',timeout:3000,maxBuffer:65536,windowsHide:true});
  if(probe.error||probe.status!==0||!isDeepStrictEqual(JSON.parse(probe.stdout)?.[0],{value:{subtotalCents:7,discountCents:0,totalCents:7},unchanged:true}))failures.push('module boundary not exercised');
  const helperCases=[[[{unitCents:2,quantity:3}]], [[]], [null], [[{unitCents:-1,quantity:1}]], [[{unitCents:Number.MAX_SAFE_INTEGER,quantity:2}]], [[{unitCents:Number.MAX_SAFE_INTEGER,quantity:1},{unitCents:1,quantity:1}]]],expected=[{value:6},{value:0},{error:'TypeError'},{error:'TypeError'},{error:'TypeError'},{error:'TypeError'}];
  const observed=spawnSync(runtime,['--permission',`--allow-fs-read=${workspace}`,'--max-old-space-size=64','-e',centsAdapter,helper],{cwd:workspace,env:process.platform==='win32'&&process.env.SystemRoot?{SystemRoot:process.env.SystemRoot}:{},input:JSON.stringify(helperCases),encoding:'utf8',timeout:3000,maxBuffer:65536,windowsHide:true});
  if(observed.error||observed.status!==0||!isDeepStrictEqual(JSON.parse(observed.stdout),expected))failures.push('cents helper contract failed');
 } catch { failures.push('module boundary unavailable'); }
 finally { fs.rmSync(boundary,{recursive:true,force:true}); }
 return failures;
}
function evaluate(observed,preservation=[]){const trials=cases(),failures=trials.flatMap((trial,index)=>isDeepStrictEqual(observed?.[index],trial.expected)?[]:[index]),complete=Array.isArray(observed)&&observed.length===trials.length;return {schema:'p7-05-delegation-oracle/1',pass:complete&&!failures.length&&!preservation.length,preservation,passed:trials.length-failures.length,total:trials.length,failed_cases:failures};}
function grade(workspace,runtime=process.execPath){
 const help=spawnSync(runtime,['--help'],{env:{},encoding:'utf8',timeout:3000,maxBuffer:131072,windowsHide:true});if(help.error||help.status!==0||!help.stdout.includes('--allow-net'))throw Error('Network-denying Node permission runtime unavailable; candidate was not loaded');
 workspace=plain(path.resolve(workspace));const frozen=manifest(),preservation=preservationFailures(workspace,frozen),candidate=safeChild(workspace,'src/cart.cjs');read(candidate,128*1024);const trials=cases(),environment=process.platform==='win32'&&process.env.SystemRoot?{SystemRoot:process.env.SystemRoot}:{};
 const result=spawnSync(runtime,['--permission',`--allow-fs-read=${workspace}`,'--max-old-space-size=64','-e',adapter,candidate],{cwd:workspace,env:environment,input:JSON.stringify(trials.map(t=>t.args)),encoding:'utf8',timeout:3000,maxBuffer:65536,windowsHide:true});let observed;
 try{if(result.error||result.status!==0)throw Error('candidate process failed');observed=JSON.parse(result.stdout);}catch{return {schema:'p7-05-delegation-oracle/1',pass:false,preservation,passed:0,total:trials.length,reason:'bounded candidate observation unavailable'};}
 return evaluate(observed,[...new Set([...preservation,...preservationFailures(workspace,frozen),...moduleBoundaryFailures(workspace,runtime)])]);
}
 module.exports={grade,cases,evaluate,preservationFailures,moduleBoundaryFailures};
if(require.main===module){try{if(process.argv.length!==3)throw Error('Usage: delegation-generation-oracle.cjs <candidate-workspace>');const result=grade(process.argv[2]);console.log(JSON.stringify(result));if(!result.pass)process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
