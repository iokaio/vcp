// SPDX-License-Identifier: Apache-2.0
// Real compiled host/SDK; expected semantics come from the CLI's governed queries.
import assert from 'node:assert/strict';
import {launchLocal} from '../dist/index.js';
let raw='';for await(const chunk of process.stdin)raw+=chunk;
const input=JSON.parse(raw);
const capabilities=['history/query','history/query/1','memory/history','memory/history/1','task/read','artifact/read','session/snapshot','events/unsubscribe'];
const initialize={protocol_version:'1.0',client:{name:'native-inspector-queries',version:'1'},capabilities,required_capabilities:capabilities};
const owned=[];
const value=(reply,kind)=>{assert.equal(reply.kind,kind);return reply.value;};
const counter=text=>{assert.equal(typeof text,'string');assert.match(text,/^(0|[1-9][0-9]*)$/);return BigInt(text);};
try {
  // An unnegotiated profile cannot silently expose a richer inspector response.
  process.stderr.write('phase: limited observer launch\n');
  const limited=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,transport:'stdio',role:'observer',initialize:{...initialize,capabilities:['history/query'],required_capabilities:['history/query']}});owned.push(limited);
  const query={scope:input.scope,task:null,selector:null,text:null,artifact:null,expand_compacted:false,limit:7,cursor:null};
  process.stderr.write('phase: limited observer ready\n');
  await assert.rejects(limited.call('history/query',query));
  await limited.dispose();
  process.stderr.write('phase: full observer launch\n');
  const client=await launchLocal({executable:input.executable,workspace:input.workspace,data:input.data,transport:'stdio',role:'observer',initialize});owned.push(client);
  process.stderr.write('phase: full observer ready\n');
  assert.equal(client.role,'observer');assert.deepEqual(client.scope,input.scope);
  const rows=[];let cursor=null,firstCursor;let pages=0;
  do {
    const page=value(await client.call('history/query',{...query,cursor}),'history');
    assert.equal(page.source_watermark,input.watermark);assert.equal(page.observed_watermark,input.watermark);assert.equal(page.newer_events,'0');counter(page.source_watermark);
    assert(page.rows.length<=7);assert.equal(page.complete,page.next_cursor===null);
    for(const row of page.rows){counter(row.sequence);counter(row.timestamp_ms);assert(!Object.hasOwn(row,'data'));assert(!Object.hasOwn(row,'record'));assert(!Object.hasOwn(row,'content'));rows.push(row);}
    cursor=page.next_cursor;firstCursor??=cursor;assert(++pages<100);
  }while(cursor);
  assert(pages>1);
  assert.deepEqual(rows.map(({id,session,task,sequence,timestamp_ms,visibility,artifacts})=>({id,session,task,sequence,timestamp_ms,visibility,artifacts})),input.history);
  const metadata=rows.find(row=>row.timestamp_ms==='9007199254740993');assert(metadata);assert.equal(metadata.metadata.provider,'offline-provider');assert.equal(metadata.metadata.model,'offline-model');assert.deepEqual(metadata.metadata.paths,['src/parser.rs']);
  assert(rows.some(row=>row.task===null),'session-level rows survive session inspection');
  const taskPage=value(await client.call('history/query',{...query,task:input.task}),'history');assert(taskPage.rows.every(row=>row.task===input.task));
  const backlinks=value(await client.call('history/query',{...query,artifact:input.artifact}),'history');assert(backlinks.rows.length>0);assert(backlinks.rows.every(row=>row.artifacts.some(artifact=>artifact.id===input.artifact)));
  await assert.rejects(client.call('history/query',{...query,task:input.task,cursor:firstCursor}));
  await assert.rejects(client.call('history/query',{...query,task:'missing-task'}));
  await assert.rejects(client.call('history/query',{...query,scope:{...input.scope,workspace:'foreign-workspace'}}));
  const request={scope:input.scope,task:input.task,claim:input.claim,limit:7,cursor:null};
  const versions=[];cursor=null;pages=0;let memoryCursor;let upper;
  do {
    const page=value(await client.call('memory/history',{...request,cursor}),'memory_history');
    assert.equal(page.watermark,input.watermark);counter(page.at);upper??=page.at;assert.equal(page.at,upper);
    assert(page.versions.length<=7);assert.equal(page.complete,page.next_cursor===null);
    for(const version of page.versions){assert.equal(version.finding.claim,input.claim);assert.equal(version.finding.content,`Parser revision ${versions.length}`);assert(version.origins.includes(input.origin));assert(version.finding.evidence.some(e=>e.artifact===input.artifact));assert.equal(version.content_truncated,false);versions.push(version.finding.version);}
    cursor=page.next_cursor;memoryCursor??=cursor;assert(++pages<10);
  }while(cursor);
  assert.equal(versions.length,33);assert.equal(new Set(versions).size,33);assert.deepEqual(versions,input.versions);assert(pages>=5);
  await assert.rejects(client.call('memory/history',{...request,limit:6,cursor:memoryCursor}));
  await assert.rejects(client.call('memory/history',{...request,task:'missing-task'}));
  await assert.rejects(client.call('memory/history',{...request,scope:{...input.scope,session:'foreign-session'}}));
  await assert.rejects(client.call('memory/history',{...request,claim:'missing-claim'}),error=>error.classification?.applicationCode==='STORE_UNAVAILABLE');
  process.stdout.write(JSON.stringify({ok:true,historyRows:rows.length,memoryVersions:versions.length,backends:'qualified by native caller'}));
} finally {for(const client of owned.reverse())await client.dispose();}
