// SPDX-License-Identifier: Apache-2.0
'use strict';
const review = require('./authoring-requalification-review-runner.cjs');
module.exports = review;
if (require.main === module) {
  try {
    const [command, ...args] = process.argv.slice(2);
    if (command === 'packets' && args.length === 5) console.log(JSON.stringify(review.packets(...args)));
    else if (command === 'project' && args.length === 7) console.log(JSON.stringify(review.projections(...args)));
    else throw Error('Usage: authoring-requalification-review.cjs packets <envelope> <hash> <candidate> <phase> <new-reader-dir> | project <envelope> <hash> <candidate> <phase> <reader-1-json> <reader-2-json> <new-owner-dir>');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}