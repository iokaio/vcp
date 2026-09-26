// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const runner = require('./authoring-requalification-runner.cjs');
module.exports = runner;
if (require.main === module) {
  try {
    const [command, ...args] = process.argv.slice(2); let result;
    if (command === 'prepare' && args.length === 2) result = runner.prepare(...args);
    else if (command === 'phase' && args.length === 4) result = runner.preparePhase(...args);
    else if (command === 'review' && args.length === 5) result = runner.recordReview(...args);
    else if (command === 'run' && args.length === 4) result = runner.run(...args);
    else throw Error('Usage: authoring-requalification.cjs prepare <spec> <new-private-dir> | phase <envelope> <envelope-sha256> <candidate> <normal|inherited|confirmation> | review <envelope> <envelope-sha256> <candidate> <phase> <owner-receipt> | run <envelope> <envelope-sha256> <phase-plan> <phase-sha256>');
    console.log(JSON.stringify(command === 'run' ? { result: path.join(path.dirname(args[2]), 'result.json'), stopped: result.stopped, candidate_stopped: result.candidate_stopped, actual_cost_micros: result.actual_cost_micros, observed_attempts: result.observed_attempts } : result));
    if (result.stopped || result.runs?.some(row => row.status !== 'completed')) process.exitCode = 1;
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}