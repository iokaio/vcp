// SPDX-License-Identifier: Apache-2.0
'use strict';
// Retire only untouched DeepInfra reservations. No provider or native invocation.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { execFileSync } = require('node:child_process'), { isDeepStrictEqual: equal } = require('node:util');
const prep = require('./authoring-prepare.cjs'), core = require('./cs3-comparison.cjs'), recovery = require('./cs3-controller-recovery-qualification.cjs');
const { plain, read, write, within, privateDirectory, noParentInstructions, noSecrets } = require('./p6-live-runner.cjs').boundaries;
const root = path.resolve(__dirname, '../..'), decisionPath = path.join(root, 'src/evals/skills/cs3-friendli-transfer/retirement-decision.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function json(reference) {
  need(reference && typeof reference.path === 'string' && path.isAbsolute(reference.path)
    && /^[a-f0-9]{64}$/.test(reference.sha256), 'Absolute hash-bound retirement evidence required');
  const bytes = read(plain(reference.path), 64 * 1024 * 1024);
  need(sha(bytes) === reference.sha256, 'Bound retirement evidence changed');
  return JSON.parse(bytes);
}
const ref = file => ({ path: plain(path.resolve(file)), sha256: sha(read(file, 64 * 1024 * 1024)) });
const skills = ['frontend-design', 'mcp-development', 'llm-integration', 'webapp-testing'];
function need(value, reason) { if (!value) throw Error(reason); }
function claimFile() { return path.join(path.dirname(core.claimFile(true)), 'vcp-cs3-friendli-retirement1.json'); }
function decision(reference) {
  const value = json(reference);
  need(equal(value, JSON.parse(read(decisionPath))) && value.schema === 'cs3-friendli-retirement-decision/1'
    && value.authority === 'owner_explicit_cs3_completion_within_existing_100_usd'
    && value.manifest_sha256 === 'ca332d5833682bb6ca467632dab259a48a28679c7d5500fff695555ade5206a4'
    && value.historical_source_sha256 === '84d6b90185d7ea16f11eb548f843e78a97daa218bda0d4a55bc759b099727320'
    && equal(value.runtime_skills, skills) && value.runtime_slots === 72 && value.unprepared_document_slots === 18
    && value.transferred_cap_micros === 54000000 && value.transferred_request_ceiling === 1440
    && value.combined_cap_micros === 98913737 && value.combined_request_ceiling === 2631
    && value.consumed_liabilities_released === false && value.model_calls === 0 && value.qualification_waiver === false, 'Exact fixed unused-reservation decision required');
  return value;
}
function producer() {
  return { controller: { path: fs.realpathSync(process.execPath), sha256: sha(read(fs.realpathSync(process.execPath), 128 * 1024 * 1024)) },
    sources: [__filename, path.join(__dirname, 'cs3-friendli-retirement-denial.cjs'), decisionPath].map(ref) };
}
function observe(input, retired = false) {
  need(equal(Object.keys(input).sort(), ['decision', 'history', 'manifest', 'recovery_decision']), 'Exact retirement inputs required'); noSecrets(input);
  const approved = decision(input.decision);
  need(input.manifest.sha256 === approved.manifest_sha256, 'Exact preserved runtime manifest required');
  const history = json(input.history), proof = recovery.friendliRetirementPrerequisites({ history, manifest: input.manifest }, input.recovery_decision);
  need(proof.manifest.source?.content_sha256 === approved.historical_source_sha256
    || proof.manifest.base?.source?.content_sha256 === approved.historical_source_sha256, 'Retirement source differs');
  need(equal(proof.groups.map(row => row.skill), skills) && proof.groups.length === 4
    && proof.document.tasks.length === 6 && proof.document.runs.length === 18, 'Exact four runtime groups and unused DOC assignment required');
  const slots = [], controls = [];
  for (const group of proof.groups) {
    const plan = group.plan, control = plain(plan.control_directory);
    need(plan.runs.length === 18 && group.tasks.length === 6 && plan.runs.every(row => row.skill === group.skill)
      && equal(fs.readdirSync(control).sort(), ['claims', ...(retired ? ['halt.json'] : []), 'plan.json'].sort())
      && fs.readdirSync(path.join(control, 'claims')).length === 0, 'Transferred group already has claims or execution');
    need(equal(json(group.plan_ref), plan), 'Retired plan reference differs');
    controls.push({ directory: control, inventory: prep.identity(control, ['claims', 'plan.json']) });
    for (const row of plan.runs) {
      const task = group.tasks.find(item => item.id === row.case_id), base = path.join(plan.directory, row.id);
      need(/^[A-Za-z0-9_-]+$/.test(row.id) && row.cap_micros === 600000 && row.call_ceiling === 16 && task
        && equal(fs.readdirSync(base).sort(), ['data', 'profile.json', 'prompt.txt', 'workspace']) && fs.readdirSync(path.join(base, 'data')).length === 0
        && sha(read(path.join(base, 'profile.json'))) === row.profile_sha256 && sha(read(path.join(base, 'prompt.txt'))) === row.prompt_sha256
        && equal(core.workspaceFiles(base), [...row.files].sort((a, b) => a.path.localeCompare(b.path)))
        && sha(core.prompt(task)) === row.prompt_sha256, 'Only exact pristine runtime slots may transfer');
      slots.push({ id: row.id, directory: base, inventory: prep.identity(base, ['.']) });
    }
  }
  need(slots.length === 72 && new Set(slots.map(row => row.id)).size === 72, 'Exactly seventy-two unique pristine slots required');
  need(path.isAbsolute(proof.document.claim_path) && (retired || !fs.existsSync(proof.document.claim_path)), 'DOC campaign was already prepared or claimed');
  need(new Set(proof.document.runs.map(row => row.id)).size === 18 && proof.document.runs.every(row => row.skill === 'document-authoring'
    && row.cap_micros === 600000 && row.call_ceiling === 16 && !slots.some(slot => slot.id === row.id)), 'Exact separate unused DOC reservation required');
  return { ...proof, slots, controls };
}
function barriers(prepared) {
  const { proof, input } = prepared;
  return [...proof.groups.map(group => ({ path: path.join(group.plan.control_directory, 'halt.json'), value: {
    schema: 'cs3-friendli-unused-group-retirement/1', manifest: input.manifest, skill: group.skill,
    retired_ids: group.plan.runs.map(row => row.id), reason: 'untouched_reservations_transferred_to_fixed_friendli',
    action: 'Permanent old group dispatch denial; no replay or qualification waiver.' } })),
  { path: proof.document.claim_path, value: { schema: 'cs3-friendli-unused-document-retirement/1', allocation: proof.document.allocation,
    retired_ids: proof.document.runs.map(row => row.id), reason: 'unprepared_reservation_transferred_to_fixed_friendli',
    action: 'Permanent old DOC preparation denial; no replay or qualification waiver.' } }];
}
function destination(directory, proof, input) {
  const archive = json(json(input.history).archive);
  const protectedPaths = [root, proof.oldRoot, archive.archive, proof.manifest.directory, ...proof.historical.protected_inventories.map(row => row.directory)];
  need(protectedPaths.every(item => !within(plain(item), directory) && !within(directory, plain(item))), 'Retirement output overlaps preserved source/evidence');
  privateDirectory(directory); noParentInstructions(path.dirname(directory));
}
function prepare(inputFile, outputDirectory) {
  const input = JSON.parse(read(inputFile)), proof = observe(input), directory = plain(path.resolve(outputDirectory));
  need(!fs.existsSync(directory) && !fs.existsSync(claimFile()), 'Fresh unclaimed retirement output required'); destination(directory, proof, input);
  const prepared = { schema: 'cs3-friendli-retirement-prepared/1', directory, input, proof, producer: producer(), model_calls: 0 };
  const targets = barriers(prepared); need(targets.every(row => !fs.existsSync(row.path)), 'Retirement barrier already exists');
  fs.mkdirSync(directory, { mode: 0o700 }); write(path.join(directory, 'prepared.json'), prepared); return ref(path.join(directory, 'prepared.json'));
}
function ownership(preparedRef, prepared) {
  return { schema: 'cs3-friendli-retirement-claim/1', prepared: preparedRef, audit_path: path.join(prepared.directory, 'audit.json'),
    barriers: barriers(prepared).map(row => ({ path: row.path, sha256: sha(JSON.stringify(row.value, null, 2) + '\n') })),
    transferred_cap_micros: 54000000, transferred_request_ceiling: 1440, model_calls: 0 };
}
function denied(prepared) {
  const arguments_ = [path.join(__dirname, 'cs3-friendli-retirement-denial.cjs'), prepared.proof.oldRoot,
    prepared.input.manifest.path, path.join(prepared.directory, 'never-created-doc'), ...prepared.proof.groups.flatMap(group => [group.plan_ref.path, group.plan_ref.sha256, group.skill])];
  // The fixed child supplies a no-dispatch fake transport and has no credential.
  const env = { ...process.env };
  for (const key of Object.keys(env)) if (['OPENROUTER_API_KEY', 'NODE_OPTIONS', 'NODE_PATH'].includes(key.toUpperCase())) delete env[key];
  const output = execFileSync(process.execPath, arguments_, { cwd: root, encoding: 'utf8', windowsHide: true, timeout: 180000, maxBuffer: 1024 * 1024, env });
  const value = JSON.parse(output);
  need(equal(value, { schema: 'cs3-friendli-retirement-denial/1', runtime_groups: skills, document_preparation_denied: true, transport_calls: 0, model_calls: 0 })
    && !fs.existsSync(path.join(prepared.directory, 'never-created-doc')), 'Actual archived entrypoints did not deny retired dispatch');
  return value;
}
function retire(preparedFile, hash) {
  const preparedRef = { path: plain(path.resolve(preparedFile)), sha256: hash }, prepared = json(preparedRef);
  need(prepared.schema === 'cs3-friendli-retirement-prepared/1' && prepared.model_calls === 0 && equal(prepared.producer, producer())
    && preparedRef.path === path.join(prepared.directory, 'prepared.json') && !fs.existsSync(claimFile()), 'Exact unconsumed retirement preparation required');
  destination(prepared.directory, prepared.proof, prepared.input);
  need(equal(observe(prepared.input), prepared.proof), 'Unused reservations changed before retirement');
  const claim = ownership(preparedRef, prepared); write(claimFile(), claim);
  for (const barrier of barriers(prepared)) write(barrier.path, barrier.value);
  need(equal(observe(prepared.input, true), prepared.proof), 'Only additive retirement barriers may change');
  const denial = denied(prepared);
  need(equal(observe(prepared.input, true), prepared.proof) && equal(producer(), prepared.producer), 'Retirement denial mutated evidence or source');
  const audit = { schema: 'cs3-friendli-retirement/1', prepared: preparedRef, claim: ref(claimFile()), barriers: claim.barriers,
    denial, transferred_slots: 90, transferred_cap_micros: 54000000, transferred_request_ceiling: 1440,
    combined_cap_micros: 98913737, combined_request_ceiling: 2631, model_calls: 0, consumed_liabilities_released: false };
  write(claim.audit_path, audit); return ref(claim.audit_path);
}
function validate(reference) {
  const audit = json(reference), prepared = json(audit.prepared), claim = ownership(audit.prepared, prepared);
  need(audit.schema === 'cs3-friendli-retirement/1' && prepared.schema === 'cs3-friendli-retirement-prepared/1'
    && audit.prepared.path === path.join(prepared.directory, 'prepared.json') && reference.path === claim.audit_path
    && equal(prepared.producer, producer()) && plain(audit.claim.path) === plain(claimFile()) && equal(json(audit.claim), claim)
    && equal(audit.barriers, claim.barriers) && audit.transferred_slots === 90 && audit.transferred_cap_micros === 54000000
    && audit.transferred_request_ceiling === 1440 && audit.combined_cap_micros === 98913737 && audit.combined_request_ceiling === 2631
    && audit.model_calls === 0 && audit.consumed_liabilities_released === false
    && equal(audit.denial, { schema: 'cs3-friendli-retirement-denial/1', runtime_groups: skills, document_preparation_denied: true, transport_calls: 0, model_calls: 0 }), 'Exact ninety-slot retirement evidence required');
  destination(prepared.directory, prepared.proof, prepared.input);
  barriers(prepared).forEach((row, index) => need(equal(json(audit.barriers[index]), row.value), 'Permanent retirement barrier changed'));
  need(equal(observe(prepared.input, true), prepared.proof), 'Retired source or raw unused evidence changed');
  return { ...prepared.proof, barriers: audit.barriers, audit: reference, transferred_slots: 90, transferred_cap_micros: 54000000, transferred_request_ceiling: 1440 };
}
module.exports = { decision, prepare, retire, validate, observe, barriers, claimFile };
if (require.main === module) {
  try { const [command, first, second] = process.argv.slice(2); const value = command === 'prepare' ? prepare(first, second) : command === 'retire' ? retire(first, second) : (() => { throw Error('Usage: prepare INPUT NEW_PRIVATE_DIRECTORY | retire PREPARED SHA256'); })(); process.stdout.write(JSON.stringify(value, null, 2) + '\n'); }
  catch (error) { process.stderr.write(error.message + '\n'); process.exitCode = 1; }
}
