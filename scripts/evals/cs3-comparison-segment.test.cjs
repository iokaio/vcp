// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), os = require('node:os'), crypto = require('node:crypto');
const prep = require('./authoring-prepare.cjs'), capture = require('./developer-runner.cjs'), campaign = require('./cs3-comparison.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-segment-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const put = (name, value) => { const file = path.join(root, name); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, typeof value === 'string' ? value : JSON.stringify(value)); return { path: file, sha256: sha(fs.readFileSync(file)) }; };
  put('archive/source.txt', 'unchanged verifier'); put('repository/src/crates/source.rs', '// native unchanged');
  const source = prep.identity(path.join(root, 'archive'), ['source.txt']), native = prep.identity(path.join(root, 'repository'), ['src/crates']);
  const build = put('build.json', { source_inputs: native }), directory = path.join(root, 'original'); fs.mkdirSync(directory);
  const runs = Array.from({ length: 108 }, (_, i) => ({ id: 'case-' + i, case_id: 'case-' + i, arm: ['none', 'nearest', 'candidate'][i % 3], skill: 'document-authoring', cap_micros: 600000, files: [] }));
  for (const row of runs) {
    const base = 'original/' + row.id; put(base + '/data/.init', ''); fs.unlinkSync(path.join(root, base, 'data/.init')); fs.mkdirSync(path.join(root, base, 'workspace'));
    row.profile_sha256 = put(base + '/profile.json', {}).sha256; row.prompt_sha256 = put(base + '/prompt.txt', 'identical task').sha256;
  }
  const origin = { schema: 'cs3-comparison-plan/2', directory, source, runs, spec: { build_receipt: build, executable: {} } };
  const originRef = put('original/plan.json', origin), global = put('git/original-claim.json', { directory, plan_sha256: originRef.sha256 });
  const headers = Object.fromEntries(['halt.json', 'result-document-authoring.json', 'active-block.json'].map(name => [name, put('original/' + name, { retained: name }).sha256]));
  const original_claims = [runs[0].id, runs[1].id, 'block-document-authoring'].map(id => ({ path: 'claims/' + id + '.json', sha256: put('original/claims/' + id + '.json', { original: id }).sha256 }));
  const consumed = runs.slice(0, 2).map(row => {
    const scope = { task: row.id, session: 'session', workspace: 'workspace' }, base = 'original/' + row.id;
    put(base + '/costs.json', [{ gaps: [], items: [{ collection: 'ledger', id: row.id, visibility: 'available', record: { scope, currency: 'USD', cap: '600000', settled: '0', active: '0', protected: '0', unresolved: '0', allocations: {}, overrun: false } }] }]);
    for (const view of ['tools', 'outputs', 'context']) put(base + '/' + view + '.json', [{ gaps: [], items: [] }]);
    put(base + '/stdout.jsonl', [{ type: 'accepted', scope }, { type: 'result', scope, exit_code: 1, conditions: { completed: false, unresolved_effect: false, internal_failure: true } }].map(JSON.stringify).join('\n'));
    const money = require('./cs3-comparison-policy.cjs').fields(require('./cs3-comparison-policy.cjs').reread(path.join(root, base), 600000));
    const evidence_sha256 = capture.runEvidence(path.join(root, base));
    const result = put(base + '/result.json', { id: row.id, case_id: row.case_id, arm: row.arm, scope, status: 'failed', ...money, observed_attempts: 0, evidence_sha256 });
    return { id: row.id, result_sha256: result.sha256, evidence_sha256, inventory: prep.identity(path.join(root, base), ['.']) };
  });
  const auditRef = put('audit.json', { schema: 'cs3-comparison-segment-origin/1', plan_sha256: originRef.sha256, global_claim_sha256: global.sha256,
    halt_sha256: headers['halt.json'], block_result_sha256: headers['result-document-authoring.json'], active_block_sha256: headers['active-block.json'], original_claims, consumed, remaining_ids: runs.slice(2).map(r => r.id) });
  const spec = { origin: { ...originRef, source_archive: path.join(root, 'archive') }, audit: auditRef }, specRef = put('spec.json', spec);
  const filename = require.resolve('./cs3-comparison-segment.cjs'), actualRequire = require('node:module').createRequire(filename), module = { exports: {} };
  const state = { source, prerequisiteFailure: false, prerequisiteChecks: 0 };
  // Only this isolated module substitutes public immutable commitments with this
  // synthetic campaign's commitments. Production has no override or retry mode.
  const code = fs.readFileSync(filename, 'utf8').replace(/const originHash = '[a-f0-9]+';/, `const originHash = '${originRef.sha256}';`).replace(/const auditHash = '[a-f0-9]+';/, `const auditHash = '${auditRef.sha256}';`);
  new Function('exports', 'require', 'module', '__filename', '__dirname', code)(module.exports, name => {
    if (name !== './cs3-comparison.cjs') return actualRequire(name);
    return { claimFile: () => global.path, workspaceFiles: campaign.workspaceFiles, sourceIdentity: () => state.source, buildProvenance() {},
      validateExecution() { state.prerequisiteChecks++; if (state.prerequisiteFailure) throw Error('Synthetic prerequisite rejected'); } };
  }, module, filename, path.join(root, 'repository/scripts/evals'));
  return { root, put, spec, specRef, origin, state, segment: module.exports, control: path.join(root, 'control') };
}
test('segment dry preparation is read-only; one claim preserves original prefix and exact106 assignment', t => {
  const f = fixture(t), before = prep.identity(path.join(f.root, 'original'), ['.']);
  const dry = f.segment.prepare(f.specRef.path, f.control, true);
  assert.equal(dry.status, 'validated_not_claimed'); assert.equal(dry.remaining, 106); assert.equal(f.state.prerequisiteChecks, 1);
  assert.equal(fs.existsSync(f.segment.claimFile()), false); assert.equal(fs.existsSync(f.control), false);
  const ready = f.segment.prepare(f.specRef.path, f.control), plan = JSON.parse(fs.readFileSync(ready.plan));
  assert.deepEqual(f.segment.validate(plan, ready.sha256), plan); assert.equal(plan.segment.addenda.every(r => r.status === 'failed'), true);
  assert.deepEqual(prep.identity(path.join(f.root, 'original'), ['.']), before);
  assert.equal(campaign.controlDirectory(plan), f.control); assert.equal(campaign.retainedPrefix(plan, plan.runs[0].id), true);
  assert.equal(campaign.claimed(plan, plan.runs[0].id), true); assert.equal(campaign.claimed(plan, plan.runs[2].id), false);
  assert.equal(campaign.slotReport(plan, plan.runs[1].id).actual_cost_micros, 0);
  assert.throws(() => f.segment.prepare(f.specRef.path, path.join(f.root, 'other')), /unclaimed segment1/);
  const addendum = path.join(f.control, 'addenda', plan.runs[0].id + '.json'); fs.writeFileSync(addendum, '{}');
  assert.throws(() => f.segment.validate(plan, ready.sha256), /addendum changed/);
});
test('segment refuses changed originals, unclaimed activity, duplicate ownership and non-verifier source', t => {
  const f = fixture(t), original = path.join(f.origin.directory, f.origin.runs[0].id, 'result.json'), bytes = fs.readFileSync(original);
  fs.writeFileSync(original, '{}'); assert.throws(() => f.segment.describe(f.spec, f.control), /raw evidence changed/); fs.writeFileSync(original, bytes);
  f.put('original/' + f.origin.runs[2].id + '/attempted.json', {}); assert.throws(() => f.segment.describe(f.spec, f.control), /already contains/); fs.unlinkSync(path.join(f.origin.directory, f.origin.runs[2].id, 'attempted.json'));
  f.state.source = { ...f.state.source, files: [...f.state.source.files, { path: 'changed-native.rs', bytes: 0, sha256: sha('') }] };
  assert.throws(() => f.segment.describe(f.spec, f.control), /non-verifier/); f.state.source = f.origin.source;
  const native = path.join(f.root, 'repository/src/crates/source.rs'); fs.appendFileSync(native, '//changed'); assert.throws(() => f.segment.describe(f.spec, f.control), /native executable source changed/); fs.writeFileSync(native, '// native unchanged');
  f.state.prerequisiteFailure = true; assert.throws(() => f.segment.prepare(f.specRef.path, f.control), /prerequisite rejected/); assert.equal(fs.existsSync(f.segment.claimFile()), false); f.state.prerequisiteFailure = false;
  const prepared = f.segment.prepare(f.specRef.path, f.control), plan = JSON.parse(fs.readFileSync(prepared.plan));
  f.put('control/claims/' + plan.runs[0].id + '.json', { plan_sha256: prepared.sha256, id: plan.runs[0].id });
  assert.throws(() => f.segment.validate(plan, prepared.sha256), /duplicated original/);
});
