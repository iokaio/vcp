// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const { specification, outside, verify, acquire } = require('../../src/tests/support/model-assets.cjs');
async function main(argv) {
  if (argv.length !== 3 || !['acquire', 'verify'].includes(argv[0]) || argv[1] !== '--root' || argv[2].startsWith('--')) {
    const error = Error('Required: model-assets.cjs acquire|verify --root <model directory outside checkout>'); error.exitCode = 2; throw error;
  }
  const repository = path.resolve(__dirname, '../..'), root = outside(argv[2], [repository]);
  const { spec, sha256 } = specification(repository);
  const result = await (argv[0] === 'acquire' ? acquire(root, spec) : verify(root, spec));
  console.log(JSON.stringify({ status: 'pass', revision: spec.revision, specification_sha256: sha256, ...result }));
}
main(process.argv.slice(2)).catch(error => {
  const missing = error.code === 'ENOENT';
  console.error(JSON.stringify({ status: missing ? 'not_run' : 'fail', reason: missing ? 'Missing local model assets; acquire the pinned selection explicitly' : error.message }));
  process.exitCode = error.exitCode || (missing ? 3 : 1);
});
