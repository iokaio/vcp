// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const {analyze,read} = require('../../../scripts/evals/analyze-execution-bundle.cjs');
function fixture() {
  const event = (id,cause,kind) => ({event:{event:{id,causation:cause,kind,workspace:'w',session:'s',task:'t'}},artifact_links:[]});
  return {schema_version:1,kind:'inspection_bundle',source_watermark:'9',task:{scope:{workspace:'w',session:'s',task:'t'},state:'completed'},
    history:[{rows:[event('failed',null,'verification')],next_cursor:'next'},
      {rows:[event('repair','failed','effect'),event('passed','repair','verification')],next_cursor:null}],
    views:{verification:[{items:[]}],costs:[{items:[{collection:'ledger',record:{scope:{task:'t'},settled:'100',active:'0',unresolved:'900'}}]}]}};
}
test('offline report reconstructs explicit edges across pages without declaring quality or settlement', () => {
  const report = analyze(fixture());
  assert.equal(report.facts.event_count,3);
  assert.deepEqual(report.facts.causal_edges,[{cause:'failed',event:'repair'},{cause:'repair',event:'passed'}]);
  assert.equal(report.facts.accounting[0].unresolved,'900');
  assert.equal(report.assessment.quality,'requires_independent_scenario_gates');
  assert.deepEqual(report.gaps,[]);
});
test('missing evidence is explicit and duplicate or cross-task events are rejected', () => {
  const bundle = fixture(); bundle.history[1].rows[0].event.event.causation = 'outside';
  bundle.history[1].rows[1].artifact_links = [{id:'a',availability:'purged'}];
  bundle.views.verification[0].next_cursor = 'missing';
  const report = analyze(bundle);
  assert.deepEqual(report.gaps.map(gap => gap.kind).sort(),['artifact_visibility','causal_parent_outside_projection','view_incomplete']);
  bundle.history[1].rows[0].event.event.task = 'other';
  assert.throws(() => analyze(bundle),/scope/);
  const duplicate = fixture(); duplicate.history[1].rows.push(duplicate.history[1].rows[0]);
  assert.throws(() => analyze(duplicate),/Duplicate/);
});

test('phase statistics separate incomplete and failed observations and preserve missing coverage', () => {
  const bundle = fixture();
  assert.deepEqual(analyze(bundle).facts.lifecycle_statistics,{available:false,reason:'not_collected',groups:[]});
  const observation = (sequence,status,elapsed_micros) => ({sequence,status,elapsed_micros,phase:'verification',scope:bundle.task.scope});
  bundle.lifecycle_diagnostics = {schema_version:1,available:true,owner:'owner-1',window:'current_owner_only',complete_history:false,dropped:2,
    observations:[observation(1,'succeeded',20),observation(2,'succeeded',10),observation(3,'failed',40),observation(4,'active',900),observation(5,'interrupted',700)]};
  const stats = analyze(bundle).facts.lifecycle_statistics;
  assert.equal(stats.complete_history,false);
  assert.equal(stats.dropped,2);
  assert.equal(stats.groups.length,4);
  assert.equal(stats.groups[0].count,2);
  assert.equal(stats.groups[0].p50_micros,10);
  assert.equal(stats.groups[0].p95_micros,20);
  assert.equal(stats.groups[1].qualification,'single_observation');
  assert.equal(stats.total_micros,undefined);
  bundle.lifecycle_diagnostics.observations[0].scope = {...bundle.task.scope,task:'foreign'};
  assert.throws(() => analyze(bundle),/scope/);
  bundle.lifecycle_diagnostics.observations[0].scope = bundle.task.scope;
  bundle.lifecycle_diagnostics.observations.push(bundle.lifecycle_diagnostics.observations[0]);
  assert.throws(() => analyze(bundle),/duplicate diagnostic/);
});

test('archive analysis verifies scoped retained bytes and rejects corruption or escaping references', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(),'vcp-archive-'));
  const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
  try {
    const bundle = fixture();
    const bytes = Buffer.from(JSON.stringify({schema_version:1,verification:'v',reason_code:'execution.verification_repair',repeats:1,threshold:3,pause_requested:false}));
    const descriptor = {spec:{id:'artifact-1',scope:bundle.task.scope,schema:'execution-completion-repair/1'},state:'complete',length:String(bytes.length),sha256:hash(bytes)};
    bundle.views.tools = [{items:[{collection:'artifact',visibility:'available',record:descriptor}]}];
    bundle.views.verification[0].items = [{collection:'verification',record:{id:'v',checks:[{specification:'check',outcome:{status:'failed'},exit_code:1,output:'check-output'}]}}];
    const bundleBytes = Buffer.from(JSON.stringify(bundle));
    fs.writeFileSync(path.join(root,'inspection-bundle.json'),bundleBytes);
    fs.mkdirSync(path.join(root,'artifacts'));
    fs.writeFileSync(path.join(root,'artifacts/artifact-1.bin'),bytes);
    const manifest = {schema_version:1,scope:bundle.task.scope,bundle:{path:'inspection-bundle.json',bytes:bundleBytes.length,sha256:hash(bundleBytes)},
      artifacts:[{descriptor,path:'artifacts/artifact-1.bin',bytes:bytes.length,sha256:hash(bytes)}]};
    const save = () => {
      const value = Buffer.from(JSON.stringify(manifest));
      fs.writeFileSync(path.join(root,'manifest.json'),value);
      fs.writeFileSync(path.join(root,'manifest.sha256'),hash(value));
    };
    save();
    const report = read(root);
    assert.equal(report.archive.verified_artifacts,1);
    assert.equal(report.archive.repairs[0].verification_present,true);
    assert.equal(report.archive.repairs[0].observed_checks[0].exit_code,1);
    assert.equal(report.analysis.assessment.quality,'requires_independent_scenario_gates');
    descriptor.state = 'aborted';
    const partialBundle = Buffer.from(JSON.stringify(bundle));
    fs.writeFileSync(path.join(root,'inspection-bundle.json'),partialBundle);
    manifest.bundle.bytes = partialBundle.length; manifest.bundle.sha256 = hash(partialBundle); save();
    const partial = read(root);
    assert.equal(partial.archive.partial_captures.length,1);
    assert.equal(partial.archive.repairs.length,0,'partial JSON cannot establish a completed repair decision');
    assert.equal(partial.archive.partial_captures[0].state,'aborted');
    descriptor.state = 'complete';
    fs.writeFileSync(path.join(root,'inspection-bundle.json'),bundleBytes);
    manifest.bundle.bytes = bundleBytes.length; manifest.bundle.sha256 = hash(bundleBytes); save();
    fs.writeFileSync(path.join(root,'artifacts/artifact-1.bin'),'corrupt');
    assert.throws(() => read(root),/integrity mismatch/);
    fs.writeFileSync(path.join(root,'artifacts/artifact-1.bin'),bytes);
    manifest.artifacts[0].path = '../private.bin'; save();
    assert.throws(() => read(root),/artifact identity/);
    manifest.artifacts[0].path = 'artifacts/artifact-1.bin';
    manifest.scope = {...bundle.task.scope,task:'foreign'}; save();
    assert.throws(() => read(root),/scope/);
    fs.writeFileSync(path.join(root,'manifest.sha256'),'0'.repeat(64));
    assert.throws(() => read(root),/manifest hash/);
  } finally {
    if (path.dirname(root) !== fs.realpathSync(os.tmpdir()) && path.dirname(root) !== path.resolve(os.tmpdir())) throw Error('Unexpected fixture cleanup path');
    fs.rmSync(root,{recursive:true,force:true});
  }
});

test('skipped tool dispatch stays separate from completed processing and failed dispatch', () => {
  const bundle = fixture();
  bundle.lifecycle_diagnostics = {schema_version:1,available:true,owner:'owner-1',window:'current_owner_only',complete_history:false,dropped:0,
    observations:['skipped','succeeded','failed','interrupted'].map((status,sequence) => ({sequence,status,elapsed_micros:10,phase:'tool_dispatch',scope:bundle.task.scope}))};
  const report = analyze(bundle);
  assert.deepEqual(report.facts.lifecycle_statistics.groups.map(group => group.status),['skipped','succeeded','failed','interrupted']);
  assert.match(report.facts.lifecycle_statistics.interpretation,/not that an effect or verification passed/);
  assert.equal(report.assessment.quality,'requires_independent_scenario_gates');
  bundle.lifecycle_diagnostics.observations[0].status='invented_success';
  assert.throws(() => analyze(bundle),/Invalid diagnostic observation/);
});
