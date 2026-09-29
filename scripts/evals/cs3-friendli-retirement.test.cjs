// SPDX-License-Identifier: Apache-2.0
'use strict';
// Synthetic archived-prerequisite API and denial-child transport only. These
// tests do not authorize or retire real reservations, nor invoke a native CLI.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { createRequire } = require('node:module'), { execFileSync } = require('node:child_process');
const filename = path.join(__dirname, 'cs3-friendli-retirement.cjs'), actual = createRequire(filename);
const prep = actual('./authoring-prepare.cjs'), realCore = actual('./cs3-comparison.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const skills = ['frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-friendli-retire-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const source = path.join(directory, 'source'), privateRoot = path.join(directory, 'private'), common = path.join(directory, 'common');
  const ref = file => ({ path: file, sha256: sha(fs.readFileSync(file)) });
  const put = (file, value) => { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, typeof value === 'string' || Buffer.isBuffer(value) ? value : JSON.stringify(value, null, 2) + '\n'); return ref(file); };
  const moduleFile = path.join(source, 'scripts/evals/cs3-friendli-retirement.cjs'), childFile = path.join(source, 'scripts/evals/cs3-friendli-retirement-denial.cjs');
  put(moduleFile, fs.readFileSync(filename)); put(childFile, fs.readFileSync(path.join(__dirname, 'cs3-friendli-retirement-denial.cjs')));
  const decision = put(path.join(source, 'src/evals/skills/cs3-friendli-transfer/retirement-decision.json'), fs.readFileSync(path.resolve(__dirname, '../../src/evals/skills/cs3-friendli-transfer/retirement-decision.json')));
  const oldRoot = path.join(privateRoot, 'old-source'), archive = path.join(privateRoot, 'archive'), oldSkl = path.join(privateRoot, 'old-consumed-skl'), reader = path.join(privateRoot, 'old-reader');
  for (const root of [oldRoot, archive, oldSkl, reader]) put(path.join(root, 'preserved.txt'), 'Synthetic immutable history\n');
  fs.mkdirSync(common); const runtime = path.join(privateRoot, 'runtime'), workspaceFiles = base => prep.identity(path.join(base, 'workspace'), ['.']).files.map(row => ({ ...row, path: row.path.slice(2) })).sort((a, b) => a.path.localeCompare(b.path));
  const prompt = task => task.request + '\n';
  const groups = skills.map(skill => {
    const tasks = Array.from({ length: 6 }, (_, index) => ({ id: skill + '-fresh-' + index, skill, request: 'Synthetic independent request ' + index, files: { 'input.txt': 'Read-only source ' + index + '\n' } }));
    const control = path.join(runtime, skill), slots = path.join(runtime, 'slots'); fs.mkdirSync(path.join(control, 'claims'), { recursive: true });
    const runs = tasks.flatMap(task => ['none', 'nearest', 'candidate'].map(arm => {
      const id = task.id + '--' + arm, base = path.join(slots, id); fs.mkdirSync(path.join(base, 'data'), { recursive: true });
      for (const [name, content] of Object.entries(task.files)) put(path.join(base, 'workspace', name), content);
      return { id, case_id: task.id, skill, arm, cap_micros: 600000, call_ceiling: 16,
        profile_sha256: put(path.join(base, 'profile.json'), { synthetic: true }).sha256,
        prompt_sha256: put(path.join(base, 'prompt.txt'), prompt(task)).sha256, files: workspaceFiles(base) };
    }));
    const plan = { directory: slots, control_directory: control, runs };
    return { skill, tasks, plan, plan_ref: put(path.join(control, 'plan.json'), plan) };
  });
  const tracked = JSON.parse(fs.readFileSync(decision.path)), manifest = { directory: runtime, source: { content_sha256: tracked.historical_source_sha256 } };
  const manifestRef = { ...put(path.join(runtime, 'manifest.json'), manifest), sha256: tracked.manifest_sha256 };
  const owner = { skill: 'skill-authoring', plan_sha256: 'a'.repeat(64), manifest_sha256: manifestRef.sha256 };
  put(path.join(runtime, 'active-skill.json'), owner);
  put(path.join(runtime, 'transitions/0.json'), { manifest_sha256: manifestRef.sha256, ordinal: 0, from: null, to: owner });
  const document = { tasks: Array.from({ length: 6 }, (_, index) => ({ id: 'document-' + index })),
    runs: Array.from({ length: 18 }, (_, index) => ({ id: 'DOC-unprepared-' + index, skill: 'document-authoring', cap_micros: 600000, call_ceiling: 16 })),
    claim_path: path.join(common, 'old-doc-claim.json'), allocation: put(path.join(privateRoot, 'document-allocation.json'), { synthetic: true }) };
  const proof = { oldRoot, manifest, groups, document, historical: { protected_inventories: [oldSkl, reader].map(directory => ({ directory, inventory: prep.identity(directory, ['.']) })) } };
  const input = { decision, history: put(path.join(privateRoot, 'history.json'), { archive: put(path.join(privateRoot, 'archive-receipt.json'), { archive }) }),
    manifest: manifestRef, recovery_decision: put(path.join(privateRoot, 'recovery.json'), { synthetic: true }) };
  const state = { childCalls: 0, rejectPrerequisites: false, failChild: false };
  const local = name => {
    if (name === './cs3-comparison.cjs') return { ...realCore, claimFile: () => path.join(common, 'old-common.json'), workspaceFiles, prompt };
    if (name === './cs3-controller-recovery-qualification.cjs') return { friendliRetirementPrerequisites(value, approved) {
      assert.deepEqual(value, { history: JSON.parse(fs.readFileSync(input.history.path)), manifest: input.manifest }); assert.deepEqual(approved, input.recovery_decision);
      if (state.rejectPrerequisites) throw Error('Synthetic archived prerequisite rejection');
      // The archived semantic reviewer is explicitly fake, but its current-byte
      // inventory projection is real and recomputed on every producer admission.
      const observed = structuredClone(proof);
      observed.shared_controls = { directory: runtime, inventory: prep.identity(runtime, ['manifest.json', 'transitions',
        ...(fs.existsSync(path.join(runtime, 'active-skill.json')) ? ['active-skill.json'] : [])]) };
      observed.historical.protected_inventories = observed.historical.protected_inventories.map(({ directory }) => ({ directory, inventory: prep.identity(directory, ['.']) }));
      return observed;
    } };
    if (name === 'node:child_process') return { execFileSync(command, args, options) {
      state.childCalls++; assert.equal(command, process.execPath); assert.equal(args[0], childFile); assert.equal(args[1], oldRoot);
      assert.equal(args[2], manifestRef.path); assert.equal(args.length, 16); assert.equal(options.timeout, 180000); assert.equal(options.windowsHide, true);
      assert(!Object.keys(options.env).some(key => ['OPENROUTER_API_KEY', 'NODE_OPTIONS', 'NODE_PATH'].includes(key.toUpperCase())));
      for (const group of groups) assert(fs.existsSync(path.join(group.plan.control_directory, 'halt.json')));
      assert(fs.existsSync(document.claim_path)); assert(!fs.existsSync(args[3]));
      if (state.failChild) throw Error('Synthetic child failure');
      return JSON.stringify({ schema: 'cs3-friendli-retirement-denial/1', runtime_groups: skills, document_preparation_denied: true, transport_calls: 0, model_calls: 0 });
    } };
    return actual(name);
  };
  const module = { exports: {} };
  new Function('require', 'module', 'exports', '__filename', '__dirname', fs.readFileSync(filename, 'utf8'))(local, module, module.exports, moduleFile, path.dirname(moduleFile));
  return { directory, source, privateRoot, oldRoot, oldSkl, archive, reader, groups, proof, input, state, moduleFile, childFile, document, put, ref,
    inputFile: put(path.join(privateRoot, 'input.json'), input).path, destination: path.join(privateRoot, 'new-retirement'), api: module.exports };
}
function retire(f) { const prepared = f.api.prepare(f.inputFile, f.destination); return { prepared, audit: f.api.retire(prepared.path, prepared.sha256) }; }

test('synthetic prepare/retire/validate fences exactly 72 pristine plus 18 unprepared slots without releasing consumed liabilities', t => {
  const f = fixture(t), old = [f.oldRoot, f.archive, f.oldSkl, f.reader].map(root => prep.identity(root, ['.']));
  const { prepared, audit } = retire(f), measured = f.api.validate(audit), record = JSON.parse(fs.readFileSync(audit.path));
  assert.equal(measured.slots.length, 72); assert.equal(measured.document.runs.length, 18); assert.equal(measured.transferred_slots, 90);
  assert.equal(measured.transferred_cap_micros, 54000000); assert.equal(measured.transferred_request_ceiling, 1440);
  assert.equal(record.model_calls, 0); assert.equal(record.consumed_liabilities_released, false); assert.equal(record.barriers.length, 5);
  assert.deepEqual(measured.shared_controls, JSON.parse(fs.readFileSync(prepared.path)).proof.shared_controls);
  assert.equal(f.state.childCalls, 1); assert(fs.existsSync(f.api.claimFile()));
  assert.deepEqual([f.oldRoot, f.archive, f.oldSkl, f.reader].map(root => prep.identity(root, ['.'])), old);
  assert.throws(() => f.api.retire(prepared.path, prepared.sha256), /unconsumed/);
  assert.throws(() => f.api.prepare(f.inputFile, path.join(f.privateRoot, 'again'))); assert.equal(f.state.childCalls, 1);
});

test('every local halt, DOC tombstone and common ownership claim remains mandatory', t => {
  const f = fixture(t), { audit } = retire(f), report = JSON.parse(fs.readFileSync(audit.path));
  for (const item of [...report.barriers, report.claim]) {
    const old = fs.readFileSync(item.path); fs.writeFileSync(item.path, '{}'); assert.throws(() => f.api.validate(audit)); fs.writeFileSync(item.path, old);
  }
  assert.equal(f.api.validate(audit).transferred_slots, 90);
  const second = f.groups[1]; f.put(path.join(second.plan.control_directory, 'claims', 'unrecorded.json'), {});
  assert.throws(() => f.api.validate(audit), /claims or execution/);
});

test('claimed, used, changed or insufficient old assignments reject before publication', t => {
  for (const change of [
    f => f.put(path.join(f.groups[0].plan.control_directory, 'claims', 'claimed.json'), {}),
    f => f.put(path.join(f.groups[0].plan.directory, f.groups[0].plan.runs[0].id, 'data', 'used.json'), {}),
    f => f.put(path.join(f.groups[0].plan.directory, f.groups[0].plan.runs[0].id, 'prompt.txt'), 'changed'),
    f => f.put(path.join(f.groups[0].plan.directory, f.groups[0].plan.runs[0].id, 'profile.json'), { changed: true }),
    f => f.put(path.join(f.groups[0].plan.directory, f.groups[0].plan.runs[0].id, 'workspace/input.txt'), 'changed'),
    f => { f.proof.groups[0].plan.runs.pop(); },
    f => f.put(f.document.claim_path, { consumed: true }),
    f => { f.proof.document.runs[0].cap_micros++; },
    f => { f.proof.document.runs.pop(); },
    f => { f.proof.document.runs[1].id = f.proof.document.runs[0].id; },
    f => { f.state.rejectPrerequisites = true; }
  ]) {
    const f = fixture(t); assert.equal(f.api.observe(f.input).slots.length, 72); change(f); assert.throws(() => f.api.prepare(f.inputFile, f.destination));
    assert(!fs.existsSync(f.destination)); assert(!fs.existsSync(f.api.claimFile())); assert.equal(f.state.childCalls, 0);
  }
});

test('protected outputs and producer drift cannot create retirement claims', t => {
  const f = fixture(t);
  for (const directory of [f.source, f.oldRoot, f.archive, f.oldSkl, f.reader, f.proof.manifest.directory])
    assert.throws(() => f.api.prepare(f.inputFile, path.join(directory, 'new-retirement')), /overlaps/);
  const prepared = f.api.prepare(f.inputFile, f.destination);
  fs.appendFileSync(f.childFile, '\n// synthetic changed producer\n');
  assert.throws(() => f.api.retire(prepared.path, prepared.sha256), /preparation/);
  assert(!fs.existsSync(f.api.claimFile())); assert.equal(f.state.childCalls, 0);
});

test('between-stage drift fails before claiming and failed denial keeps one-shot barriers without a success audit', t => {
  let f = fixture(t), prepared = f.api.prepare(f.inputFile, f.destination);
  f.put(path.join(f.groups[0].plan.directory, f.groups[0].plan.runs[0].id, 'data', 'late.json'), {});
  assert.throws(() => f.api.retire(prepared.path, prepared.sha256), /pristine/); assert(!fs.existsSync(f.api.claimFile()));
  f = fixture(t); prepared = f.api.prepare(f.inputFile, f.destination); f.state.failChild = true;
  assert.throws(() => f.api.retire(prepared.path, prepared.sha256), /Synthetic child failure/);
  assert(fs.existsSync(f.api.claimFile())); assert(fs.existsSync(f.document.claim_path));
  assert(f.groups.every(group => fs.existsSync(path.join(group.plan.control_directory, 'halt.json'))));
  assert(!fs.existsSync(path.join(f.destination, 'audit.json')));
  assert.throws(() => f.api.retire(prepared.path, prepared.sha256), /unconsumed/); assert.equal(f.state.childCalls, 1);
});

test('shared transition, owner, consumed SKL and reader drift reject retirement before the common claim', t => {
  for (const change of [
    f => f.put(path.join(f.proof.manifest.directory, 'transitions/1.json'), { synthetic: 'new transition' }),
    f => f.put(path.join(f.proof.manifest.directory, 'active-skill.json'), { synthetic: 'changed owner' }),
    f => fs.unlinkSync(path.join(f.proof.manifest.directory, 'active-skill.json')),
    f => f.put(path.join(f.oldSkl, 'preserved.txt'), 'Changed consumed raw SKL evidence\n'),
    f => f.put(path.join(f.reader, 'preserved.txt'), 'Changed retained reader evidence\n')
  ]) {
    const f = fixture(t), prepared = f.api.prepare(f.inputFile, f.destination);
    change(f);
    assert.throws(() => f.api.retire(prepared.path, prepared.sha256), /Unused reservations changed before retirement/);
    assert(!fs.existsSync(f.api.claimFile())); assert(!fs.existsSync(f.document.claim_path));
    assert(f.groups.every(group => !fs.existsSync(path.join(group.plan.control_directory, 'halt.json'))));
    assert(!fs.existsSync(path.join(f.destination, 'audit.json'))); assert.equal(f.state.childCalls, 0);
  }
});

test('actual denial child rejects barrier removal over synthetic archived modules without native transport', t => {
  const f = fixture(t); retire(f);
  f.put(path.join(f.oldRoot, 'scripts/evals/cs3-comparison.cjs'), `const fs=require('node:fs'),path=require('node:path');exports.run=async(file,hash,skill,call)=>{const p=JSON.parse(fs.readFileSync(file));if(fs.existsSync(path.join(p.control_directory,'halt.json')))throw Error('Unknown skill or terminal halted envelope');call();};`);
  f.put(path.join(f.oldRoot, 'scripts/evals/cs3-document-remediation.cjs'), `const fs=require('node:fs');exports.claimFile=()=>${JSON.stringify(f.document.claim_path)};exports.prepare=()=>{if(fs.existsSync(exports.claimFile()))throw Error('New private one-shot DOC campaign required');throw Error('Unexpected preparation admission');};`);
  const destination = path.join(f.privateRoot, 'never-created-doc'), args = [f.childFile, f.oldRoot, f.input.manifest.path, destination,
    ...f.groups.flatMap(group => [group.plan_ref.path, group.plan_ref.sha256, group.skill])];
  const env = { ...process.env }; for (const key of Object.keys(env)) if (['OPENROUTER_API_KEY', 'NODE_OPTIONS', 'NODE_PATH'].includes(key.toUpperCase())) delete env[key];
  const run = () => execFileSync(process.execPath, args, { env, encoding: 'utf8', windowsHide: true, timeout: 10000, stdio: ['ignore', 'pipe', 'pipe'] });
  assert.equal(JSON.parse(run()).transport_calls, 0); assert(!fs.existsSync(destination));
  const halt = path.join(f.groups[0].plan.control_directory, 'halt.json'), bytes = fs.readFileSync(halt);
  fs.unlinkSync(halt); assert.throws(run); assert(!fs.existsSync(destination));
  fs.writeFileSync(halt, bytes); fs.unlinkSync(f.document.claim_path);
  assert.throws(run); assert(!fs.existsSync(destination));
});
