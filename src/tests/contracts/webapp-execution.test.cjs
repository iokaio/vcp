// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const {embeddedInventory,gradeNative,validateDocument,nearMissOracle,runNonBrowser}=require('../../../scripts/evals/webapp-execution.cjs');
test('fixed renderer-readiness expression parses without executing page code',()=>{
  const source=require('node:fs').readFileSync(require('node:path').join(__dirname,'../support/windows/webapp/FrozenWebHost.cs'),'utf8');
  const expression=source.match(/new Promise[^\\]+/)[0];assert.doesNotThrow(()=>new(require('node:vm').Script)(expression));
});
test('UI keyboard oracle joins the actual typed control rather than a focusable label decoy',()=>{
  const source=require('node:fs').readFileSync(require('node:path').join(__dirname,'../support/windows/webapp/UiArtifactHost.cs'),'utf8');
  const helpers=JSON.parse(source.match(/const string UiHelpers=("[^\r\n]+");/)[1]);
  const suffix=JSON.parse(source.match(/string script="\(\(\) => \{"\+UiHelpers\+("const e=document\.activeElement[^\r\n]+");/)[1]);
  const element=(name,type)=>({type,disabled:false,textContent:name,getAttribute:()=>null,getClientRects:()=>[{}],getBoundingClientRect:()=>({width:20,height:20,left:1,top:1,right:21,bottom:21}),matches:()=>true});
  const search=element('Search','search'),wrongType=element('Search','checkbox'),decoy=element('Search',undefined),clear=element('Clear selection',undefined);
  const document={activeElement:decoy,querySelectorAll:selector=>selector==='input'?[wrongType,search]:[clear],getElementById:()=>null};
  const context={document,innerWidth:800,innerHeight:600,getComputedStyle:()=>({visibility:'visible',opacity:'1',outlineStyle:'solid',outlineWidth:'2px',outlineColor:'rgb(0,0,0)',boxShadow:'none'})};
  const expression='(() => {'+helpers+suffix;
  const observe=()=>require('node:vm').runInNewContext(expression,context,{timeout:1000});
  assert.equal(observe().name,'');
  document.activeElement=wrongType;assert.equal(observe().name,'');
  document.activeElement=search;assert.equal(observe().name,'Search');
  document.activeElement=clear;assert.equal(observe().name,'Clear selection');
  clear.parentElement={};context.getComputedStyle=e=>({visibility:'visible',opacity:e===clear.parentElement?'0':'1',outlineStyle:'solid',outlineWidth:'2px',outlineColor:'rgb(0,0,0)',boxShadow:'none'});
  assert.equal(observe().name,'');
  delete clear.parentElement;document.activeElement=search;
  context.getComputedStyle=e=>({visibility:'visible',opacity:'1',outlineStyle:'none',outlineWidth:'0px',outlineColor:'rgb(0,0,0)',outlineOffset:e===search?'2px':'0px',boxShadow:'rgb(0,0,0) 0px 0px 0px 3px'});
  const staticShadow=observe();assert.equal(staticShadow.signature,staticShadow.unfocused['Clear selection']);
  assert.match(source,/baseline\[name\]\.Contains\(signature\)/);
  assert.match(source,/getComputedStyle\(e,'::before'\),getComputedStyle\(e,'::after'\)/);
  assert.match(source,/await UiClick\("Send request"\);await UiSnapshot\("ui-invalid"\)/);
});
test('all compiled executable page/script bytes match immutable v1 inventory',()=>assert.equal(embeddedInventory().inventory.length,5));
test('exact UI motion projection honors inactive animations and bounded cyclic CSS lists',()=>{
  const source=require('node:fs').readFileSync(require('node:path').join(__dirname,'../support/windows/webapp/UiArtifactHost.cs'),'utf8');
  const expression=JSON.parse(source.match(/const string UiMotionScript=("[^\r\n]+");/)[1]);
  const defaults={animationName:'none',animationDuration:'0s',animationPlayState:'running',animationIterationCount:'1',transitionProperty:'all',transitionDuration:'0s'};
  const observe=(normal={},before={},after={})=>require('node:vm').runInNewContext(expression,{document:{querySelectorAll:()=>[{}]},matchMedia:()=>({matches:true}),getComputedStyle:(_element,pseudo)=>({...defaults,...(pseudo==='::before'?before:pseudo==='::after'?after:normal)})},{timeout:1000});
  assert.equal(observe({animationName:'none',animationDuration:'2s'}).animationMs,0);
  assert.equal(observe({animationName:'pulse',animationDuration:'2s',animationPlayState:'paused'}).animationMs,0);
  assert.equal(observe({animationName:'pulse',animationDuration:'2s',animationIterationCount:'0'}).animationMs,0);
  assert.equal(observe({animationName:'none,pulse,pulse',animationDuration:'2s',animationPlayState:'running,paused,running',animationIterationCount:'infinite,infinite,0'}).animationMs,0);
  assert.equal(observe({animationName:'pulse,pulse',animationDuration:'2s',animationPlayState:'paused,running'}).animationMs,2000);
  assert.equal(observe({animationName:'none,pulse',animationDuration:'10s,20s,30s'}).animationMs,20000);
  assert.equal(observe({animationName:'"none"',animationDuration:'2s'}).animationMs,2000);
  for(const names of ['"pulse,one",pulse','pulse\\,one,pulse'])assert.equal(observe({animationName:names,animationDuration:'1s,2s',animationPlayState:'paused,running'}).animationMs,2000);
  assert.equal(observe({}, {animationName:'pulse',animationDuration:'1s'}).animationMs,1000);
  assert.equal(observe({}, {}, {animationName:'pulse',animationDuration:'1s',animationPlayState:'paused'}).animationMs,0);
  assert.equal(observe({transitionProperty:'none',transitionDuration:'2s'}).transitionMs,0);
  assert.equal(observe({transitionProperty:'opacity,all',transitionDuration:'1s,0s'}).transitionMs,0);
  assert.equal(observe({transitionProperty:'all,opacity',transitionDuration:'1s,0s'}).transitionMs,1000);
  assert.equal(observe({transitionProperty:'opacity,opacity',transitionDuration:'1s,0s'}).transitionMs,0);
  assert.equal(observe({transitionProperty:'opacity,width',transitionDuration:'1s,0s,3s'}).transitionMs,1000);
  assert.throws(()=>observe({animationName:Array(129).fill('pulse').join(',')}),/CSS list bound/);
  assert.throws(()=>observe({animationName:'x'.repeat(8193)}),/CSS list bound/);
  assert.throws(()=>observe({animationName:'pulse',animationPlayState:'bogus'}),/CSS animation shape/);
});
test('WEB oracle detects ASCII-only, type and internal-spacing regressions',()=>{
  assert.equal(nearMissOracle(value=>{if(typeof value!=='string')throw TypeError();return value.trim().replace(/[A-Z]/g,x=>x.toLowerCase());}).pass,true);
  assert.equal(nearMissOracle(value=>value.trim().toLowerCase()).pass,false);
  assert.equal(nearMissOracle(value=>String(value).trim().replace(/[A-Z]/g,x=>x.toLowerCase())).pass,false);
  assert.equal(nearMissOracle(value=>{if(typeof value!=='string')throw TypeError();return value.trim().replace(/\s+/g,' ').replace(/[A-Z]/g,x=>x.toLowerCase());}).pass,false);
});
test('native receipt cannot substitute synthetic-only or unclean evidence',()=>{
  for(const receipt of [null,{}, {outcome:'dom_observed',status:'cleaned',processes_drained:true,events:[]}])assert.throws(()=>gradeNative(receipt));
});
test('independent WEB projection oracle rejects forged pass-state values',()=>{
  const poll={url:'https://cs3-fixture.invalid/web-poll/index.html',status:'2 items',retryHidden:true,items:['Alpha','Beta']};
  validateDocument('web-poll-success',poll);
  for(const changed of [{...poll,items:['Beta','Alpha']},{...poll,items:['Alpha','Beta','Alpha']},{...poll,status:'Unable to load'},{...poll,url:'https://blocked.invalid/'}])assert.throws(()=>validateDocument('web-poll-success',changed));
  const form={url:'https://cs3-fixture.invalid/web-form/index.html',title:'Contact',value:'Ada',required:true,error:'',status:'Saved Ada.',active:'name'};
  validateDocument('web-success',form);assert.throws(()=>validateDocument('web-success',{...form,status:'Saved Bob.'}));assert.throws(()=>validateDocument('web-success',{...form,required:false}));
});
test('occupied port, absent browser and original near-miss oracles execute without installation',async()=>{
  const results=await runNonBrowser();assert.equal(results.length,3);
  assert.ok(['pass','unavailable'].includes(results[0].outcome));
  assert.equal(results[1].browser_interaction,'not_run');assert.equal(results[1].installation,false);
  assert.equal(results[2].outcome,'baseline_defect_detected');assert.equal(results[2].files_modified,false);
});
