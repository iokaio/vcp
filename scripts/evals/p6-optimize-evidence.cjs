// SPDX-License-Identifier: Apache-2.0
'use strict';
// Local canonical reports from a finished trial. Never starts or resumes work.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {spawnSync} = require('node:child_process');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = value => JSON.stringify(value, null, 2) + '\n';
function read(file, maximum = 16 * 1024 * 1024) {
  for(let current=path.resolve(file);;current=path.dirname(current)) {
    if(fs.lstatSync(current).isSymbolicLink()) throw Error('Symlink or junction rejected');
    if(path.dirname(current)===current) break;
  }
  const stat = fs.statSync(file);
  if (!stat.isFile() || stat.size > maximum) throw Error('Bounded regular file required');
  return fs.readFileSync(file);
}
function child(root, id) {
  if (typeof id !== 'string' || !/^[A-Za-z0-9_-]+$/.test(id)) throw Error('Invalid trial ID');
  return path.join(root, id);
}
function collect(planFile, rowId, destination) {
  const planBytes = read(planFile), plan = JSON.parse(planBytes);
  const directory = path.resolve(path.dirname(planFile));
  const row = plan.runs.find(r => r.id === rowId);
  if (plan.schema !== 'p6-live-plan/1' || path.resolve(plan.directory) !== directory || !row || sha(read(plan.executable, 1024*1024*1024)) !== plan.executable_sha256) throw Error('Frozen trial identity mismatch');
  const base = child(directory, rowId), resultBytes = read(path.join(base, 'result.json'));
  for(const name of ['workspace','data']) {
    if(fs.lstatSync(path.join(base,name)).isSymbolicLink()) throw Error('Linked trial storage rejected');
  }
  const result = JSON.parse(resultBytes);
  if (!result.scope?.task || !['completed', 'failed', 'cancelled'].includes(result.status)) throw Error('Finished canonical task required');
  fs.mkdirSync(destination); // New evidence only; never overwrite an earlier collection.
  const captures = [];
  function invoke(name, command) {
    const args = ['--format','jsonl','--non-interactive','--workspace',path.join(base,'workspace'),'--data-dir',path.join(base,'data'), ...command];
    const output = spawnSync(plan.executable, args, {shell:false, windowsHide:true, encoding:'utf8', timeout:60000, maxBuffer:16*1024*1024, stdio:['ignore','pipe','pipe']});
    fs.writeFileSync(path.join(destination,name+'.jsonl'),output.stdout || '',{flag:'wx'});
    fs.writeFileSync(path.join(destination,name+'.stderr.txt'),output.stderr || '',{flag:'wx'});
    if(output.error || output.status !== 0) throw Error(`Canonical command failed: ${name}`);
    const frames = output.stdout.trim().split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
    const data = frames.findLast(frame => frame.type === 'result')?.data;
    if (!data) throw Error(`Missing canonical result: ${name}`);
    captures.push({name,args,stdout_sha256:sha(Buffer.from(output.stdout))});
    return data;
  }
  function costs(name) {
    const items=[]; let cursor=null, page=0;
    do {
      const args=['inspect',result.scope.task,'--view','costs','--limit','128'];
      if(cursor) args.push('--cursor',JSON.stringify(cursor));
      if(page >= 64) throw Error('Cost pagination bound');
      const data=invoke(`${name}-${page++}`,args);
      if(!Array.isArray(data.items) || data.gaps?.length || data.items.some(i=>i.visibility !== 'available')) throw Error('Incomplete canonical costs');
      items.push(...data.items.map(i=>({id:i.id,collection:i.collection,record:i.record})));
      cursor=data.next_cursor;
    } while(cursor);
    return items.sort((a,b)=>a.id.localeCompare(b.id));
  }
  const beforeCosts=costs('costs-before');
  const status=invoke('status',['optimize','status']);
  const baseline=invoke('report-before',['optimize','report']);
  const current=invoke('report-after',['optimize','report']);
  if(!baseline.report?.id || !current.report?.id) throw Error('Missing canonical report identity');
  const comparison=invoke('compare',['optimize','compare',baseline.report.id,current.report.id]);
  const afterCosts=costs('costs-after');
  if(JSON.stringify(beforeCosts)!==JSON.stringify(afterCosts)) throw Error('Cost records changed during local optimization inspection');
  const evidence={schema:'p6-optimize-evidence/1',plan_sha256:sha(planBytes),trial_result_sha256:sha(resultBytes),executable_sha256:plan.executable_sha256,collector_sha256:sha(read(__filename)),row_id:rowId,scope:result.scope,status,baseline,current,comparison,captures,cost_records_unchanged:true,provider_calls:0,policy_changes:0,limitations:['Two reports inspect the same retained trial, not an optimization treatment or causal improvement.','Each isolated trial is sparse local history; campaign-level matched metrics remain separate.','Adaptive interview and selected policy apply/rollback are established by the native fixtures, not fabricated in this finished trial.']};
  fs.writeFileSync(path.join(destination,'evidence.json'),json(evidence),{flag:'wx'});
  return {evidence:path.join(destination,'evidence.json'),report_before:baseline.report.id,report_after:current.report.id,cost_records_unchanged:true};
}
if(require.main===module) {
  try {
    const [plan,row,destination,...rest]=process.argv.slice(2);
    if(!plan || !row || !destination || rest.length) throw Error('Usage: node scripts/evals/p6-optimize-evidence.cjs <plan.json> <finished-row-id> <new-output-directory>');
    process.stdout.write(json(collect(plan,row,destination)));
  } catch(error) { console.error(error.message); process.exitCode=1; }
}
