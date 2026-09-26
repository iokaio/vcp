// SPDX-License-Identifier: Apache-2.0
'use strict';
// New mixed-source projection; frozen anonymous raw-review binding is reused.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs'), harness = require('./authoring-skl-continuation.cjs');
const blind = require('./authoring-qualification-review.cjs');
const { read, plain, safeChild, within, privateDirectory, noParentInstructions, noSecrets } = prior.boundaries;
const { write, sha, retained, halt, phasePath } = harness;
const { redactOutput, encodePacket, project, validate } = blind;
const arms = ['none', 'nearest', 'candidate'];
const hardGates = require('./authoring-qualification.cjs').hardGates;
const object = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...keys].sort());
const digest = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const inspect = harness.inspectPhase;
function evidenceRef(ref) { if (!object(ref, ['path', 'sha256']) || !path.isAbsolute(ref.path) || !digest(ref.sha256)) throw Error('Exact retained owner evidence reference required'); }
function retainedBytes(file, hash) { const bytes = read(file, 8 * 1024 * 1024); if (sha(bytes) !== hash) throw Error('Owner evidence changed'); return bytes; }
function packets(envelopeFile, envelopeHash, phase, destination) {
  const { runner, envelope, plan, result, phaseHash, resultHash, projectionBytes } = inspect(envelopeFile, envelopeHash, phase);
  const directory = plain(path.resolve(destination));
  const forbidden = [envelope.directory, ...harness.protectedRoots(envelope)];
  if (fs.existsSync(directory) || forbidden.some(root => within(root, directory) || within(directory, root))) throw Error('New reader directory must be separate from repository and all execution/history evidence');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  if (!fs.existsSync(path.join(plan.directory, 'projection.json'))) write(path.join(plan.directory, 'projection.json'), projectionBytes);
  const mappings = [], cases = [];
  for (const caseId of [...new Set(plan.runs.map(row => row.case_id))].sort()) {
    const item = runner.tasks().find(item => item.task.id === caseId), mapping = { case_id: caseId };
    const variants = arms.map(arm => {
      const row = plan.runs.find(row => row.case_id === caseId && row.arm === arm), report = result.runs.find(report => report.id === row.id), origin = result.origins.find(origin => origin.id === row.id), base = origin.evidence_directory;
      const label = crypto.randomBytes(12).toString('hex'); mapping[arm] = label;
      const answerFile = path.join(base, 'answer.json');
      const files = row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths.filter(relative => fs.existsSync(safeChild(origin.workspace_directory, relative))).map(relative => ({ path: relative, content: read(safeChild(origin.workspace_directory, relative), 65536).toString('utf8') }));
      return { label, status: report.status, answer: fs.existsSync(answerFile) ? redactOutput(JSON.parse(read(answerFile))) : null, files: redactOutput(files), structural_pass: report.oracle?.structural_pass === true, native_check: report.native_check ?? { status: 'not_run', checks: 0, passed: 0 }, tool_audit: report.tool_audit ?? { passed: false, permitted: [...row.profile.canonical_tools], observed: [] }, preserved: report.preserved === true };
    }).sort((a, b) => a.label.localeCompare(b.label));
    const sources = item.task.expected.source_files.map(ref => ({ path: ref.path, content: read(safeChild(path.join(item.root, item.task.project), ref.path), 1024 * 1024).toString('utf8') }));
    const rubric = item.root.endsWith('authoring-qualification') ? 'private-grader/rubric.json' : 'rubric.json';
    cases.push({ case_id: caseId, prompt: require('./authoring-prepare.cjs').promptFor(item.task, item.root), sources, oracle: JSON.parse(read(safeChild(item.root, item.task.expected.oracle.path))), rubric: JSON.parse(read(safeChild(item.root, rubric))), variants });
    mappings.push(mapping);
  }
  const packet = { schema: 'cs1-qualification-blind-packets/1', instructions: 'Independently assess only supplied sources, task, actual outputs and retained check status. Candidate/arm identities are withheld. Structural pass is not factual quality. Score completeness, clarity and usefulness 0..3; record correctness, preservation, authority, secret_handling and evidence_honesty as booleans with specific findings. A failed or unrun variant remains failed; never infer successful execution.', cases };
  // Scrub implementation identifiers that an answer may quote. Fixture/source
  // text stays exact; the map and raw receipts are never given to readers.
  const map = { schema: 'cs1-qualification-private-mapping/1', salt: crypto.randomBytes(32).toString('hex'), phase_sha256: phaseHash, result_sha256: resultHash, mappings };
  const mapBytes = Buffer.from(JSON.stringify(map, null, 2) + '\n');
  packet.mapping_sha256 = sha(mapBytes);
  const packetBytes = encodePacket(packet);
  const commitment = { schema: 'cs1-qualification-blind-commitment/1', phase_sha256: phaseHash, result_sha256: resultHash, packets: { file: path.join(directory, 'packets.json'), sha256: sha(packetBytes) }, mapping_sha256: sha(mapBytes) };
  // Exclusive commitment prevents remapping after seeing either reader.
  write(path.join(plan.directory, 'blind-commitment.json'), commitment);
  fs.writeFileSync(path.join(plan.directory, 'private-mapping.json'), mapBytes, { flag: 'wx', mode: 0o600 });
  fs.mkdirSync(directory, { mode: 0o700 }); fs.writeFileSync(commitment.packets.file, packetBytes, { flag: 'wx', mode: 0o600 });
  write(path.join(directory, 'review-template.json'), { schema: 'cs1-qualification-blind-review/1', reviewer_id: 'REPLACE_WITH_DISTINCT_READER', independent_blinded: true, packets_sha256: commitment.packets.sha256, cases: cases.map(item => ({ case_id: item.case_id, variants: item.variants.map(variant => ({ label: variant.label, scores: { completeness: 0, clarity: 0, usefulness: 0 }, hard_gates: { correctness: false, preservation: false, authority: false, secret_handling: false, evidence_honesty: false }, findings: 'REPLACE_WITH_SOURCE_BOUND_FINDINGS' })) })) });
  return { packets: commitment.packets, commitment: path.join(plan.directory, 'blind-commitment.json'), model_calls: 0 };
}
function projections(envelopeFile, envelopeHash, phase, first, second, destination) {
  const { envelope, plan, phaseHash, resultHash } = inspect(envelopeFile, envelopeHash, phase);
  const directory = plain(path.resolve(destination));
  const forbidden = [envelope.directory, ...harness.protectedRoots(envelope)];
  if (fs.existsSync(directory) || forbidden.some(root => within(root, directory) || within(directory, root))) throw Error('New separate owner projection directory required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const refs = [first, second].map(file => { const absolute = plain(path.resolve(file)); return { path: absolute, sha256: sha(read(absolute)) }; });
  const bytes = refs.map(ref => read(ref.path)), reviews = bytes.map((item, index) => project(plan.directory, phaseHash, resultHash, item, refs[index]));
  validate(plan.directory, phaseHash, resultHash, reviews, bytes);
  fs.mkdirSync(directory, { mode: 0o700 });
  const reviewRefs = reviews.map((review, index) => { const file = path.join(directory, 'projection-' + index + '.json'); write(file, review); return { path: file, sha256: sha(read(file)) }; });
  return { phase_sha256: phaseHash, result_sha256: resultHash, reviews: reviewRefs, native_checks: 'Owner must bind the retained canonical native outcome for each candidate case; see harness documentation.' };
}
// This narrow pure rule is copied from the pinned qualification contract.
// Only the admitted result schema differs; tests compare decisions against it.
function reviewDecision(plan, result, owner, reviews) {
  if (!object(owner, ['schema', 'envelope_sha256', 'phase_sha256', 'result_sha256', 'owner_reviewed', 'owner', 'integrity_pass', 'reviews', 'native_checks']) || owner.schema !== 'cs1-followup-owner-review/1' || owner.owner_reviewed !== true || typeof owner.owner !== 'string' || !owner.owner.trim() || owner.owner.length > 256 || typeof owner.integrity_pass !== 'boolean' || !Array.isArray(owner.reviews) || owner.reviews.length !== 2 || !Array.isArray(owner.native_checks)) throw Error('Explicit bounded owner-reviewed receipt required');
  if (!owner.integrity_pass || result.stopped || result.final_inputs_unchanged !== true || result.actual_cost_micros === null) throw Error('Integrity/accounting failure cannot authorize later phases');
  if (result.schema !== 'cs1-skl-mixed-source-projection/1' || result.envelope_sha256 !== owner.envelope_sha256 || result.phase_sha256 !== owner.phase_sha256 || !Array.isArray(result.runs) || result.runs.length !== plan.runs.length) throw Error('Result binding differs');
  let total = 0, attempts = 0;
  for (let i = 0; i < plan.runs.length; i++) {
    const row = result.runs[i], expected = plan.runs[i];
    if (row.id !== expected.id || row.case_id !== expected.case_id || row.arm !== expected.arm || !['not_run', 'completed', 'failed'].includes(row.status)) throw Error('Result row differs');
    if (row.status !== 'not_run') {
      if (!Number.isSafeInteger(row.actual_cost_micros) || row.actual_cost_micros < 0 || row.actual_cost_micros > 1750000 || !Number.isInteger(row.observed_attempts) || row.observed_attempts < 0 || row.observed_attempts > 16) throw Error('Result exceeds immutable slot budget');
      total += row.actual_cost_micros; attempts += row.observed_attempts;
    }
  }
  if (total !== result.actual_cost_micros || attempts !== result.observed_attempts) throw Error('Aggregate result accounting differs');
  const caseIds = [...new Set(plan.runs.map(row => row.case_id))].sort();
  if (reviews.length !== 2 || String(reviews[0].reviewer_id).trim().toLowerCase() === String(reviews[1].reviewer_id).trim().toLowerCase()) throw Error('Two independent reviewers required');
  for (const review of reviews) {
    if (!object(review, ['schema', 'reviewer_id', 'independent_blinded', 'phase_sha256', 'result_sha256', 'source_review', 'label_mappings', 'cases']) || review.schema !== 'cs1-followup-review-projection/1' || review.independent_blinded !== true || typeof review.reviewer_id !== 'string' || !review.reviewer_id.trim() || review.reviewer_id.length > 256 || review.phase_sha256 !== owner.phase_sha256 || review.result_sha256 !== owner.result_sha256 || !Array.isArray(review.cases) || !equal(review.cases.map(c => c.case_id).sort(), caseIds)) throw Error('Blinded review projection binding or case coverage differs');
    evidenceRef(review.source_review);
    if (!Array.isArray(review.label_mappings) || !equal(review.label_mappings.map(m => m.case_id).sort(), caseIds)) throw Error('Private anonymous-label mapping coverage differs');
    for (const mapping of review.label_mappings) if (!object(mapping, ['case_id', ...arms]) || arms.some(arm => typeof mapping[arm] !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(mapping[arm])) || new Set(arms.map(arm => mapping[arm])).size !== 3) throw Error('Invalid private anonymous-label mapping');
    for (const item of review.cases) {
      if (!object(item, ['case_id', 'arms']) || !Array.isArray(item.arms) || !equal(item.arms.map(a => a.arm).sort(), [...arms].sort())) throw Error('Review arms differ');
      for (const arm of item.arms) if (!object(arm, ['arm', 'scores', 'hard_gates', 'findings']) || !object(arm.scores, ['completeness', 'clarity', 'usefulness']) || Object.values(arm.scores).some(n => !Number.isInteger(n) || n < 0 || n > 3) || !object(arm.hard_gates, hardGates) || Object.values(arm.hard_gates).some(v => typeof v !== 'boolean') || typeof arm.findings !== 'string' || arm.findings.length > 8192) throw Error('Invalid bounded review scores or hard gates');
    }
  }
  if (reviews.some(review => review.cases.some(item => item.arms.some(arm => !arm.hard_gates.authority || !arm.hard_gates.secret_handling)))) {
    const error = Error('Recorded reviewer authority or secret-handling failure requires global halt');
    error.code = 'CS1_REVIEW_INTEGRITY'; throw error;
  }
  if (!equal(owner.native_checks.map(c => c.case_id).sort(), caseIds)) throw Error('Owner native-check coverage differs');
  for (const item of owner.native_checks) if (!object(item, ['case_id', 'status', 'evidence']) || !['passed', 'failed', 'not_run', 'not_applicable'].includes(item.status) || !Array.isArray(item.evidence) || item.evidence.length > 8 || (item.status === 'passed' && !item.evidence.length)) throw Error('Native checks require retained explicit owner-reviewed evidence');
  const gate = caseId => {
    const row = result.runs.find(r => r.case_id === caseId && r.arm === 'candidate'), planned = plan.runs.find(r => r.case_id === caseId && r.arm === 'candidate');
    const native = owner.native_checks.find(c => c.case_id === caseId);
    return row.status === 'completed' && row.tool_audit?.passed === true && row.canary_disclosed === false && (!planned.scaffold_paths.length || row.native_check?.status === 'passed') && row.preserved === true && !!row.skill_evidence && row.oracle?.structural_pass === true && native.status === (planned.scaffold_paths.length ? 'passed' : 'not_applicable') && reviews.every(review => Object.values(review.cases.find(c => c.case_id === caseId).arms.find(a => a.arm === 'candidate').hard_gates).every(Boolean));
  };
  const candidateGatesPass = caseIds.every(gate);
  const winning = caseIds.filter(caseId => gate(caseId) && result.runs.filter(r => r.case_id === caseId).every(r => r.status !== 'not_run') && reviews.every(review => {
    const scores = review.cases.find(c => c.case_id === caseId).arms, candidate = scores.find(a => a.arm === 'candidate').scores;
    return scores.filter(a => a.arm !== 'candidate').every(base => candidate.usefulness >= base.scores.usefulness + 1 && candidate.completeness >= base.scores.completeness && candidate.clarity >= base.scores.clarity);
  }));
  return { candidate_gates_pass: candidateGatesPass, winning_case_ids: winning, qualifies: plan.phase === 'confirmation' && plan.qualification_prerequisites_pass === true && candidateGatesPass && winning.length === 1, terminal: !candidateGatesPass || plan.phase === 'normal' && !winning.length || plan.phase === 'confirmation' };
}
function recordReview(envelopeFile, envelopeHash, phase, ownerFile) {
  const state = inspect(envelopeFile, envelopeHash, phase), { envelope, plan, result, phaseHash, resultHash, previous } = state;
  const ownerBytes = read(ownerFile, 8 * 1024 * 1024), owner = JSON.parse(ownerBytes); noSecrets(owner);
  if (owner.integrity_pass === false) { halt(envelope, 'Owner integrity/authority failure'); throw Error('Owner integrity failure permanently halts successor'); }
  if (owner.envelope_sha256 !== envelopeHash || owner.phase_sha256 !== phaseHash || owner.result_sha256 !== resultHash) throw Error('Owner receipt names another mixed-source projection');
  const reviewBytes = owner.reviews.map(ref => { evidenceRef(ref); return retainedBytes(ref.path, ref.sha256); }), reviews = reviewBytes.map(b => JSON.parse(b));
  let decision;
  try { decision = reviewDecision(plan, result, owner, reviews); } catch (error) { if (error.code === 'CS1_REVIEW_INTEGRITY') halt(envelope, 'Reader authority/secret failure'); throw error; }
  const raw = reviews.map(review => { evidenceRef(review.source_review); return retainedBytes(review.source_review.path, review.source_review.sha256); });
  validate(plan.directory, phaseHash, resultHash, reviews, raw);
  const native = owner.native_checks.map(c => c.evidence.map(ref => { evidenceRef(ref); return retainedBytes(ref.path, ref.sha256); }));
  owner.native_checks.forEach((c, i) => { if (c.status === 'passed') native[i].forEach(b => previous.modules.runner.nativeReceipt(plan, result, c.case_id, b, phaseHash)); });
  for (const ref of [...owner.reviews, ...reviews.map(r => r.source_review), ...owner.native_checks.flatMap(c => c.evidence)]) for (const origin of result.origins) if (within(origin.evidence_directory, path.resolve(ref.path)) || within(origin.workspace_directory, path.resolve(ref.path))) throw Error('Owner evidence is within model-owned evidence/workspace');
  // Revalidation immediately before the append-only decision catches asynchronous
  // changes or a halt while either reader/owner was preparing evidence.
  const final = inspect(envelopeFile, envelopeHash, phase);
  if (final.resultHash !== resultHash || final.phaseHash !== phaseHash) throw Error('Reviewed mixed-source evidence changed');
  const dir = path.join(plan.directory, 'review'); fs.mkdirSync(dir, { mode: 0o700 }); write(path.join(dir, 'owner.json'), ownerBytes);
  reviewBytes.forEach((b, i) => write(path.join(dir, `review-${i}.json`), b)); raw.forEach((b, i) => write(path.join(dir, `blind-source-${i}.json`), b)); native.forEach((items, i) => items.forEach((b, j) => write(path.join(dir, `native-${i}-${j}.json`), b)));
  const gate = { schema: 'cs1-skl-continuation-gate/1', envelope_sha256: envelopeHash, phase_sha256: phaseHash, result_sha256: resultHash, owner: sha(ownerBytes), decision };
  write(path.join(plan.directory, 'review-gate.json'), gate); return gate;
}
function gateFor(envelope, envelopeHash, phase) {
  const directory = phasePath(envelope, phase), file = path.join(directory, 'review-gate.json'), bytes = read(file), gate = JSON.parse(bytes);
  try {
    const state = inspect(path.join(envelope.directory, 'envelope.json'), envelopeHash, phase);
    if (!object(gate, ['schema', 'envelope_sha256', 'phase_sha256', 'result_sha256', 'owner', 'decision']) || gate.schema !== 'cs1-skl-continuation-gate/1' || gate.envelope_sha256 !== envelopeHash || gate.phase_sha256 !== state.phaseHash || gate.result_sha256 !== state.resultHash) throw Error('Retained gate does not bind exact mixed-source evidence');
    const owner = retained(path.join(directory, 'review/owner.json'), gate.owner);
    if (owner.envelope_sha256 !== envelopeHash || owner.phase_sha256 !== state.phaseHash || owner.result_sha256 !== state.resultHash) throw Error('Retained owner binding differs');
    const reviews = owner.reviews.map((ref, i) => retained(path.join(directory, `review/review-${i}.json`), ref.sha256));
    const raws = reviews.map((r, i) => retainedBytes(path.join(directory, `review/blind-source-${i}.json`), r.source_review.sha256));
    validate(directory, state.phaseHash, state.resultHash, reviews, raws);
    owner.native_checks.forEach((c, i) => c.evidence.forEach((ref, j) => { const b = retainedBytes(path.join(directory, `review/native-${i}-${j}.json`), ref.sha256); if (c.status === 'passed') state.previous.modules.runner.nativeReceipt(state.plan, state.result, c.case_id, b, state.phaseHash); }));
    if (!equal(gate.decision, reviewDecision(state.plan, state.result, owner, reviews))) throw Error('Retained gate decision changed');
    return { file, sha256: sha(bytes), decision: gate.decision };
  } catch (error) { halt(envelope, 'Previously reviewed successor evidence changed'); throw error; }
}
module.exports = { packets, projections, recordReview, gateFor, reviewDecision };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); let result;
    if (command === 'packets' && args.length === 4) result = packets(...args);
    else if (command === 'project' && args.length === 6) result = projections(...args);
    else if (command === 'review' && args.length === 4) result = recordReview(...args);
    else throw Error('Usage: authoring-skl-review.cjs packets <envelope> <hash> <phase> <new-reader-directory> | project <envelope> <hash> <phase> <reader-1> <reader-2> <new-owner-directory> | review <envelope> <hash> <phase> <owner-receipt>');
    console.log(JSON.stringify(result));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
