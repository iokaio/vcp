// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const quality = require('./p6-task-quality.cjs');
const gateFile = path.resolve(__dirname, '../../src/evals/tasks/p6-profile-gate-v1.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function read(file) {
  const fd = fs.openSync(file, 'r'), limit = 16 * 1024 * 1024;
  try {
    const stat = fs.fstatSync(fd);
    if (!stat.isFile() || stat.size > limit) throw Error('Qualification input exceeds bound or is not a regular file');
    const bytes = Buffer.alloc(stat.size + 1);
    let used = 0;
    while (used < bytes.length) {
      const n = fs.readSync(fd, bytes, used, bytes.length - used, null);
      if (!n) break;
      used += n;
    }
    if (used > stat.size) throw Error('Qualification input changed during read');
    return bytes.subarray(0, used);
  } finally { fs.closeSync(fd); }
}
function tail(n, k, p) {
  if (p === 0) return 0;
  if (p === 1) return 1;
  let choose = 0, sum = 0;
  for (let x = 0; x <= n; x++) {
    if (x >= k) sum += Math.exp(choose + x * Math.log(p) + (n - x) * Math.log1p(-p));
    if (x < n) choose += Math.log(n - x) - Math.log(x + 1);
  }
  return Math.min(1, sum);
}
function lowerBound(passed, planned, confidence = 0.95) {
  if (!Number.isSafeInteger(planned) || planned < 0 || planned > 10000 || !Number.isSafeInteger(passed) || passed < 0 || passed > planned || !(confidence > 0 && confidence < 1)) throw Error('Invalid binomial sample');
  if (!passed) return 0;
  let low = 0, high = 1;
  for (let i = 0; i < 64; i++) {
    const mid = (low + high) / 2;
    if (tail(planned, passed, mid) < 1 - confidence) low = mid; else high = mid;
  }
  return (low + high) / 2;
}
function percentile(values, p) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
}
function summarize(result, pool = quality.load()) {
  const gateBytes = read(gateFile), gate = JSON.parse(gateBytes);
  if (!gate.task_manifests.includes(pool.manifest.revision)) throw Error('Manifest is outside the frozen gate');
  if (result.schema !== 'p6-live-result/1' || result.manifest_sha256 !== pool.manifest_sha256 || !Array.isArray(result.runs)) throw Error('Execution identity mismatch');
  const submission = {revision:pool.manifest.revision, manifest_sha256:pool.manifest_sha256,
    runs:result.runs.map(({case_id,strategy,start_state_sha256,status,answer}) => ({case_id,strategy,start_state_sha256,status,answer}))};
  const graded = quality.grade(submission, pool);
  const summaries = [];
  for (const strategy of pool.manifest.strategies) for (const partition of ['tuning', 'held_out']) {
    const selected = graded.runs.filter(r => r.strategy === strategy.id && r.partition === partition);
    const receipts = selected.map(r => result.runs.find(x => x.case_id === r.case_id && x.strategy === r.strategy));
    const knownCost = receipts.every(r => Number.isSafeInteger(r?.actual_cost_micros) && r.actual_cost_micros >= 0);
    const costs = knownCost ? receipts.reduce((sum, r) => sum + r.actual_cost_micros, 0) : null;
    if (costs !== null && !Number.isSafeInteger(costs)) throw Error('Cost sum exceeds exact integer bound');
    const latencies = receipts.filter(r => Number.isSafeInteger(r?.latency_ms) && r.latency_ms >= 0).map(r => r.latency_ms);
    const classes = ['analysis', 'review', 'generation'].map(task_class => {
      const rows = selected.filter(r => r.task_class === task_class), passed = rows.filter(r => r.task_success).length;
      const lower = lowerBound(passed, rows.length, gate.one_sided_confidence);
      return {task_class, planned:rows.length, passed, unsuccessful:rows.length - passed,
        success_lower_bound:lower, minimum_sample_met:rows.length >= gate.minimum_independent_heldout_tasks_per_class,
        population_floor_met:lower >= gate.candidate_population_success_floor};
    });
    const serious = selected.filter(r => !r.task_success).length;
    const reasons = [];
    if (partition !== 'held_out') reasons.push('tuning cannot qualify held-out defaults');
    if (classes.some(c => !c.minimum_sample_met)) reasons.push('insufficient independent held-out tasks per class');
    if (classes.some(c => !c.population_floor_met)) reasons.push('confidence bound below candidate quality floor');
    if (serious > gate.maximum_serious_failures) reasons.push('unsuccessful tasks conservatively fail serious-failure gate');
    if (!knownCost) reasons.push('incomplete attributable cost');
    if (latencies.length !== selected.length) reasons.push('incomplete wall-time evidence');
    // The frozen corpus itself excludes statistical activation even on all-pass receipts.
    reasons.push('frozen smoke corpus does not establish representative independent samples');
    summaries.push({strategy:strategy.id, partition, planned:selected.length, passed:selected.filter(r => r.task_success).length,
      unsuccessful:serious, actual_cost_micros:costs, latency_observations:latencies.length,
      latency_p50_ms:percentile(latencies, 0.5), latency_p95_ms:percentile(latencies, 0.95), classes,
      activation_eligible:false, rejection_reasons:reasons});
  }
  return {schema:'p6-profile-qualification/1', manifest_sha256:pool.manifest_sha256, gate_sha256:sha(gateBytes),
    qualification_source_sha256:sha(read(__filename)), grader_sha256:graded.grader_sha256,
    evidence_kind:'source-bound interpretation of supplied live-runner receipts; canonical capture audit remains required',
    decision:'reject_automatic_default_activation', enabled_defaults:[], retained_defaults:'explicit configured values only',
    summaries, limitations:[gate.uncertainty, gate.release_limit], rollback:gate.rollback};
}
function report(directory) {
  const planBytes = read(path.join(directory, 'plan.json')), resultBytes = read(path.join(directory, 'result.json'));
  const plan = JSON.parse(planBytes), result = JSON.parse(resultBytes), pool = quality.load(plan.manifest_revision);
  if (plan.schema !== 'p6-live-plan/1' || result.authorization !== true || result.plan_sha256 !== sha(planBytes) || plan.manifest_sha256 !== pool.manifest_sha256 || plan.grader_sha256 !== sha(read(path.join(__dirname, 'p6-task-quality.cjs'))) || plan.runner_sha256 !== sha(read(path.join(__dirname, 'p6-live-runner.cjs')))) throw Error('Plan/result/source binding mismatch');
  if (plan.profile_gate_sha256 !== sha(read(gateFile)) || plan.profile_reporter_sha256 !== sha(read(__filename))) throw Error('Frozen gate or reporting source binding mismatch');
  if (plan.runs.length !== pool.cases.length * pool.manifest.strategies.length || result.runs.length !== plan.runs.length || result.runs.some((r,i) => r.case_id !== plan.runs[i].case_id || r.strategy !== plan.runs[i].strategy || r.start_state_sha256 !== plan.runs[i].start_state_sha256)) throw Error('Execution order or denominator changed');
  return {...summarize(result, pool), plan_sha256:sha(planBytes), result_sha256:sha(resultBytes), executable_sha256:plan.executable_sha256, strategies:plan.strategies};
}
module.exports = {lowerBound, percentile, summarize, report};
if (require.main === module) {
  try {
    if (process.argv.length !== 3) throw Error('Usage: node scripts/evals/p6-profile-qualification.cjs <private-trial-directory>');
    process.stdout.write(JSON.stringify(report(process.argv[2]), null, 2) + '\n');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
