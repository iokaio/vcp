// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const {analyze} = require('../../../scripts/evals/analyze-execution-bundle.cjs');
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
