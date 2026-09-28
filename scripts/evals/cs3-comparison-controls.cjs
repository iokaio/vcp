// SPDX-License-Identifier: Apache-2.0
'use strict';
// Trusted oracle controls, never candidate inputs or paid runs.
const fs = require('node:fs'), crypto = require('node:crypto');
const { tasks } = require('../../src/evals/skills/cs3-comparison/cohort.cjs');
const oracle = require('./cs3-comparison-oracle.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const server = mode => `'use strict';
const exact=(x,keys)=>x&&typeof x==='object'&&!Array.isArray(x)&&Object.keys(x).sort().join(',')===[...keys].sort().join(',');
exports.handle=(m,s)=>{
const valid=m&&typeof m==='object'&&!Array.isArray(m)&&m.jsonrpc==='2.0'&&typeof m.method==='string';
const has=valid&&Object.hasOwn(m,'id'), id=has?m.id:null;
const error=(code)=>({jsonrpc:'2.0',id,error:{code,message:'Rejected'}}), result=value=>({jsonrpc:'2.0',id,result:value});
if(!valid||(has&&typeof id!=='string'&&!(Number.isSafeInteger(id)&&id>=0)))return {jsonrpc:'2.0',id:null,error:{code:-32600,message:'Invalid request'}};
if(!has){if(m.method==='notifications/initialized'&&m.params===undefined&&s.negotiated)s.ready=true;return null;}
if(m.method==='initialize'){
if(s.negotiated)return error(-32600);
if(!exact(m.params,['protocolVersion'])||m.params.protocolVersion!=='2025-03-26')return error(-32602);
s.negotiated=true;return result({protocolVersion:'2025-03-26',capabilities:{}});}
if(!s.ready)return error(-32000);
${mode === 'inventory' ? `
if(m.method==='tools/list'){
if(m.params!==undefined&&!exact(m.params,[]))return error(-32602);
return result({tools:[{name:'inventory_count',inputSchema:{type:'object',properties:{sku:{type:'string',enum:['cedar','birch']}},required:['sku'],additionalProperties:false}}]});}
if(m.method==='tools/call'){
if(!exact(m.params,['name','arguments'])||m.params.name!=='inventory_count'||!exact(m.params.arguments,['sku'])||!['cedar','birch'].includes(m.params.arguments.sku))return error(-32602);
return result({content:[{type:'text',text:m.params.arguments.sku==='cedar'?'7':'0'}],isError:false});}` : `
if(m.method==='resources/list'){
if(m.params===undefined||exact(m.params,[]))return result({resources:[{uri:'memo://cedar',name:'Cedar'}],nextCursor:'next'});
if(exact(m.params,['cursor'])&&m.params.cursor==='next')return result({resources:[{uri:'memo://birch',name:'Birch'}]});return error(-32602);}
if(m.method==='resources/read'){
if(!exact(m.params,['uri'])||!['memo://cedar','memo://birch'].includes(m.params.uri))return error(-32602);
return result({contents:[{uri:m.params.uri,mimeType:'text/plain',text:m.params.uri==='memo://cedar'?'seven':'zero'}]});}`}
return error(-32601);
};
`;
const embed = `'use strict';
const exact=(x,keys)=>x&&typeof x==='object'&&!Array.isArray(x)&&Object.keys(x).sort().join(',')===[...keys].sort().join(',');
exports.embed=async(texts,send,signal)=>{
if(!Array.isArray(texts)||texts.length<1||texts.length>3||texts.some(t=>typeof t!=='string'||!t.length||[...t].length>16))throw new TypeError('Invalid texts');
if(signal?.aborted){const e=new Error('Aborted');e.name='AbortError';throw e;}
const r=await send({model:'local-embed-2',input:texts,dimensions:2},signal);
if(!exact(r,['data','usage'])||!Array.isArray(r.data)||r.data.length!==texts.length||!exact(r.usage,['input_tokens'])||!Number.isSafeInteger(r.usage.input_tokens)||r.usage.input_tokens<0)throw new TypeError('Invalid response');
const vectors=[],seen=new Set();
for(const item of r.data){if(!exact(item,['index','embedding'])||!Number.isInteger(item.index)||item.index<0||item.index>=texts.length||seen.has(item.index)||!Array.isArray(item.embedding)||item.embedding.length!==2||!item.embedding.every(Number.isFinite))throw new TypeError('Invalid item');seen.add(item.index);vectors[item.index]=item.embedding;}
return {vectors,inputTokens:r.usage.input_tokens};
};
`;
const stream = `'use strict';
exports.collect=async(chunks,signal)=>{
const abort=()=>{if(signal?.aborted){const e=new Error('Aborted');e.name='AbortError';throw e;}};
abort();const iterator=chunks[Symbol.asyncIterator](),decoder=new TextDecoder('utf-8',{fatal:true});let pending='',text='';
const line=(s)=>{if(!s)return null;let x;try{x=JSON.parse(s)}catch{throw new TypeError('Invalid JSON')}
if(!x||typeof x!=='object'||Array.isArray(x))throw new TypeError('Invalid event');
if(x.type==='delta'&&Object.keys(x).sort().join(',')==='text,type'&&typeof x.text==='string'){text+=x.text;if([...text].length>48)throw new TypeError('Too much text');return null;}
if(x.type==='done'&&Object.keys(x).sort().join(',')==='tokens,type'&&Number.isSafeInteger(x.tokens)&&x.tokens>=0)return {text,tokens:x.tokens};throw new TypeError('Invalid event');};
try{for(;;){abort();const next=await iterator.next();abort();if(next.done){pending+=decoder.decode();const result=line(pending);if(result)return result;throw new TypeError('Missing done');}
if(!(next.value instanceof Uint8Array))throw new TypeError('Invalid chunk');pending+=decoder.decode(next.value,{stream:true});let at;while((at=pending.indexOf('\\n'))>=0){const result=line(pending.slice(0,at));pending=pending.slice(at+1);if(result)return result;}}}
finally{if(typeof iterator.return==='function')await iterator.return();}
};
`;
const controls = [
  { id: 'MCP-cs3-inventory-tool-v1', files: { 'server.cjs': server('inventory') }, mutations: [text => text.replace("if(!s.ready)return error(-32000);", ''), text => text.replace("result=value=>({jsonrpc:'2.0'", "result=value=>({jsonrpc:'1.0'")] },
  { id: 'MCP-cs3-resource-pages-v1', files: { 'server.cjs': server('resources') }, mutations: [text => text.replace("if(!s.ready)return error(-32000);", ''), text => text.replace("?'seven':'zero'", "?'seven':'missing'")] },
  { id: 'LLM-cs3-embedding-batch-v1', files: { 'embed.cjs': embed }, mutations: [text => text.replace('vectors[item.index]=item.embedding', 'vectors.push(item.embedding)'), text => text.replace('[...t].length>16', 't.length>16')] },
  { id: 'LLM-cs3-delta-reader-v1', files: { 'stream.cjs': stream }, mutations: [text => text.replace("if(typeof iterator.return==='function')await iterator.return();", ''), text => text.replace('const result=line(pending);if(result)return result;', '')] },
  { id: 'WEB-near-miss-unit-v1', files: { 'normalize.cjs': "exports.normalize=value=>{if(typeof value!=='string')throw new TypeError('String required');return value.trim().replace(/[A-Z]/g,letter=>String.fromCharCode(letter.charCodeAt(0)+32));};\n" }, mutations: [() => "exports.normalize=value=>value.trim().toLowerCase();\n"] }
];
async function run(executor) {
  const sourceBefore = { source_sha256: sha(fs.readFileSync(__filename)), oracle_sha256: sha(fs.readFileSync(require.resolve('./cs3-comparison-oracle.cjs'))), grader_sha256: sha(fs.readFileSync(require.resolve('./developer-grader.cjs'))) };
  const results = [];
  for (const control of controls) {
    const task = tasks.find(t => t.id === control.id) || { id: control.id }, positive = await oracle.nodeGrade(task, control.files, executor);
    const negatives = [];
    for (const mutate of control.mutations) negatives.push(await oracle.nodeGrade(task, Object.fromEntries(Object.entries(control.files).map(([name, text]) => [name, mutate(text)])), executor));
    results.push({ case_id: task.id, positive, negatives, passed: positive.passed && negatives.every(negative => !negative.passed) });
  }
  if (sourceBefore.source_sha256 !== sha(fs.readFileSync(__filename)) || sourceBefore.oracle_sha256 !== sha(fs.readFileSync(require.resolve('./cs3-comparison-oracle.cjs'))) || sourceBefore.grader_sha256 !== sha(fs.readFileSync(require.resolve('./developer-grader.cjs')))) throw Error('Oracle controls changed during execution');
  return { schema: 'cs3-node-oracle-controls/1', status: results.every(r => r.passed) ? 'passed' : 'failed', model_calls: 0, executor: executor.name,
    ...sourceBefore, node_sha256: sha(fs.readFileSync(fs.realpathSync(process.execPath))), results };
}
module.exports = { controls, run };
if (require.main === module) {
  const node = fs.realpathSync(process.execPath), executor = oracle.appContainerExecutor({ node, nodeSha256: sha(fs.readFileSync(node)) });
  run(executor).then(result => {
    const bytes = JSON.stringify(result, null, 2) + '\n', destination = process.argv[2];
    if (destination) { fs.writeFileSync(destination, bytes, { flag: 'wx', mode: 0o600 }); process.stdout.write(JSON.stringify({ path: destination, sha256: sha(bytes), status: result.status, cases: result.results.length, model_calls: 0 }) + '\n'); }
    else process.stdout.write(bytes);
    if (result.status !== 'passed') process.exitCode = 1;
  }).catch(error => { process.stderr.write(error.stack + '\n'); process.exitCode = 1; });
}
