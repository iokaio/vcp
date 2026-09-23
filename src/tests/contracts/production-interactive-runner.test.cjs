// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const readline = require('node:readline');
const {Readable} = require('node:stream');
const {spawn, spawnSync} = require('node:child_process');
const {controlFrame,controlWriter} = require('../../../scripts/evals/production-interactive-qualification.cjs');

test('native control pipe closing during a pending write preserves supervisor cleanup', {timeout:10000}, async () => {
  const child=spawn(process.execPath,['-e',"process.stdout.write('PIPE-READY');setTimeout(()=>process.exit(0),200)"],{windowsHide:true,shell:false,stdio:['pipe','pipe','pipe'],env:{SystemRoot:process.env.SystemRoot||''}});
  const failures=[],sent=[];
  const send=controlWriter(child.stdin,error=>failures.push(error),value=>sent.push(value));
  const closed=new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',(code,signal)=>resolve({code,signal}));});
  try {
    await new Promise((resolve,reject)=>{child.stdout.once('data',bytes=>bytes.toString().includes('PIPE-READY')?resolve():reject(Error('Missing ready marker')));child.once('error',reject);});
    // Keep a real OS write pending while the non-reading peer exits. This
    // oversized transport-only payload deterministically exposes the race;
    // it is not an admissible command for the production PTY driver.
    send({action:'write',text:'x'.repeat(1024*1024)});
    const deadline=Date.now()+3000;
    while(!failures.length&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,10));
    assert.equal(failures.length,1,'Closed reader must be a supervised control failure');
    assert.equal(send({action:'terminate'}),false,'Cleanup must not write repeatedly to the broken pipe');
    assert.deepEqual(await closed,{code:0,signal:null});
    assert.equal(failures.length,1,'Write callback and stream error must not double-report');
    assert.equal(sent.length,1);
  } finally {
    if(child.exitCode===null&&child.signalCode===null)child.kill();
    await closed;
  }
});

test('PTY controls are independently parseable JSON lines with exact terminal bytes', async () => {
  const controls = [
    {action:'write',text:'/pause\r'},
    {action:'write',text:'/status\r/cost\r'},
    {action:'write',text:'/resume\r'},
    {action:'write',text:'quoted "text", slash \\, café 漢字\r\n'},
    {action:'write',text:'/exit\r'},
    {action:'terminate'},
  ];
  const encoded = controls.map(controlFrame).join('');
  // The native driver uses BufRead::lines followed by from_str on EACH line.
  // Fragment delivery independently of control boundaries, as a pipe can do.
  const bytes = Buffer.from(encoded);
  const input = Readable.from(Array.from({length:Math.ceil(bytes.length/3)},(_,i)=>bytes.subarray(i*3,i*3+3)));
  const observed = [];
  for await (const line of readline.createInterface({input})) observed.push(JSON.parse(line));
  assert.deepEqual(observed, controls);
  assert.equal(encoded.split('\n').length, controls.length + 1);
});

const driver = process.env.VCP_DELEGATION_PTY_DRIVER
  || path.resolve(__dirname,'../../../artifacts/p7-owner-native-package-v2/delegation-pty-driver.exe');

test('production control frames reach the native PTY driver without provider access', {
  skip:process.platform !== 'win32' || !fs.existsSync(driver), timeout:30000,
}, async t => {
  for (const mode of ['pause-resume-exit','terminate']) {
    await t.test(mode, async () => {
      const root = fs.mkdtempSync(path.join(os.tmpdir(),'vcp-production-pty-framing-'));
      let child, closed=false, close,writeControl;
      try {
        const script = path.join(root,'terminal.cjs');
        fs.writeFileSync(script,[
          "'use strict';",
          "if (process.env.OPENROUTER_API_KEY) process.exit(91);",
          "process.stdout.write('FRAMING-READY\\n');",
          "let input=''; const seen=new Set();",
          "process.stdin.on('data', bytes=>{",
          " input+=bytes.toString('utf8');",
          " for(const [command,marker] of [['/pause','FRAMING-PAUSED'],['/resume','FRAMING-RESUMED']])",
          "  if(input.includes(command)&&!seen.has(command)){seen.add(command);process.stdout.write(marker+'\\n');}",
          " if(input.includes('/exit')) process.exit(seen.has('/pause')&&seen.has('/resume')?0:92);",
          "});",
          "setInterval(()=>{},1000);",
        ].join('\n'));
        const spec = path.join(root,'driver-spec.json');
        fs.writeFileSync(spec,JSON.stringify({executable:process.execPath,workspace:root,arguments:[script]}));
        const env={...process.env};delete env.OPENROUTER_API_KEY;
        child=spawn(driver,[spec],{windowsHide:true,shell:false,stdio:['pipe','pipe','pipe'],env});
        close=new Promise(resolve=>child.once('close',(code,signal)=>{closed=true;resolve({code,signal});}));
        let text='',stderr='',failure;
        const events=[];
        child.once('error',error=>{failure=error;});
        writeControl=controlWriter(child.stdin,error=>{failure=error;});
        child.stderr.on('data',bytes=>{stderr+=bytes.toString('utf8');});
        const lines=readline.createInterface({input:child.stdout});
        lines.on('line',line=>{
          try {const event=JSON.parse(line);events.push(event);if(event.type==='output')text+=event.text;}
          catch(error){failure=error;}
        });
        async function waitFor(predicate,label) {
          const deadline=Date.now()+5000;
          while(!predicate()) {
            if(failure)throw failure;
            assert.ok(!closed,`Driver closed before ${label}: ${stderr}`);
            assert.ok(Date.now()<deadline,`Deadline waiting for ${label}: ${text} ${stderr}`);
            await new Promise(resolve=>setTimeout(resolve,20));
          }
        }
        await waitFor(()=>text.includes('FRAMING-READY'),'startup');
        if(mode==='pause-resume-exit') {
          assert.equal(writeControl({action:'write',text:'/pause\r'}),true);
          await waitFor(()=>text.includes('FRAMING-PAUSED'),'pause bytes');
          assert.equal(writeControl({action:'write',text:'/resume\r'}),true);
          await waitFor(()=>text.includes('FRAMING-RESUMED'),'resume bytes');
          assert.equal(writeControl({action:'write',text:'/exit\r'}),true);
        } else assert.equal(writeControl({action:'terminate'}),true);
        await waitFor(()=>events.some(event=>event.type==='exit'),'structured PTY exit');
        const status=await close;
        assert.deepEqual(status,{code:0,signal:null},stderr);
        assert.equal(events.filter(event=>event.type==='started').length,1);
        assert.equal(events.filter(event=>event.type==='exit').length,1);
        if(mode==='pause-resume-exit')assert.equal(events.find(event=>event.type==='exit').code,0);
        assert.equal(failure,undefined);
      } finally {
        if(child&&!closed) {
          writeControl?.({action:'terminate'});
          await Promise.race([close,new Promise(resolve=>setTimeout(resolve,2000))]);
          if(!closed) {
            spawnSync(path.join(process.env.SystemRoot,'System32/taskkill.exe'),['/PID',String(child.pid),'/T','/F'],{windowsHide:true,timeout:5000,stdio:'ignore'});
            await Promise.race([close,new Promise(resolve=>setTimeout(resolve,2000))]);
          }
        }
        // This test owns its freshly allocated directory exclusively.
        fs.rmSync(root,{recursive:true,force:true});
      }
    });
  }
});
