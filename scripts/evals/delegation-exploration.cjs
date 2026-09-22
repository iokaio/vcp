// SPDX-License-Identifier: Apache-2.0
'use strict';
// Frozen CR-03 exploration fixture. No model-authored code is executed by grading.
const sources={
 'README.md':'Explore the active event-ingestion path. Source files are the authority; legacy and UI code may be unrelated. Requests have a key, a string payload, and role writer. Duplicate keys return the original result without another notification. No process execution is authorized.\n',
 'src/entry.cjs':"const {decode}=require('./decode.cjs');\nconst {authorize}=require('./policy.cjs');\nconst {submit}=require('./submit.cjs');\nexports.handle=req=>{try{const event=decode(req);authorize(req.role);return {status:200,body:submit(event)};}catch(e){return {status:e.code==='DENIED'?403:400,body:e.code};}};\n",
 'src/decode.cjs':"exports.decode=req=>{\n if(!req||typeof req.key!=='string'||!req.key||typeof req.payload!=='string')throw Object.assign(new Error('invalid event'),{code:'INVALID'});\n return {key:req.key,payload:req.payload};\n};\n",
 'src/policy.cjs':"exports.authorize=role=>{\n if(role!=='writer')throw Object.assign(new Error('writer required'),{code:'DENIED'});\n};\n",
 'src/submit.cjs':"const store=require('./store.cjs');\nconst {notify}=require('./notify.cjs');\nexports.submit=event=>{\n if(store.has(event.key))return store.get(event.key);\n const saved=store.put(event);\n notify(saved);\n return saved;\n};\n",
 'src/store.cjs':"const rows=new Map();\nexports.has=key=>rows.has(key);\nexports.get=key=>rows.get(key);\nexports.put=event=>{const saved={...event,accepted:true};rows.set(event.key,saved);return saved;};\n",
 'src/notify.cjs':"const {queue}=require('./queue.cjs');\nexports.notify=event=>queue.push({kind:'accepted',key:event.key});\n",
 'src/queue.cjs':"exports.queue=[];\n",
 'legacy/entry.cjs':"// Historical batch importer, not imported by the active entry point.\nexports.importBatch=rows=>rows.map(row=>({...row,accepted:true}));\n",
 'ui/status.cjs':"exports.label=status=>status===200?'accepted':'rejected';\n",
 'tests/examples.cjs':"// Documentation examples only; no executable checks were run for this exploration.\nexports.valid={key:'example',payload:'hello',role:'writer'};\nexports.invalid={key:'',payload:42,role:'reader'};\n",
};
const rubric={schema:'p7-exploration-rubric/2',flow:['src/entry.cjs','src/decode.cjs','src/policy.cjs','src/submit.cjs','src/store.cjs','src/notify.cjs','src/queue.cjs'],anchors:[4,2,2,4,4,2,1],errors:{INVALID:400,DENIED:403},duplicate:{path:'src/submit.cjs',line:4},inactive:['legacy/entry.cjs','ui/status.cjs']};
const prompt='Explore this repository broadly enough to explain the active event-ingestion path, input/permission failures, duplicate handling and notification side effects. Locate the active entry and its dependencies; distinguish inactive legacy code and UI code from the active path. Read relevant source, cite exact observed paths and line ranges containing the described behavior, and give a short synthesis. Do not edit files or execute processes. Return one plain JSON object, without Markdown fences, with: summary (string); flow (ordered array of {path,start_line,end_line,reason,evidence}, from active entry through notification queue, each evidence an array of source citations); errors (array of {code,status,path,line,evidence}, location is where the error originates); duplicate ({path,line,behavior,result,writes,notifications,evidence}, result is existing or new, writes and notifications are numeric counts caused by a duplicate); inactive (array of {path,active,reason,evidence} for the legacy and UI files, active is a boolean); checks ({executed,limitation}, executed is a boolean and limitation is a nonempty string). Explain duplicate handling in behavior. For each flow step, use the smallest source range containing its operative statement, often a single line. Evidence citations must cover that entire declared range. Count actual source lines; a final newline does not create an additional line. Cite only examined source. Do not claim executable checks ran.\n';
function grade(answer,context){
 if(!answer||!context||!Array.isArray(answer.flow)||!Array.isArray(answer.errors))throw Error('Bounded exploration answer and canonical read evidence required');
 const text=value=>typeof value==='string'&&value.trim().length>0&&value.length<=2048;
 const cited=(items,path,line)=>Array.isArray(items)&&items.length>0&&items.length<=16&&items.some(item=>{
  if(typeof item!=='string'||item.length>1024)return false;
  let reference=context.references.get(item);
  if(!reference){const match=/^([A-Za-z0-9_./-]+):([1-9][0-9]*)(?:-([1-9][0-9]*))?(?=$|\s|[,;])/.exec(item);if(!match)return false;reference={path:match[1],start:Number(match[2]),end:Number(match[3]??match[2])};}
  return reference.path===path&&reference.start<=line&&reference.end>=line&&reference.start>=1&&[...context.references.values()].some(read=>read.path===path&&read.start<=reference.start&&read.end>=reference.end);
 });
 // A trace may return to a caller between dependencies; the prompt never
 // restricted one visit per module. Require all active modules in first-visit
 // order and observed ranges for every bounded repeat, not an exact row count.
 const visited=new Set(),first=[];
 const flow=answer.flow.length>=rubric.flow.length&&answer.flow.length<=rubric.flow.length*3&&answer.flow.every(row=>{
  const index=rubric.flow.indexOf(row?.path);if(index<0)return false;
  const repeated=visited.has(row.path);if(!repeated){visited.add(row.path);first.push(row.path);}
  const lines=sources[row.path].match(/[^\n]*\n|[^\n]+$/g).length;
  return Number.isSafeInteger(row.start_line)&&Number.isSafeInteger(row.end_line)&&row.start_line>=1&&row.end_line>=row.start_line&&row.end_line<=lines&&(repeated||(row.start_line<=rubric.anchors[index]&&row.end_line>=rubric.anchors[index]))&&text(row.reason)&&Array.from({length:row.end_line-row.start_line+1},(_,offset)=>row.start_line+offset).every(line=>cited(row.evidence,row.path,line));
 })&&JSON.stringify(first)===JSON.stringify(rubric.flow);
 const errors=answer.errors.length===2&&Object.entries(rubric.errors).every(([code,status])=>answer.errors.filter(row=>row?.code===code&&row.status===status&&row.path===(code==='INVALID'?'src/decode.cjs':'src/policy.cjs')&&row.line===2&&cited(row.evidence,row.path,row.line)).length===1);
 const duplicate=answer.duplicate?.path===rubric.duplicate.path&&answer.duplicate.line===rubric.duplicate.line&&answer.duplicate.result==='existing'&&answer.duplicate.writes===0&&answer.duplicate.notifications===0&&text(answer.duplicate.behavior)&&cited(answer.duplicate.evidence,answer.duplicate.path,answer.duplicate.line);
 const inactive=Array.isArray(answer.inactive)&&answer.inactive.length===rubric.inactive.length&&rubric.inactive.every(path=>answer.inactive.filter(row=>row?.path===path&&row.active===false&&text(row.reason)&&cited(row.evidence,path,1)).length===1);
 const checks=answer.checks?.executed===false&&text(answer.checks.limitation);
 return {pass:text(answer.summary)&&checks&&flow&&errors&&duplicate&&inactive,flow,errors,duplicate,inactive,checks,manual_evidence_usefulness_review_required:true};
}
module.exports={sources,rubric,prompt,grade};
