// SPDX-License-Identifier: Apache-2.0
'use strict';
// Synthetic retirement proof only. No real global claim, private audit or dispatch.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { createRequire } = require('node:module');
const prep = require('./authoring-prepare.cjs');
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-runtime-retirement-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const put = (file, value) => {
    file = path.isAbsolute(file) ? file : path.join(directory, file);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n');
    return { path: file, sha256: sha(fs.readFileSync(file)) };
  };
  const filename = require.resolve('./cs3-runtime-amendment.cjs'), actualRequire = createRequire(filename), module = { exports: {} };
  const syntheticRoot = path.join(directory, 'synthetic-source');
  const decision = actualRequire('../../src/evals/skills/cs3-runtime-remediation/decision.json');
  const decisionRef = put(path.join(syntheticRoot, 'src/evals/skills/cs3-runtime-remediation/decision.json'), decision);
  fs.mkdirSync(path.join(syntheticRoot, 'scripts/evals'), { recursive: true });
  for (const name of ['cs3-comparison-oracle.cjs', 'cs3-runtime-boundary-oracle.cjs']) fs.copyFileSync(path.join(__dirname, name), path.join(syntheticRoot, 'scripts/evals', name));
  new Function('exports', 'require', 'module', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(
    module.exports, name => name === './cs3-comparison.cjs'
      ? { ...actualRequire(name), claimFile: () => path.join(directory, 'synthetic-common-git/original-claim.json') }
      : actualRequire(name), module, filename, path.join(syntheticRoot, 'scripts/evals'));
  const helper = module.exports, controls = path.join(directory, 'old-controls');
  const groups = helper.skills.map((skill, index) => ({ skill, ids: Array.from({ length: 18 }, (_, offset) => `original-${index}-${offset}`) }));
  const manifest = { schema: 'cs3-comparison-isolation/1', directory: controls, groups };
  const manifestRef = put('old-controls/manifest.json', manifest);
  const auditGroups = groups.map(group => {
    const control = path.join(controls, group.skill); fs.mkdirSync(path.join(control, 'claims'), { recursive: true });
    return { skill: group.skill, plan: put(path.join(control, 'plan.json'), { control_directory: control }),
      disposition: null, status: group.skill === 'skill-authoring' ? 'unqualified' : 'retired_undispatched',
      claimed_ids: [], undispatched_ids: group.ids, slots: [], review_evidence: [] };
  });
  const ids = groups.slice(1).flatMap(group => group.ids), haltFile = path.join(controls, 'global-halt.json');
  const halt = { schema: 'cs3-comparison-runtime-retirement-halt/1', manifest_sha256: manifestRef.sha256,
    reason: 'untouched_skills_superseded_by_source_bound_runtime_amendment', retired_ids: ids,
    action: 'All original isolated dispatch permanently stopped; no replay.' };
  const haltRef = { path: haltFile, sha256: sha(JSON.stringify(halt, null, 2) + '\n') };
  const audit = { schema: 'cs3-comparison-isolation-retirement/1', manifest: manifestRef, groups: auditGroups,
    pre_controls_inventory: prep.identity(controls, ['.']), retirement_halt: haltRef, retired_ids: ids,
    transferred_cap_micros: 43200000, transferred_request_ceiling: 1152, model_calls: 0 };
  put(haltFile, halt); audit.controls_inventory = prep.identity(controls, ['.']);
  let auditRef;
  function seal(mutateClaim, mutatePrepared) {
    const { controls_inventory, retirement_claim, archived_dispatch_denied_groups, ...before } = audit;
    const prepared = { ...before, schema: 'cs3-comparison-isolation-retirement-prepared/1' };
    if (mutatePrepared) mutatePrepared(prepared);
    const claim = { schema: 'cs3-runtime-retirement-claim/1', manifest: manifestRef, prepared: put('prepared.json', prepared),
      audit_path: path.join(directory, 'audit.json'), retirement_halt: audit.retirement_halt, retired_ids: audit.retired_ids,
      transferred_cap_micros: 43200000, transferred_request_ceiling: 1152 };
    if (mutateClaim) mutateClaim(claim);
    audit.retirement_claim = put(helper.retirementClaimFile(), claim);
    audit.archived_dispatch_denied_groups = 5; auditRef = put('audit.json', audit);
  }
  seal();
  return { directory, put, helper, manifest, audit, halt, seal, decision, decisionRef, check: () => helper.retirement(audit, manifest, auditRef) };
}
test('authentic synthetic preparation, exclusive claim and halt permit exactly the seventy-two unspent reservations', t => {
  const f = fixture(t);
  assert.deepEqual(f.check(), { retired_ids: f.audit.retired_ids, transferred_cap_micros: 43200000, transferred_request_ceiling: 1152 });
  assert.equal(f.audit.retired_ids.length, 72);
  assert(f.audit.retired_ids.every(id => !f.manifest.groups[0].ids.includes(id)));
});
test('retirement cannot transfer consumed SKL identities, duplicate slots, extra money, requests or activity', t => {
  for (const mutate of [
    a => a.retired_ids = [filler, ...a.retired_ids.slice(1)],
    a => a.retired_ids = [...a.retired_ids.slice(1), a.retired_ids[1]],
    a => a.retired_ids = a.retired_ids.slice(1),
    a => a.transferred_cap_micros++, a => a.transferred_request_ceiling++,
    a => a.model_calls = 1, a => a.archived_dispatch_denied_groups = 4,
  ]) {
    const f = fixture(t); mutate(f.audit); assert.throws(f.check, /seventy-two-slot/);
  }
});
const filler = 'original-0-0';
test('retirement binds exact common claim path, audit destination, manifest and unaltered prepared proof', t => {
  for (const mutate of [
    claim => claim.audit_path = path.join(path.dirname(claim.audit_path), 'foreign.json'),
    claim => claim.audit_path = 'audit.json',
    claim => claim.manifest = { ...claim.manifest, sha256: '0'.repeat(64) },
    claim => claim.transferred_cap_micros++,
    claim => claim.extra = 'not authorized',
  ]) {
    const f = fixture(t); f.seal(mutate); assert.throws(f.check, /identity differs/);
  }
  let f = fixture(t); f.seal(undefined, p => p.model_calls = 1); assert.throws(f.check, /preparation proof changed/);
  f = fixture(t); f.seal(undefined, p => p.schema = 'wrong'); assert.throws(f.check, /identity differs/);
  f = fixture(t); const claim = JSON.parse(fs.readFileSync(f.audit.retirement_claim.path));
  f.audit.retirement_claim = f.put('foreign-common/claim.json', claim); assert.throws(f.check, /seventy-two-slot/);
  f = fixture(t); fs.appendFileSync(f.audit.retirement_claim.path, ' '); assert.throws(f.check, /evidence changed/);
});
test('pre/post proof permits only the exact permanent halt, never altered plans, extra files or directories', t => {
  for (const mutate of [
    f => f.audit.controls_inventory.files[0].sha256 = '0'.repeat(64),
    f => f.audit.pre_controls_inventory.files.pop(),
    f => f.audit.pre_controls_inventory.directories.push('./extra'),
    f => f.put('old-controls/extra.json', {}),
    f => fs.mkdirSync(path.join(f.manifest.directory, 'extra')),
    f => fs.appendFileSync(path.join(f.manifest.directory, 'manifest.json'), ' '),
    f => fs.unlinkSync(f.audit.retirement_halt.path),
  ]) {
    const f = fixture(t); mutate(f); f.seal(); assert.throws(f.check);
  }
  const f = fixture(t); f.audit.retirement_halt = f.put(f.audit.retirement_halt.path, { ...f.halt, reason: 'release consumed liabilities' }); f.seal();
  assert.throws(f.check, /identity differs/);
});
test('even rehashed preparation cannot hide retired group claims or execution artifacts', t => {
  for (const relative of ['claims/claimed.json', 'result.json', 'active-block.json']) {
    const f = fixture(t), group = f.audit.groups[1], plan = JSON.parse(fs.readFileSync(group.plan.path));
    f.put(path.join(plan.control_directory, relative), { synthetic: true });
    f.audit.controls_inventory = prep.identity(f.manifest.directory, ['.']);
    const files = f.audit.controls_inventory.files.filter(row => row.path !== './global-halt.json'), directories = f.audit.controls_inventory.directories;
    f.audit.pre_controls_inventory = { ...f.audit.controls_inventory, files, content_sha256: sha(JSON.stringify({ files, directories })) };
    f.seal(); assert.throws(f.check, /Retired skill has claims or execution evidence/);
  }
});
test('source-matched decision cannot increase the fixed allocation, requests, deadline or waive qualification', t => {
  const f = fixture(t); assert.deepEqual(f.helper.decision(f.decisionRef), f.decision);
  assert.equal(f.decision.combined_cap_micros, 66613737 + 10800000 + 11650000);
  assert.equal(f.decision.combined_request_ceiling, 1779 + 288 + 306);
  for (const mutation of [
    { combined_cap_micros: 89063738 }, { combined_request_ceiling: 2374 },
    { runtime_cap_micros: 54000001 }, { transferred_cap_micros: 43200001 },
    { slot_requests: 17 }, { deadline_seconds: 601 }, { provider_timeout_seconds: 60 }, { provider_timeout_seconds: 121 }, { max_transport_retries: 1 },
    { original_skill_authoring_candidate_version: '1.0.3' }, { qualification_or_promotion_waiver: true },
  ]) {
    // Rewrite only the synthetic tracked decision, so rejection tests fixed
    // policy values rather than merely an unequal reference to source bytes.
    const ref = f.put(f.decisionRef.path, { ...f.decision, ...mutation });
    assert.throws(() => f.helper.decision(ref), /Exact fixed runtime amendment decision/);
  }
});

test('oracle correction is bound to exact original bytes, replacement bytes and only six approved case identities', t => {
  const f = fixture(t), original = f.decision.oracle_amendment;
  for (const mutation of [{ id: 'other' }, { original_oracle_sha256: '0'.repeat(64) }, { replacement_sha256: '0'.repeat(64) },
    { replacement_module: 'scripts/evals/cs3-comparison-oracle.cjs' }, { affected_case_ids: original.affected_case_ids.slice(1) }, { reason: 'waive all checks' }]) {
    const ref = f.put(f.decisionRef.path, { ...f.decision, oracle_amendment: { ...original, ...mutation } });
    assert.throws(() => f.helper.decision(ref), /source-bound six-case oracle/);
  }
  const ref = f.put(f.decisionRef.path, f.decision), sourceRoot = path.resolve(path.dirname(f.decisionRef.path), '../../../..');
  fs.appendFileSync(path.join(sourceRoot, 'scripts/evals/cs3-runtime-boundary-oracle.cjs'), '\n// changed');
  assert.throws(() => f.helper.decision(ref), /source-bound six-case oracle/);
});
