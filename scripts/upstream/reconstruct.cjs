// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { readComponent, reconstruct, compareTree, compareIndex, sha256 } = require('../../src/tests/support/upstream-selection.cjs');
function main(argv) {
  const mode = argv.shift(), options = {};
  if (!['reconstruct', 'verify', 'verify-index'].includes(mode)) throw Error('Expected reconstruct, verify or verify-index');
  for (let i = 0; i < argv.length; i += 2) {
    if (!['--component', '--source', '--output', '--record'].includes(argv[i]) || Object.hasOwn(options, argv[i]) || !argv[i + 1]) throw Error('Invalid reconstruction arguments');
    options[argv[i]] = argv[i + 1];
  }
  const required = mode !== 'reconstruct' ? ['--component'] : ['--component', '--source', '--output', '--record'];
  if (required.some(key => !options[key]) || Object.keys(options).length !== required.length) throw Error('Required options: ' + required.join(', '));
  const repository = path.resolve(__dirname, '../..');
  const { component, selection } = readComponent(repository, options['--component']);
  if (mode !== 'reconstruct') {
    const record = JSON.parse(fs.readFileSync(path.join(repository, selection.result_inventory), 'utf8'));
    if (record.component !== component.id || record.commit !== component.commit || record.selection_sha256 !== sha256(JSON.stringify(selection))) throw Error('Result inventory identity mismatch');
    const errors = compareTree(path.join(repository, selection.destination), record);
    if (errors.length) throw Error(errors.join('\n'));
    if (mode === 'verify-index') compareIndex(repository, selection.destination, record);
    console.log(JSON.stringify({ component: component.id, files: record.files.length, files_sha256: record.files_sha256, status: 'pass' }));
  } else {
    const recordFile = path.resolve(options['--record']);
    const output = path.resolve(options['--output']);
    const relative = path.relative(output, recordFile);
    if (!relative || (!relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative))) throw Error('Result record must be outside reconstructed source');
    if (fs.existsSync(recordFile)) throw Error('Result record already exists');
    const result = reconstruct({ repository, source: path.resolve(options['--source']), output, component, selection });
    fs.writeFileSync(recordFile, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
    console.log(JSON.stringify({ component: component.id, files: result.files.length, files_sha256: result.files_sha256, status: 'pass' }));
  }
}
try { main(process.argv.slice(2)); }
catch (error) { console.error('Reconstruction failed: ' + error.message); process.exitCode = 1; }
