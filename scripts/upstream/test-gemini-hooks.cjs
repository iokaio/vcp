// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const { main } = require('./test-gemini.cjs');
const fixture = path.resolve(__dirname, '../../src/tests/fixtures/gemini/hooks-upstream.json');
main(process.argv.slice(2), {
  task_id: 'P10-01', suites: require(fixture).suites, fixture, runner: __filename
}).then(code => { process.exitCode = code; }).catch(error => {
  console.error('Gemini hooks qualification failed: ' + error.message); process.exitCode = 2;
});
