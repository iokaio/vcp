// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'), assert=require('node:assert/strict');
const {taskboard,inventory,sqlAgreement}=require('./acceptance.cjs');
const {fixture}=require('./fixture.cjs');
test('A complete oracle and restart preserve every field and reject partial invalid/conflict writes',async()=>{
  const app=fixture('A'),state=await taskboard(app.request,'full');
  await taskboard(app.request,'restart',state);await taskboard(app.request,'fault',state);
  assert.equal(state.expected.tasks.length,5);
});
test('A export-only phase does not require later import feature',async()=>{
  const app=fixture('A');const request=(m,p,b)=>{assert(p!=='/api/tasks/import');return app.request(m,p,b);};
  await taskboard(request,'restart-export',await taskboard(request,'export'));
});
test('A oracle detects invalid-document partial writes',async()=>{await assert.rejects(taskboard(fixture('A','partial').request,'full'),/Rejected import wrote data/);});
test('B transactional idempotency/concurrency/restart and SQL audit agreement',async()=>{
  const app=fixture('B'),state=await inventory(app.request,'full');
  await inventory(app.request,'restart',state);await inventory(app.request,'fault',state);
  const observed={uniqueOperationId:true,onHand:state.expected.stock,adjustments:state.expected.history};
  sqlAgreement(state,observed);
  assert.throws(()=>sqlAgreement(state,{...observed,uniqueOperationId:false}),/uniqueness/);
  assert.throws(()=>sqlAgreement(state,{...observed,onHand:state.expected.stock+1}));
  assert.throws(()=>sqlAgreement(state,{...observed,adjustments:observed.adjustments.slice(1)}),/SQL and API/);
});
test('B oracle refuses non-idempotent old-version retry',async()=>{await assert.rejects(inventory(fixture('B','duplicate').request,'full'),/Expected 200; received 412/);});
