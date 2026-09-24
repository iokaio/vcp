// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {validateReady,attachLocal,launchLocal} from '../dist/local.js';
const pin={pid:3,created:'18446744073709551615',principal:{sid:[1,2,3,4,5,6,7,8],session:1},image:'C:\\tools\\vcp.exe',file:{volume:1,index:'18446744073709551615'}};
const ready={schema:'vcp-local-ready/1',server:pin,scope:{workspace:'workspace',session:'session'},role:'controller'};
test('private bootstrap shape validates native counters and exact role/scope without trusting claims',()=>{
 assert.equal(validateReady(ready,'controller').server.created,pin.created);
 for(const value of [{...ready,role:'observer'},{...ready,extra:'private'},{...ready,server:{...pin,pid:0}},{...ready,server:{...pin,created:'18446744073709551616'}},{...ready,scope:{workspace:'workspace',session:'session',extra:true}}])assert.throws(()=>validateReady(value,'controller'));
 assert.throws(()=>validateReady(ready,'controller',{workspace:'other',session:'session'}));
 const attachment={endpoint:'\\\\.\\pipe\\vcp-local-'+ 'a'.repeat(64),server:pin,ticket:'b'.repeat(64)};
 assert.equal(validateReady({...ready,attachment},'controller').attachment.endpoint,attachment.endpoint);
 assert.throws(()=>validateReady({...ready,attachment:{...attachment,server:{...pin,pid:9}}},'controller'));
});
test('forged or serialized attachment handles rejected before process creation',()=>{
 for(const attachment of [{},JSON.parse('{}'),null])assert.throws(()=>attachLocal({executable:'C:\\vcp.exe',attachment}),e=>e.code==='INVALID_ARGUMENT');
 assert.throws(()=>launchLocal({executable:'vcp.exe',workspace:'relative',role:'observer'}),e=>e.code==='INVALID_ARGUMENT');
});

test('publisher bootstrap accepts only explicit controller profile selection, never key values',()=>{
 const base={executable:'C:\\vcp.exe',workspace:'C:\\workspace',role:'controller'};
 for(const publisher of [null,{}, {profile:'relative'}, {profile:'C:\\private\\publisher.json',key:'secret'}, {profile:'C:\\private\\publisher.json',automatic:true}]) {
   assert.throws(()=>launchLocal({...base,publisher}),e=>e.code==='INVALID_ARGUMENT');
 }
 assert.throws(()=>launchLocal({...base,role:'observer',publisher:{profile:'C:\\private\\publisher.json'}}),e=>e.code==='INVALID_ARGUMENT');
});

test('observer reconnect references are serializable hints with exact bounded noncredential fields', async () => {
 const {validateObserverReconnectReference,reconnectObserverLocal}=await import('../dist/local.js');
 const reference={endpoint:'\\\\.\\pipe\\vcp-local-'+ 'a'.repeat(64),server:pin,scope:ready.scope};
 const copy=validateObserverReconnectReference(JSON.parse(JSON.stringify(reference)));
 assert.deepEqual(copy,reference); copy.server.principal.sid[0]=99; assert.equal(reference.server.principal.sid[0],1);
 assert.deepEqual(validateReady({...ready,observer_reconnect:reference},'controller').observer_reconnect,reference);
 for(const bad of [{...reference,ticket:'secret'},{...reference,role:'controller'},{...reference,endpoint:'\\\\remote\\pipe\\vcp-local-'+ 'a'.repeat(64)},{...reference,scope:{...ready.scope,token:'secret'}},{...reference,server:{...pin,extra:'secret'}}]) {
  assert.throws(()=>validateObserverReconnectReference(bad));
  assert.throws(()=>reconnectObserverLocal({executable:'C:\\vcp.exe',reference:bad}));
 }
 assert.throws(()=>validateReady({...ready,observer_reconnect:{...reference,scope:{...ready.scope,workspace:'other'}}},'controller'));
});
