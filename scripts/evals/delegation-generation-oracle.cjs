// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs=require('node:fs'),path=require('node:path'),{spawnSync}=require('node:child_process');
const {cases,evaluate}=require('./builtin-generation-oracle.cjs');
const adapter=`for(const key of Object.keys(process.env))if(key.toUpperCase()!=='SYSTEMROOT')delete process.env[key];
const fs=require('node:fs'),parse=JSON.parse.bind(JSON),encode=JSON.stringify.bind(JSON),write=process.stdout.write.bind(process.stdout);
if(!process.permission||process.permission.has('net')!==false)throw Error('Network denial required');
const trials=parse(fs.readFileSync(0,'utf8')),quote=require(process.argv[1]).quoteCart;
write(encode(trials.map(args=>{const before=encode(args);try{return {value:quote(...args),unchanged:before===encode(args)};}catch(error){return {error:error.name,unchanged:before===encode(args)};}})));`;
function grade(workspace){
 const result=spawnSync(process.execPath,['--permission',`--allow-fs-read=${workspace}`,'--max-old-space-size=64','-e',adapter,path.join(workspace,'src/cart.cjs')],{cwd:workspace,env:{SystemRoot:process.env.SystemRoot},input:JSON.stringify(cases().map(c=>c.args)),encoding:'utf8',timeout:3000,maxBuffer:65536,windowsHide:true});
 if(result.error||result.status!==0)return {pass:false,passed:0,total:cases().length,reason:'bounded candidate observation unavailable'};
 return evaluate(JSON.parse(result.stdout));
}
module.exports={grade};
if(require.main===module){try{if(process.argv.length!==3)throw Error('One isolated candidate mirror required');const result=grade(path.resolve(process.argv[2]));console.log(JSON.stringify(result));if(!result.pass)process.exitCode=1;}catch(error){console.error(error.message);process.exitCode=1;}}
