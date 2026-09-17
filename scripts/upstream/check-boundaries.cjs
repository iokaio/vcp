// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { readComponent } = require('../../src/tests/support/upstream-selection.cjs');
const { validatePath } = require('../../src/tests/support/upstream-inventory.cjs');
const { workspacePackages, validateBoundaries } = require('../../src/tests/support/upstream-effects.cjs');
try {
  if (process.argv.length !== 2) throw Error('This check takes no arguments');
  const repository = path.resolve(__dirname, '../..');
  const { component, selection } = readComponent(repository, 'codex');
  const root = path.join(repository, selection.destination, 'codex-rs');
  validatePath(component.boundary_inventory);
  const catalog = JSON.parse(fs.readFileSync(path.join(repository, 'src/third_party', component.boundary_inventory), 'utf8'));
  const ledger = fs.readFileSync(path.join(repository, 'docs/plan/20-traceability.md'), 'utf8');
  const taskIds = new Set([...ledger.matchAll(/^\| (P\d+-\d{2}) /gm)].map(match => match[1]));
  console.log(JSON.stringify(validateBoundaries(catalog, workspacePackages(root), { root, commit: component.commit, taskIds })));
} catch (error) { console.error('Boundary inventory failed: ' + error.message); process.exitCode = 1; }
