// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const fs = require('node:fs');
const { parseSelection, runSuite, sourceIdentity } = require('../src/tests/support/harness.cjs');
async function main() {
  const root = path.resolve(__dirname, '..');
  const registry = JSON.parse(fs.readFileSync(path.join(root, 'src/tests/registry.json'), 'utf8'));
  let selection;
  try { selection = parseSelection(process.argv.slice(2), registry); }
  catch (error) { console.error(error.message); return 2; }
  if (Number(process.versions.node.split('.')[0]) < 24) {
    console.error('Node.js 24 or later is required; tests were not run.');
    return 3;
  }
  const stop = new AbortController();
  const cancel = () => stop.abort();
  process.on('SIGINT', cancel);
  process.on('SIGTERM', cancel);
  try {
    const result = await runSuite({
      root, registry, selection, source: sourceIdentity(root),
      outputRoot: selection.outputRoot || path.join(root, 'artifacts', 'tests'),
      signal: stop.signal,
      announce: (command) => console.log('Run: ' + JSON.stringify(command))
    });
    console.log(JSON.stringify({
      run_id: result.manifest.run_id, status: result.manifest.status,
      attempts: result.manifest.attempts.map(a => ({case: a.case_id, backend: a.backend, status: a.status, exit_code: a.exit_code, reason: a.reason})),
      manifest: path.relative(root, result.manifestPath).replaceAll('\\', '/')
    }, null, 2));
    return result.exitCode;
  } finally {
    process.removeListener('SIGINT', cancel);
    process.removeListener('SIGTERM', cancel);
  }
}
main().then(code => { process.exitCode = code; }).catch(error => {
  console.error('Harness failure: ' + error.message);
  process.exitCode = 1;
});
