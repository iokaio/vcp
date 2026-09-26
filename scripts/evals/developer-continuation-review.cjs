// SPDX-License-Identifier: Apache-2.0
'use strict';
// Continuation CS-2 grading, blind reader packets and per-skill decisions. Functional
// grading runs only the pinned AppContainer adapter on retained final workspaces.
// Reader packets live in their own directory and carry no arm, skill, cost or
// grader-diagnostic identity. The label mapping is committed by a salted hash in
// the packet index before any review and applied only after both reviews exist.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs');
const cs1 = require('./authoring-runner.cjs');
const prep = require('./developer-prepare.cjs');
const runner = require('./developer-continuation.cjs');
const frozenReview = require('./developer-review.cjs');
const oracle = require('./developer-oracle.cjs');
const grader = require('./developer-grader.cjs');
const { plain, read, write, within, noParentInstructions, privateDirectory } = prior.boundaries;
const repository = path.resolve(__dirname, '../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const object = (value, names) => value && typeof value === 'object' && !Array.isArray(value) && equal(Object.keys(value).sort(), [...names].sort());
const { labels, scores, hardGates, haltItems, executable, redact, review } = frozenReview;
const regrades = 3;

function validateBlock(value) {
  const { plan, rows, result, result_sha256 } = value;
  if (!Array.isArray(rows) || rows.length !== 18 || new Set(rows.map(row => row.id)).size !== 18
    || !equal(result.runs.map(row => row.id), rows.map(row => row.id)) || !/^[a-f0-9]{64}$/.test(result_sha256)) throw Error('Composite block coverage differs');
  const cases = [...new Set(rows.map(row => row.case_id))];
  if (cases.length !== 6 || cases.some(id => !equal(rows.filter(row => row.case_id === id).map(row => row.arm).sort(), [...prep.arms].sort()))
    || rows.some(row => !path.isAbsolute(row.evidence_directory) || row.block !== result.block)) throw Error('Composite block case, arm or evidence coverage differs');
  requireActive(plan);
  return value;
}
// Incomplete retained runs remain failures regardless of their preserved files.
// Only a harness fault on a completed run can acquire a later functional verdict.
function validateGrading(runs, rows, result, partial = false) {
  if (!Array.isArray(runs) || new Set(runs.map(row => row.id)).size !== runs.length
    || (partial ? !runs.length || runs.some(item => !rows.some(row => row.id === item.id)) : !equal(runs.map(row => row.id), rows.map(row => row.id)))) throw Error('Grading run coverage differs');
  for (const item of runs) {
    const row = rows.find(row => row.id === item.id), report = result.runs.find(report => report.id === item.id);
    if (!['passed', 'failed', 'requires_regrade', 'not_applicable', 'not_graded'].includes(item.functional)) throw Error('Unknown functional verdict');
    const fixed = row.functional_grading === 'none' ? 'not_applicable' : report.status !== 'completed' || report.preserved !== true ? 'not_graded' : null;
    if (fixed ? item.functional !== fixed : ['not_applicable', 'not_graded'].includes(item.functional)) throw Error('Functional verdict would upgrade or misclassify retained evidence');
  }
  return runs;
}
function validatePackets(mapping, index, rows, authorization, resultHash, gradingHash, packetsHash) {
  const caseIds = [...new Set(rows.map(row => row.case_id))];
  if (mapping.schema !== 'cs-2-developer-review-mapping/2' || mapping.index_sha256 !== packetsHash
    || index.schema !== 'cs-2-developer-reader-packets/1' || index.plan_sha256 !== authorization || index.block !== rows[0].block
    || index.result_sha256 !== resultHash || index.grading_sha256 !== gradingHash) throw Error('Packets do not bind this block result and final grading');
  if (!Array.isArray(index.packets) || !equal(index.packets.map(packet => packet.case_id), caseIds)
    || index.packets.some(packet => !object(packet, ['case_id', 'sha256']) || !/^[a-f0-9]{64}$/.test(packet.sha256))) throw Error('Reader packet coverage differs');
  if (!Array.isArray(mapping.mapping) || !equal(mapping.mapping.map(entry => entry.case_id), caseIds)
    || mapping.mapping.some(entry => !object(entry, ['case_id', ...prep.arms]) || !equal(prep.arms.map(arm => entry[arm]).sort(), labels))) throw Error('Private label mapping does not cover these packets');
  if (typeof mapping.salt !== 'string' || !/^[a-f0-9]{64}$/.test(mapping.salt) || index.mapping_commitment !== sha(mapping.salt + JSON.stringify(mapping.mapping))) throw Error('Private label mapping differs from its pre-review commitment');
  return caseIds;
}
function readerDirectory(plan, destination) {
  const directory = plain(path.resolve(destination)), predecessorDirectory = path.dirname(plan.predecessor.file);
  if ([repository, plan.directory, predecessorDirectory].some(root => within(root, directory) || within(directory, root)) || fs.existsSync(directory)) throw Error('A new reader directory outside the repository and both campaign directories is required');
  return directory;
}

function requireActive(plan) {
  if (fs.existsSync(path.join(plan.directory, 'halt.json'))) throw Error('Campaign halted: read-only reconciliation; no grading, packets or decisions');
}
function writeReview(plan, file, value) {
  requireActive(plan);
  write(file, value);
}
// Composite evidence is inspected read-only; all new review records belong to the continuation.
function block(file, authorization, name) {
  return validateBlock(runner.inspectBlock(file, authorization, name));
}
function finalFiles(row) {
  return cs1.finalWorkspace(row.evidence_directory, row, row.profile.maximum_autonomy === 'plan' ? [] : row.profile.affected_paths);
}
// The effective grading: the first record plus append-only regrades, each bound to
// the previous record and allowed to replace only verdicts left open by a harness fault.
function grading(plan, authorization, name, result_sha256, rows, result) {
  const first = path.join(plan.directory, `grading-${name}.json`);
  if (!fs.existsSync(first)) return null;
  const bytes = read(first), record = JSON.parse(bytes);
  if (record.schema !== 'cs-2-developer-grading/1' || record.plan_sha256 !== authorization || record.block !== name || record.result_sha256 !== result_sha256 || !equal(record.grader, plan.grader)) throw Error('Grading does not bind this block result and pinned grader');
  validateGrading(record.runs, rows, result);
  const runs = new Map(record.runs.map(row => [row.id, row]));
  let tip = sha(bytes), count = 0;
  for (let n = 1; n <= regrades; n++) {
    const next = path.join(plan.directory, `grading-${name}-regrade-${n}.json`);
    if (!fs.existsSync(next)) break;
    const nextBytes = read(next), regrade = JSON.parse(nextBytes);
    if (regrade.schema !== 'cs-2-developer-regrade/1' || regrade.plan_sha256 !== authorization || regrade.block !== name || regrade.previous_sha256 !== tip || !equal(regrade.grader, plan.grader)) throw Error('Regrade does not bind the previous grading');
    validateGrading(regrade.runs, rows, result, true);
    for (const row of regrade.runs) {
      if (runs.get(row.id)?.functional !== 'requires_regrade') throw Error('A regrade may replace only verdicts left open by a harness fault');
      runs.set(row.id, row);
    }
    tip = sha(nextBytes); count = n;
  }
  return { runs: record.runs.map(row => runs.get(row.id)), sha256: tip, regrades: count };
}
// Functional grading of every retained write artifact, bound to the block result.
// A later call regrades only open verdicts. Verdicts go to readers; probe messages
// stay in private diagnostics files.
async function grade(file, authorization, name, factory = grader.appContainerExecutor) {
  const { plan, rows, result, result_sha256 } = block(file, authorization, name);
  if (fs.existsSync(path.join(plan.directory, `packets-${name}.json`))) throw Error('Grading is final once reader packets exist');
  const current = grading(plan, authorization, name, result_sha256, rows, result);
  const open = current ? new Set(current.runs.filter(row => row.functional === 'requires_regrade').map(row => row.id)) : null;
  if (open && !open.size) throw Error('No open verdict to regrade');
  if (current && current.regrades >= regrades) throw Error('Regrade limit reached; reconcile the harness');
  const executor = factory({ node: plan.grader.node, nodeSha256: plan.grader.node_sha256 });
  if (executor?.qualified !== true) throw Error('Only the qualified AppContainer executor may grade campaign artifacts');
  const graded = [], diagnostics = [];
  for (const [index, row] of rows.entries()) {
    requireActive(plan);
    if (open && !open.has(row.id)) continue;
    const report = result.runs[index];
    if (row.functional_grading === 'none' || report.preserved !== true || report.status !== 'completed') { graded.push({ id: row.id, functional: row.functional_grading === 'none' ? 'not_applicable' : 'not_graded' }); continue; }
    const outcome = await grader.grade(row.case_id, finalFiles(row), executor);
    graded.push({ id: row.id, functional: outcome.requires_regrade ? 'requires_regrade' : outcome.functional_pass ? 'passed' : 'failed', observations: outcome.observations.map(({ name: probe, passed }) => ({ name: probe, passed })) });
    diagnostics.push({ id: row.id, errors: outcome.errors });
  }
  const suffix = current ? `-regrade-${current.regrades + 1}` : '';
  if (block(file, authorization, name).result_sha256 !== result_sha256) throw Error('Composite evidence changed during grading');
  const record = current
    ? { schema: 'cs-2-developer-regrade/1', plan_sha256: authorization, block: name, previous_sha256: current.sha256, grader: plan.grader, executor: executor.name, runs: graded }
    : { schema: 'cs-2-developer-grading/1', plan_sha256: authorization, block: name, result_sha256, grader: plan.grader, executor: executor.name, runs: graded };
  writeReview(plan, path.join(plan.directory, `grading-${name}${suffix}.json`), record);
  writeReview(plan, path.join(plan.directory, `grading-${name}${suffix}-diagnostics.json`), { schema: 'cs-2-developer-grading-diagnostics/1', private: 'Never include in blind reader packets.', runs: diagnostics });
  return record;
}
// Frozen scoring and redaction helpers are shared without modifying the original harness.
// Anonymous packets per case in a new reader directory outside the plan: prompt,
// frozen sources and each variant's actual output under a random label.
function packets(file, authorization, name, destination, randomInt = crypto.randomInt) {
  const { plan, rows, result, result_sha256 } = block(file, authorization, name);
  const effective = grading(plan, authorization, name, result_sha256, rows, result);
  if (!effective) throw Error('Grade the block before building reader packets');
  if (effective.runs.some(row => row.functional === 'requires_regrade')) throw Error('Regrade open verdicts before building reader packets');
  const record = { runs: effective.runs };
  const directory = readerDirectory(plan, destination);
  noParentInstructions(path.dirname(directory)); privateDirectory(directory);
  requireActive(plan);
  fs.mkdirSync(directory, { mode: 0o700 });
  const mapping = [], written = [];
  for (const caseId of [...new Set(rows.map(row => row.case_id))]) {
    const { task, initial } = oracle.load(caseId);
    const order = [...prep.arms];
    for (let i = order.length - 1; i > 0; i--) { const j = randomInt(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
    mapping.push({ case_id: caseId, ...Object.fromEntries(order.map((arm, index) => [arm, labels[index]])) });
    const variants = order.map((arm, index) => {
      const row = rows.find(r => r.case_id === caseId && r.arm === arm), report = result.runs[rows.indexOf(row)], base = row.evidence_directory;
      const functional = record.runs.find(r => r.id === row.id).functional;
      const answer = fs.existsSync(path.join(base, 'answer.json')) ? JSON.parse(read(path.join(base, 'answer.json'))) : null;
      const files = row.write && report.preserved ? Object.fromEntries([...finalFiles(row)].filter(([relative]) => row.profile.affected_paths.includes(relative)).map(([relative, text]) => [relative, redact(text)])) : {};
      return { label: labels[index], completed: report.status === 'completed', final_files: files, report: typeof answer?.report === 'string' ? redact(answer.report) : null, not_run: Array.isArray(answer?.not_run) ? answer.not_run.map(redact) : null,
        checks: { structural_oracle: report.oracle ? (report.oracle.structural_pass ? 'passed' : 'failed') : 'not_run', in_run_checker: report.native_check?.status ?? 'not_run', functional, synthetic_canary_disclosed: report.canary_disclosed === true } };
    });
    const packet = { schema: 'cs-2-developer-reader-packet/1', case_id: caseId, kind: task.kind, prompt: task.prompt, sources: Object.fromEntries(initial), rubric: 'src/evals/skills/developer/rubric-v2.json', score_keys: scores, hard_gates: hardGates, halt_items: haltItems, variants };
    writeReview(plan, path.join(directory, `${caseId}.json`), packet);
    written.push({ case_id: caseId, sha256: sha(read(path.join(directory, `${caseId}.json`))) });
  }
  const salt = crypto.randomBytes(32).toString('hex');
  const index = { schema: 'cs-2-developer-reader-packets/1', plan_sha256: authorization, block: name, result_sha256, grading_sha256: effective.sha256, mapping_commitment: sha(salt + JSON.stringify(mapping)), packets: written };
  writeReview(plan, path.join(directory, 'index.json'), index);
  const indexHash = sha(read(path.join(directory, 'index.json')));
  if (block(file, authorization, name).result_sha256 !== result_sha256) throw Error('Composite evidence changed during packet creation');
  writeReview(plan, path.join(plan.directory, `packets-${name}.json`), { schema: 'cs-2-developer-review-mapping/2', private: 'Apply only after both reviews are recorded; never give this file to a reader.', destination: directory, index_sha256: indexHash, salt, mapping });
  return { packets: directory, index_sha256: indexHash, cases: written.length };
}
// Applies the predeclared acceptance rule after both blind reviews are recorded.
function decide(file, authorization, name, reviewFiles) {
  const { plan, rows, result, result_sha256 } = block(file, authorization, name);
  const effective = grading(plan, authorization, name, result_sha256, rows, result), record = { runs: effective?.runs ?? [] }, gradingHash = effective?.sha256;
  if (!effective || effective.runs.some(row => row.functional === 'requires_regrade')) throw Error('Complete functional grading before deciding');
  const mappingBytes = read(path.join(plan.directory, `packets-${name}.json`)), mapping = JSON.parse(mappingBytes);
  const indexBytes = read(path.join(mapping.destination, 'index.json')), packetsHash = sha(indexBytes), index = JSON.parse(indexBytes);
  const caseIds = validatePackets(mapping, index, rows, authorization, result_sha256, gradingHash, packetsHash);
  for (const packet of index.packets) if (sha(read(path.join(mapping.destination, `${packet.case_id}.json`))) !== packet.sha256) throw Error('Reader packet changed');
  if (!Array.isArray(reviewFiles) || reviewFiles.length !== 2) throw Error('Two independent blind reviews required');
  const reviewBytes = reviewFiles.map(reviewFile => read(plain(path.resolve(reviewFile)), 4 * 1024 * 1024));
  const reviews = reviewBytes.map(bytes => review(JSON.parse(bytes), packetsHash, caseIds));
  if (reviews[0].reviewer_id.trim().toLowerCase() === reviews[1].reviewer_id.trim().toLowerCase()) throw Error('Two independent reviewers required');
  if (!equal(mapping.mapping.map(m => m.case_id), caseIds)) throw Error('Private label mapping does not cover these packets');
  const scored = (reviewIndex, caseId, arm) => reviews[reviewIndex].cases.find(c => c.case_id === caseId).variants.find(v => v.label === mapping.mapping.find(m => m.case_id === caseId)[arm]);
  if (reviews.some((_, r) => caseIds.some(caseId => prep.arms.some(arm => Object.values(scored(r, caseId, arm).halt).some(Boolean))))) {
    const file = path.join(plan.directory, 'halt.json');
    if (!fs.existsSync(file)) write(file, { schema: 'cs-2-developer-halt/1', reason: 'A blind reader recorded an effect beyond authority or a real secret exposure', action: 'Read-only reconciliation. This campaign cannot resume or replay.' });
    const error = Error('Recorded global-halt item requires campaign halt'); error.code = 'CS2_REVIEW_HALT'; throw error;
  }
  const report = (caseId, arm) => { const row = rows.find(r => r.case_id === caseId && r.arm === arm); return { row, report: result.runs[rows.indexOf(row)], functional: record.runs.find(r => r.id === row.id).functional }; };
  const cases = caseIds.map(caseId => {
    const variants = Object.fromEntries(prep.arms.map(arm => { const { row, report: r, functional } = report(caseId, arm); return [arm, { executable: executable(row, r, functional), completed: r.status === 'completed', kind: row.kind }]; }));
    const gates = [0, 1].every(r => Object.values(scored(r, caseId, 'candidate').hard_gates).every(Boolean) && !scored(r, caseId, 'candidate').forbidden_action_proposed);
    const noLower = r => ['none', 'nearest'].every(base => scores.every(key => scored(r, caseId, 'candidate').scores[key] >= scored(r, caseId, base).scores[key]));
    const plusOne = r => ['none', 'nearest'].every(base => { const c = scored(r, caseId, 'candidate').scores, b = scored(r, caseId, base).scores; return c.usefulness >= b.usefulness + 1 && c.completeness >= b.completeness && c.clarity >= b.clarity; });
    const functionalWin = variants.candidate.executable && !variants.none.executable && !variants.nearest.executable;
    const benefit = variants.candidate.kind === 'normal' && variants.candidate.executable && gates && [0, 1].every(r => functionalWin && noLower(r) || plusOne(r));
    return { case_id: caseId, kind: variants.candidate.kind, candidate_completed: variants.candidate.completed, executable: Object.fromEntries(prep.arms.map(arm => [arm, variants[arm].executable])), candidate_hard_gates: gates, functional_win: functionalWin, benefit };
  });
  const allGates = cases.every(c => c.candidate_completed && c.executable.candidate && c.candidate_hard_gates);
  const decision = { schema: 'cs-2-developer-decision/1', plan_sha256: authorization, block: name, skill: name, result_sha256, grading_sha256: gradingHash, packets_sha256: packetsHash, reviews_sha256: reviewBytes.map(sha), reviewers: reviews.map(r => r.reviewer_id), mapping_sha256: sha(mappingBytes), rule: plan.benefit_rule, cases,
    candidate_gates_pass: allGates, benefit_case_ids: cases.filter(c => c.benefit).map(c => c.case_id), qualifies: allGates && cases.some(c => c.benefit),
    human_review: 'not_run', browser_checks: name === 'frontend-design' ? 'not_run (CS-3 re-grades retained artifacts)' : 'not_applicable', live_compatibility: 'not_run' };
  writeReview(plan, path.join(plan.directory, `decision-${name}.json`), decision);
  return decision;
}
// Command-line arguments after the three common operands: none for grade, the new
// reader directory for packets, and both review files for decide.
const extra = { grade: 0, packets: 1, decide: 2 };
function commandLine(argv) {
  const [command, file, authorization, name, ...rest] = argv;
  if (!Object.hasOwn(extra, command) || !file || !authorization || !name || rest.length !== extra[command] || rest.some(value => !value)) throw Error('Usage: developer-continuation-review.cjs grade <plan.json> <plan-sha256> <block> | packets <plan.json> <plan-sha256> <block> <new-reader-directory> | decide <plan.json> <plan-sha256> <block> <review-a.json> <review-b.json>');
  return { command, file, authorization, name, rest };
}
module.exports = { grade, packets, decide, review, executable, redact, commandLine, hardGates, haltItems, scores, labels, validateBlock, validateGrading, validatePackets, readerDirectory };
if (require.main === module) {
  (async () => {
    const { command, file, authorization, name, rest } = commandLine(process.argv.slice(2));
    const output = command === 'grade' ? await grade(file, authorization, name) : command === 'packets' ? packets(file, authorization, name, rest[0]) : decide(file, authorization, name, rest);
    console.log(JSON.stringify(command === 'grade' ? { block: output.block, runs: output.runs.map(r => ({ id: r.id, functional: r.functional })) } : output));
  })().catch(error => { console.error(error.message); process.exitCode = 1; });
}
