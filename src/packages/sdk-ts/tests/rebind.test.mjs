// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {decodeRebindResult,rebindLocal} from '../dist/rebind.js';
const data={workspace:'workspace',root:'C:\\moved',binding_revision:'9007199254740993',authority:'7',workspace_revision:'19',trust:'untrusted',rebound:true,history_preserved:true,tasks_resumed:false,descriptor:'PRIVATE_DESCRIPTOR',next_action:'PRIVATE_MESSAGE'};
const wire=data=>JSON.stringify({schema_version:1,type:'result',correlation:'command',scope:null,exit_code:0,data})+'\n';
test('rebind keeps exact durable identity and counters and excludes private native output',()=>{
 const result=decodeRebindResult(wire(data),'workspace');
 assert.equal(result.binding_revision,'9007199254740993'); assert.equal(result.root,data.root);
 assert.equal(JSON.stringify(result).includes('PRIVATE'),false);
 for(const patch of [{workspace:'other'},{tasks_resumed:true},{history_preserved:false},{binding_revision:9007199254740992},{authority:'18446744073709551616'},{trust:'maybe'},{root:'relative'},{rebound:'true'}]) assert.throws(()=>decodeRebindResult(wire({...data,...patch}),'workspace'));
 assert.throws(()=>decodeRebindResult(wire(data)+wire(data),'workspace'));
 assert.throws(()=>decodeRebindResult('{broken','workspace'));
});
test('rebind rejects unsafe arguments before process creation',()=>{
 for(const patch of [{executable:'vcp'},{workspace:'relative'},{data:'relative'},{workspaceId:'bad/id'}]) assert.throws(()=>rebindLocal({executable:'C:\\vcp.exe',workspace:'C:\\project',workspaceId:'workspace',...patch}));
});
