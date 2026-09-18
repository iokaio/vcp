// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const {spawn,spawnSync}=require('node:child_process');
const hash=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
function run(program,args){const r=spawnSync(program,args,{encoding:'utf8',windowsHide:true,timeout:600000,maxBuffer:4*1024*1024});if(r.error||r.signal||r.status!==0)throw Error('Qualification child failed: '+(r.stderr||r.error||r.status));return r.stdout;}
async function killAt(binary,root,kind,phase){
  return new Promise((resolve,reject)=>{
    const child=spawn(binary,['crash',root,kind,phase],{windowsHide:true,stdio:['ignore','pipe','pipe']});let stdout='',stderr='',killed=false;
    const timer=setTimeout(()=>{child.kill();reject(Error('Crash barrier timeout'));},30000);
    child.stdout.on('data',bytes=>{stdout+=bytes;if(stdout.includes('BARRIER '+phase+'\n')||stdout.includes('BARRIER '+phase+'\r\n')){killed=true;child.kill();}});
    child.stderr.on('data',bytes=>{stderr+=bytes;});child.once('error',reject);
    child.once('close',(code,signal)=>{clearTimeout(timer);try{assert(killed);assert(stdout.includes('ACK 1'));assert(!stderr.includes('panicked at'));resolve({stdout,stderr,exit_code:code,signal});}catch(e){reject(e);}});
  });
}
async function main(args){
  if(args.length!==6||args[0]!=='--binary'||args[2]!=='--age'||args[4]!=='--output-root')throw Error('Expected --binary --age --output-root');
  if(process.platform!=='win32')throw Error('Native Windows required');
  const binary=path.resolve(args[1]),age=path.resolve(args[3]),root=path.resolve(args[5]);fs.mkdirSync(root,{recursive:true});
  const pin=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../src/third_party/components/age-qualification.json')));
  if(hash(age)!==pin.binary_sha256)throw Error('Independent age binary does not match the reviewed pin');
  const directory=path.join(root,crypto.randomUUID());fs.mkdirSync(directory);const manifest=path.join(directory,'manifest.json');
  const record={schema_version:1,task_id:'P0-04',status:'running',started_at:new Date().toISOString(),binary_sha256:hash(binary),age_sha256:hash(age),crashes:[]};
  const save=()=>fs.writeFileSync(manifest,JSON.stringify(record,null,2)+'\n');save();
  try{
    assert.equal(run(age,['--version']).trim(),'v1.3.2');
    for(const kind of ['sqlite','files'])for(const phase of ['before_write','before_commit','after_commit','after_ack']){
      const root=path.join(directory,kind+'-'+phase);const killed=await killAt(binary,root,kind,phase);
      const reopened=JSON.parse(run(binary,['inspect',root,kind]));assert.equal(reopened.status,'pass');
      const expected=['after_commit','after_ack'].includes(phase)?2:1;assert.equal(reopened.view.sequence,expected);
      assert.equal(reopened.view.records.find(r=>r.id==='reservation-1').payload.reserved_microusd,12500);
      assert.equal(reopened.view.records.find(r=>r.id==='claim-old').payload.disputed,true);
      record.crashes.push({backend:kind,barrier:phase,expected_sequence:expected,observed_sequence:reopened.view.sequence,...killed});save();
    }
    const transfer=path.join(directory,'handoff');record.producer=JSON.parse(run(binary,['handoff-produce',transfer]));
    const expected=Buffer.from('VCP synthetic interoperability v1\r\n');
    run(age,['--decrypt','-i',path.join(transfer,'recovery/identity.txt'),'-o',path.join(transfer,'go-plaintext.txt'),path.join(transfer,'interop.age')]);
    assert.deepEqual(fs.readFileSync(path.join(transfer,'go-plaintext.txt')),expected);
    const signed=JSON.parse(run(age,['--decrypt','-i',path.join(transfer,'recovery/identity.txt'),path.join(transfer,'vault',record.producer.snapshot+'.age')]));
    const publicKey=crypto.createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),Buffer.from(signed.writer)]),format:'der',type:'spki'});
    assert(crypto.verify(null,Buffer.concat([Buffer.from('vcp-p0-snapshot-signature-v1\0'),Buffer.from(signed.body)]),publicKey,Buffer.from(signed.signature)));
    const recipient=fs.readFileSync(path.join(transfer,'recipient.txt'),'utf8').trim();
    run(age,['--encrypt','-r',recipient,'-o',path.join(transfer,'go.age'),path.join(transfer,'go-plaintext.txt')]);
    record.consumer=JSON.parse(run(binary,['handoff-consume',transfer]));assert.equal(record.consumer.status,'pass');
    record.interoperability={rust_to_go:'pass',go_to_rust:'pass',ed25519_node_verification:'pass',age_version:'v1.3.2'};save();
    const benchmark=path.join(directory,'benchmark'),metrics=path.join(directory,'metrics.json');
    run('pwsh',['-NoProfile','-File',path.resolve(__dirname,'../../src/tests/support/windows/storage-metrics.ps1'),'-Binary',binary,'-Root',benchmark,'-Output',metrics]);
    record.metrics=JSON.parse(fs.readFileSync(metrics,'utf8').replace(/^\uFEFF/,''));assert.equal(record.metrics.status,'pass');assert(record.metrics.samples>=2);assert(record.metrics.cpu_ms>0);assert(record.metrics.peak_sampled_disk_bytes>0);
    const result=JSON.parse(fs.readFileSync(metrics+'.stdout.log','utf8'));assert.equal(result.status,'pass');assert.equal(result.rows.length,24);
    for(const row of result.rows){assert.equal(row.search.status,'pass');assert.equal(row.search.canonical_sequence,1);assert.equal(row.search.retained_vectors,2);assert(row.first_ciphertext_bytes>0);assert(row.restore_us>0);if(row.packaging==='incremental')assert(row.changed_ciphertext_bytes<row.first_ciphertext_bytes);}
    for(const row of result.rows){const key=`${row.backend}-${row.records-4}-${row.repetition}/${row.packaging}`;row.peak_sampled_snapshot_bytes=record.metrics.peak_sampled_snapshot_bytes[key];assert(row.peak_sampled_snapshot_bytes>=row.first_ciphertext_bytes);assert(row.conversion_us>0);}
    record.measurements=result.rows;record.status='pass';record.exit_code=0;
  }catch(error){record.status='fail';record.reason=error.message;record.exit_code=1;}
  record.ended_at=new Date().toISOString();save();console.log(JSON.stringify({status:record.status,reason:record.reason,manifest}));process.exitCode=record.exit_code;
}
main(process.argv.slice(2)).catch(error=>{console.error(error.message);process.exitCode=1;});
