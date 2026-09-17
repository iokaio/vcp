// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { ownedRoot } = require('../support/experiments.cjs');
const { workspacePackages, validateBoundaries } = require('../support/upstream-effects.cjs');
function write(root, file, text) { const target = path.join(root, file); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, text); }
test('workspace discovery includes implicit inherited, target and dev path dependencies', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    write(fixture.root, 'Cargo.toml', '[workspace]\nmembers=["main"]\n[workspace.dependencies]\nshared={path="shared"}\n');
    write(fixture.root, 'main/Cargo.toml', '[package]\nname="main"\n[dependencies]\nshared.workspace=true\n[target."cfg(windows)".dev-dependencies]\nhelper={path="../helper"}\n');
    write(fixture.root, 'shared/Cargo.toml', '[package]\nname="shared"\n');
    write(fixture.root, 'helper/Cargo.toml', '[package]\nname="helper"\n[dev-dependencies]\nmain={path="../main"}\n');
    const packages = workspacePackages(fixture.root);
    assert.deepEqual(packages.map(item => item.name), ['helper', 'main', 'shared']);
    assert.deepEqual(packages.find(item => item.name === 'main').dependencies, ['helper', 'shared']);
    write(fixture.root, 'helper/Cargo.toml', '[package]\nname="shared"\n');
    assert.throws(() => workspacePackages(fixture.root), /duplicate workspace package/);
  } finally { fixture.cleanup(); }
});
test('workspace discovery rejects escaping paths, unresolved inheritance and unsupported rules', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    write(fixture.root, 'Cargo.toml', '[workspace]\nmembers=["main"]\n');
    write(fixture.root, 'main/Cargo.toml', '[package]\nname="main"\n[dependencies]\nmissing.workspace=true\n');
    assert.throws(() => workspacePackages(fixture.root), /Missing inherited/);
    write(fixture.root, 'main/Cargo.toml', '[package]\nname="main"\n[dependencies]\nouter={path="../../outside"}\n');
    assert.throws(() => workspacePackages(fixture.root), /Unsafe or nonportable/);
    write(fixture.root, 'main/Cargo.toml', '[package]\nversion="1.0.0"\n');
    assert.throws(() => workspacePackages(fixture.root), /Invalid or duplicate/);
    write(fixture.root, 'Cargo.toml', '[workspace]\nmembers=["main"]\nexclude=["helper"]\n');
    assert.throws(() => workspacePackages(fixture.root), /Unsupported workspace/);
  } finally { fixture.cleanup(); }
});
test('boundary coverage rejects missing, unknown and multiply owned packages', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    write(fixture.root, 'example/src/lib.rs', 'pub fn execute() {}\n');
    const packages = [{ name: 'example', manifest: 'example/Cargo.toml', dependencies: [] }];
    const options = { root: fixture.root, commit: 'a'.repeat(40), taskIds: new Set(['P0-08']) };
    const catalog = { schema_version: 1, component: 'codex', commit: options.commit, verification: 'static_only', scope: 'Synthetic navigation check',
      groups: [{ id: 'example', packages: ['example'], capabilities: ['process'], handling: 'retain-with-adapters', owner_tasks: ['P0-08'], gate: 'VCP authority' }],
      seams: [{ id: 'execute', group: 'example', package: 'example', path: 'example/src/lib.rs', symbol: 'pub fn execute', observation: 'Synthetic fixture' }] };
    assert.equal(validateBoundaries(catalog, packages, options).packages, 1);
    assert.throws(() => validateBoundaries(catalog, [...packages, { name: 'new-package', manifest: 'new/Cargo.toml' }], options), /Unassigned workspace/);
    const duplicate = structuredClone(catalog); duplicate.groups[0].packages.push('example');
    assert.throws(() => validateBoundaries(duplicate, packages, options), /multiply assigned/);
    const unknown = structuredClone(catalog); unknown.groups[0].packages.push('unknown');
    assert.throws(() => validateBoundaries(unknown, packages, options), /Unknown or multiply/);
    const invalidOwner = structuredClone(catalog); invalidOwner.groups[0].owner_tasks = ['P0-99'];
    assert.throws(() => validateBoundaries(invalidOwner, packages, options), /Invalid boundary group/);
    const wrongGroup = structuredClone(catalog);
    wrongGroup.groups.push({ ...wrongGroup.groups[0], id: 'other', packages: ['other'] });
    wrongGroup.seams[0].group = 'other';
    assert.throws(() => validateBoundaries(wrongGroup, [...packages, { name: 'other', manifest: 'other/Cargo.toml' }], options), /Invalid effect seam/);
    const stale = structuredClone(catalog); stale.seams[0].symbol = 'pub fn missing';
    assert.throws(() => validateBoundaries(stale, packages, options), /Stale source symbol/);
    const escaped = structuredClone(catalog); escaped.seams[0].path = '../outside';
    assert.throws(() => validateBoundaries(escaped, packages, options), /Unsafe or nonportable/);
    assert.throws(() => validateBoundaries({ ...catalog, verification: 'runtime_qualified' }, packages, options), /Invalid boundary inventory/);
    assert.throws(() => validateBoundaries(catalog, packages, { ...options, commit: 'b'.repeat(40) }), /Invalid boundary inventory/);
  } finally { fixture.cleanup(); }
});
