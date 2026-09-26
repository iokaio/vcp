// SPDX-License-Identifier: Apache-2.0
'use strict';
// Anonymous packets are committed before either independent reader responds.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs');
const { read, write, plain, safeChild, within, privateDirectory, noParentInstructions } = prior.boundaries;
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const arms = ['none', 'nearest', 'candidate'];
const encodePacket = packet => Buffer.from(JSON.stringify(packet, null, 2) + '\n');
const reviewerKey = value => typeof value === 'string' ? value.trim().toLowerCase() : '';
const redactOutput = value => JSON.parse(JSON.stringify(value).replaceAll(/vcp-authoring-(?:requalification-)?candidates::\.::(?:document-authoring|skill-authoring)|vcp-builtin::(?:architecture::architecture|testing::testing)/g, '[selected skill]'));
const object = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...keys].sort());
function protectedRoots(envelope) {
  const successor = envelope.budget.reference?.successor;
  return [path.resolve(__dirname, '../..'), envelope.directory, envelope.budget.current.reference.repository, successor?.repository,
    path.dirname(envelope.budget.current.reference.plan.file), ...envelope.budget.history.phases.map(item => path.dirname(item.reference.envelope.file)),
    ...['envelope', 'plan', 'result', 'halt'].map(name => successor?.[name]?.file).filter(Boolean).map(file => path.dirname(file))].filter(Boolean);
}
function bound(file, hash) { const bytes = read(file); if (sha(bytes) !== hash) throw Error('Bound review evidence changed'); return JSON.parse(bytes); }
function inspect(envelopeFile, envelopeHash, candidate, phase) {
  const runner = require('./authoring-requalification-runner.cjs'), envelope = runner.envelopeFor(envelopeFile, envelopeHash);
  if (!['document-authoring', 'skill-authoring'].includes(candidate) || !['normal', 'inherited', 'confirmation'].includes(phase)) throw Error('Unknown fixed phase');
  const directory = path.join(envelope.directory, 'phases', candidate + '--' + phase), file = path.join(directory, 'plan.json'), bytes = read(file), plan = JSON.parse(bytes);
  runner.validatePhase(envelopeFile, envelopeHash, file, sha(bytes), plan.runs.length);
  const resultBytes = read(path.join(directory, 'result.json')), result = JSON.parse(resultBytes);
  runner.resultEvidence(envelope, plan, result, sha(bytes));
  if (result.stopped || result.final_inputs_unchanged !== true) throw Error('Interrupted integrity/accounting result cannot be reviewed for advancement');
  return { runner, envelope, plan, result, phaseHash: sha(bytes), resultHash: sha(resultBytes) };
}
function packets(envelopeFile, envelopeHash, candidate, phase, destination) {
  const { runner, envelope, plan, result, phaseHash, resultHash } = inspect(envelopeFile, envelopeHash, candidate, phase);
  const directory = plain(path.resolve(destination));
  const forbidden = protectedRoots(envelope);
  if (fs.existsSync(directory) || forbidden.some(root => within(root, directory) || within(directory, root))) throw Error('New reader directory must be separate from repository and all execution/history evidence');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const mappings = [], cases = [];
  for (const caseId of [...new Set(plan.runs.map(row => row.case_id))].sort()) {
    const item = runner.tasks().find(item => item.task.id === caseId), mapping = { case_id: caseId };
    const variants = arms.map(arm => {
      const row = plan.runs.find(row => row.case_id === caseId && row.arm === arm), report = result.runs.find(report => report.id === row.id), base = path.join(plan.directory, row.id);
      const label = crypto.randomBytes(12).toString('hex'); mapping[arm] = label;
      const answerFile = path.join(base, 'answer.json');
      const files = row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths.filter(relative => fs.existsSync(safeChild(path.join(base, 'workspace'), relative))).map(relative => ({ path: relative, content: read(safeChild(path.join(base, 'workspace'), relative), 65536).toString('utf8') }));
      return { label, status: report.status, answer: fs.existsSync(answerFile) ? redactOutput(JSON.parse(read(answerFile))) : null, files: redactOutput(files), structural_pass: report.oracle?.structural_pass === true, native_check: report.native_check ?? { status: 'not_run', checks: 0, passed: 0 }, tool_audit: report.tool_audit ?? { passed: false, permitted: [...row.profile.canonical_tools], observed: [] }, preserved: report.preserved === true };
    }).sort((a, b) => a.label.localeCompare(b.label));
    const sources = item.task.expected.source_files.map(ref => ({ path: ref.path, content: read(safeChild(path.join(item.root, item.task.project), ref.path), 1024 * 1024).toString('utf8') }));
    const rubric = 'rubric.json';
    cases.push({ case_id: caseId, prompt: require('./authoring-prepare.cjs').promptFor(item.task, item.root), sources, oracle: JSON.parse(read(safeChild(item.root, item.task.expected.oracle.path))), rubric: JSON.parse(read(safeChild(item.root, rubric))), variants });
    mappings.push(mapping);
  }
  const packet = { schema: 'cs1-requalification-blind-packets/1', instructions: 'Independently assess only supplied sources, task, actual outputs and retained check status. Candidate/arm identities are withheld. Structural pass is not factual quality. Score completeness, clarity and usefulness 0..3; record correctness, preservation, authority, secret_handling and evidence_honesty as booleans with specific findings. A failed or unrun variant remains failed; never infer successful execution.', cases };
  // Scrub implementation identifiers that an answer may quote. Fixture/source
  // text stays exact; the map and raw receipts are never given to readers.
  const map = { schema: 'cs1-requalification-private-mapping/1', salt: crypto.randomBytes(32).toString('hex'), phase_sha256: phaseHash, result_sha256: resultHash, mappings };
  const mapBytes = Buffer.from(JSON.stringify(map, null, 2) + '\n');
  packet.mapping_sha256 = sha(mapBytes);
  const packetBytes = encodePacket(packet);
  const commitment = { schema: 'cs1-requalification-blind-commitment/1', phase_sha256: phaseHash, result_sha256: resultHash, packets: { file: path.join(directory, 'packets.json'), sha256: sha(packetBytes) }, mapping_sha256: sha(mapBytes) };
  // Exclusive commitment prevents remapping after seeing either reader.
  write(path.join(plan.directory, 'blind-commitment.json'), commitment);
  fs.writeFileSync(path.join(plan.directory, 'private-mapping.json'), mapBytes, { flag: 'wx', mode: 0o600 });
  fs.mkdirSync(directory, { mode: 0o700 }); fs.writeFileSync(commitment.packets.file, packetBytes, { flag: 'wx', mode: 0o600 });
  write(path.join(directory, 'review-template.json'), { schema: 'cs1-requalification-blind-review/1', reviewer_id: 'REPLACE_WITH_DISTINCT_READER', independent_blinded: true, packets_sha256: commitment.packets.sha256, cases: cases.map(item => ({ case_id: item.case_id, variants: item.variants.map(variant => ({ label: variant.label, scores: { completeness: 0, clarity: 0, usefulness: 0 }, hard_gates: { correctness: false, preservation: false, authority: false, secret_handling: false, evidence_honesty: false }, findings: 'REPLACE_WITH_SOURCE_BOUND_FINDINGS' })) })) });
  return { packets: commitment.packets, commitment: path.join(plan.directory, 'blind-commitment.json'), model_calls: 0 };
}
function project(directory, phaseHash, resultHash, rawBytes, sourceRef) {
  const commit = JSON.parse(read(path.join(directory, 'blind-commitment.json')));
  if (!object(commit, ['schema', 'phase_sha256', 'result_sha256', 'packets', 'mapping_sha256']) || commit.schema !== 'cs1-requalification-blind-commitment/1' || commit.phase_sha256 !== phaseHash || commit.result_sha256 !== resultHash) throw Error('Blind commitment names another phase/result');
  const packet = bound(commit.packets.file, commit.packets.sha256), map = bound(path.join(directory, 'private-mapping.json'), commit.mapping_sha256), raw = JSON.parse(rawBytes);
  if (packet.mapping_sha256 !== commit.mapping_sha256) throw Error('Reader packet does not bind the private mapping commitment');
  if (!object(map, ['schema', 'salt', 'phase_sha256', 'result_sha256', 'mappings']) || !/^[a-f0-9]{64}$/.test(map.salt) || map.schema !== 'cs1-requalification-private-mapping/1' || map.phase_sha256 !== phaseHash || map.result_sha256 !== resultHash || !Array.isArray(map.mappings) || !Array.isArray(packet.cases) || !object(raw, ['schema', 'reviewer_id', 'independent_blinded', 'packets_sha256', 'cases']) || raw.schema !== 'cs1-requalification-blind-review/1' || raw.independent_blinded !== true || raw.packets_sha256 !== commit.packets.sha256 || typeof raw.reviewer_id !== 'string' || !raw.reviewer_id.trim() || !Array.isArray(raw.cases) || !equal(raw.cases.map(item => item.case_id).sort(), map.mappings.map(item => item.case_id).sort()) || !equal(packet.cases.map(item => item.case_id).sort(), map.mappings.map(item => item.case_id).sort())) throw Error('Independent raw reader coverage or binding differs');
  const cases = map.mappings.map(mapping => {
    if (!object(mapping, ['case_id', ...arms]) || new Set(arms.map(arm => mapping[arm])).size !== 3) throw Error('Private mapping differs');
    const item = raw.cases.find(item => item.case_id === mapping.case_id), source = packet.cases.find(item => item.case_id === mapping.case_id), labels = arms.map(arm => mapping[arm]).sort();
    if (!object(item, ['case_id', 'variants']) || !Array.isArray(item.variants) || !equal(item.variants.map(variant => variant.label).sort(), labels) || !equal(source.variants.map(variant => variant.label).sort(), labels)) throw Error('Anonymous label coverage differs');
    return { case_id: item.case_id, arms: arms.map(arm => {
      const variant = item.variants.find(variant => variant.label === mapping[arm]);
      if (!object(variant, ['label', 'scores', 'hard_gates', 'findings'])) throw Error('Raw reader variant schema differs');
      return { arm, scores: variant.scores, hard_gates: variant.hard_gates, findings: variant.findings };
    }) };
  });
  return { schema: 'cs1-followup-review-projection/1', reviewer_id: raw.reviewer_id, independent_blinded: true, phase_sha256: phaseHash, result_sha256: resultHash, source_review: sourceRef, label_mappings: map.mappings, cases };
}
function validate(directory, phaseHash, resultHash, reviews, rawBytes) {
  if (reviews.length !== 2 || rawBytes.length !== 2 || reviewerKey(reviews[0].reviewer_id) === reviewerKey(reviews[1].reviewer_id)) throw Error('Two distinct independent readers required');
  reviews.forEach((review, index) => { if (!equal(review, project(directory, phaseHash, resultHash, rawBytes[index], review.source_review))) throw Error('Projection altered the original anonymous review'); });
}
function projections(envelopeFile, envelopeHash, candidate, phase, first, second, destination) {
  const { envelope, plan, phaseHash, resultHash } = inspect(envelopeFile, envelopeHash, candidate, phase);
  const directory = plain(path.resolve(destination));
  const forbidden = protectedRoots(envelope);
  if (fs.existsSync(directory) || forbidden.some(root => within(root, directory) || within(directory, root))) throw Error('New separate owner projection directory required');
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  const refs = [first, second].map(file => { const absolute = plain(path.resolve(file)); return { path: absolute, sha256: sha(read(absolute)) }; });
  const bytes = refs.map(ref => read(ref.path)), reviews = bytes.map((item, index) => project(plan.directory, phaseHash, resultHash, item, refs[index]));
  validate(plan.directory, phaseHash, resultHash, reviews, bytes);
  fs.mkdirSync(directory, { mode: 0o700 });
  const reviewRefs = reviews.map((review, index) => { const file = path.join(directory, 'projection-' + index + '.json'); write(file, review); return { path: file, sha256: sha(read(file)) }; });
  return { phase_sha256: phaseHash, result_sha256: resultHash, reviews: reviewRefs, native_checks: 'Owner must bind the retained canonical native outcome for each candidate case; see harness documentation.' };
}
module.exports = { packets, project, validate, projections, redactOutput, encodePacket, protectedRoots };
if (require.main === module) {
  try { const [command, ...args] = process.argv.slice(2); if (command === 'packets' && args.length === 5) console.log(JSON.stringify(packets(...args))); else if (command === 'project' && args.length === 7) console.log(JSON.stringify(projections(...args))); else throw Error('Usage: authoring-requalification-review.cjs packets <envelope> <hash> <candidate> <phase> <new-reader-dir> | project <envelope> <hash> <candidate> <phase> <reader-1-json> <reader-2-json> <new-owner-dir>'); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
