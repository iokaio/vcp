// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const runner=require('../../../scripts/evals/delegation-live-runner.cjs');
const findings=()=>runner.rubric.defects.map(d=>({...d,kind:'defect',trigger:'The specified boundary input',consequence:'Returned value violates documented contract',uncertainty:'Demonstrated by comparison; caller assumptions are bounded',evidence:[d.path+':1','base/'+d.path+':1']}));
test('independent review rubric detects introduced and pre-existing bugs, rejects benign false positives',()=>{
 assert.equal(runner.gradeReview({findings:findings()}).pass,true);
 assert.equal(runner.gradeReview({findings:findings().slice(0,1)}).pass,false);
 const wrong=findings();wrong[1].introduced_by_change='introduced';assert.equal(runner.gradeReview({findings:wrong}).pass,false);
 const falsePositive={...findings()[0],path:'receipt.cjs'};
 assert.equal(runner.gradeReview({findings:[...findings(),falsePositive]}).false_positives,1);
 const ungrounded=findings();ungrounded[0].evidence=['shipping.cjs:1'];assert.equal(runner.gradeReview({findings:ungrounded}).pass,false);
 const alternate=findings();alternate[1].reproduction={arguments:[15,10],expected:2,actual:1};assert.equal(runner.gradeReview({findings:alternate}).pass,true);
 alternate[1].reproduction.actual=0;assert.equal(runner.gradeReview({findings:alternate}).pass,false);
 alternate[1].reproduction={arguments:[20,10],expected:2,actual:2};assert.equal(runner.gradeReview({findings:alternate}).pass,false);
});
function state(){return {records:{ledger:{collection:'ledger',value:{currency:'USD',cap:'1000',active:'0',unresolved:'0',settled:'30',overrun:false}},root:{collection:'attempt',value:{id:'a',scope:{task:'root'},phase:'settled',role:'main',charged:'10'}},child:{collection:'attempt',value:{id:'b',scope:{task:'child'},phase:'settled',role:'child',charged:'20'}},sa:{collection:'settlement',value:{attempt:'a',applied:true,observation:{final_usage:true}}},sb:{collection:'settlement',value:{attempt:'b',applied:true,observation:{final_usage:true}}}}};}
test('root accounting includes retained child and rejects unknown, missing or foreign support cost',()=>{
 assert.equal(runner.accounting(state(),1000,'root','child').actual_cost_micros,30);
 for(const mutate of [s=>s.records.ledger.value.unresolved='1',s=>delete s.records.sb,s=>s.records.child.value.scope.task='foreign',s=>s.records.child.value.charged='1',s=>s.records.child.value.previous='retry']){
  const s=state();mutate(s);assert.throws(()=>runner.accounting(s,1000,'root','child'));
 }
});
test('preparation refuses aggregate exposure overflow before reading model config or creating trial',()=>{
 const temp=fs.mkdtempSync(path.join(os.tmpdir(),'vcp-delegation-contract-')),spec=path.join(temp,'spec.json'),trial=path.join(temp,'trial');
 fs.writeFileSync(spec,JSON.stringify({stage:'generation',stage_cap_usd:'5.0',overall_cap_usd:'10.0',prior_exposure_micros:5000001}));
 assert.throws(()=>runner.prepare(spec,trial),/ceiling/);assert.equal(fs.existsSync(trial),false);
 fs.rmSync(temp,{recursive:true});
});
