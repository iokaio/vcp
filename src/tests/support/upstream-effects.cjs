// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const TOML = require('@iarna/toml');
const { validatePath } = require('./upstream-inventory.cjs');

// Discover path dependencies as Cargo workspace members, including members
// reached only through dev/target dependencies. This does not resolve features
// or claim to replace Cargo's target-specific dependency graph.
function sourceResolver(root, externalRoots = []) {
  const realRoot = fs.realpathSync(root);
  const allowed = [realRoot, ...externalRoots.map(directory => fs.realpathSync(directory))];
  const inside = file => allowed.some(directory => file === directory || file.startsWith(directory + path.sep));
  return relative => {
    if (!externalRoots.length) validatePath(relative);
    else {
      if (typeof relative !== 'string' || path.posix.normalize(relative) !== relative) throw Error('Unsafe or nonportable workspace path');
      validatePath(relative.replace(/^(?:\.\.\/)+/, ''));
    }
    const lexical = path.resolve(realRoot, relative);
    if (!inside(lexical)) throw Error('Unsafe or nonportable workspace path: outside selected roots');
    const resolved = fs.realpathSync(lexical);
    if (!inside(resolved)) throw Error('Workspace source escaped its selected roots');
    return resolved;
  };
}
function workspacePackages(root, { externalRoots = [] } = {}) {
  const resolve = sourceResolver(root, externalRoots);
  function read(relative) {
    return TOML.parse(fs.readFileSync(resolve(relative), 'utf8'));
  }
  const workspace = read('Cargo.toml').workspace;
  if (!Array.isArray(workspace?.members) || workspace.exclude?.length) throw Error('Unsupported workspace membership rules');
  const byDirectory = new Map(), names = new Set();
  function visit(directory) {
    resolve(directory);
    if (byDirectory.has(directory)) return;
    const document = read(directory + '/Cargo.toml');
    const name = document.package?.name;
    if (typeof name !== 'string' || !/^[a-zA-Z0-9_-]+$/.test(name) || names.has(name)) throw Error('Invalid or duplicate workspace package');
    names.add(name);
    const item = { name, manifest: directory + '/Cargo.toml', dependencyDirectories: new Set() };
    byDirectory.set(directory, item);
    function dependencies(table) {
      for (const [key, value] of Object.entries(table || {})) {
        if (!value || typeof value !== 'object' || Array.isArray(value)) continue;
        if (['dependencies', 'dev-dependencies', 'build-dependencies'].includes(key)) {
          for (const [dependency, specification] of Object.entries(value)) {
            if (!specification || typeof specification !== 'object') continue;
            const inherited = specification.workspace === true;
            const actual = inherited ? workspace.dependencies?.[dependency] : specification;
            if (inherited && !actual) throw Error('Missing inherited dependency: ' + dependency);
            if (typeof actual?.path === 'string') {
              const target = path.posix.normalize(path.posix.join(inherited ? '' : directory, actual.path));
              resolve(target);
              item.dependencyDirectories.add(target);
              visit(target);
            }
          }
        } else if (key !== 'workspace') dependencies(value);
      }
    }
    dependencies(document);
  }
  workspace.members.forEach(visit);
  return [...byDirectory.values()].map(item => ({ name: item.name, manifest: item.manifest,
    dependencies: [...item.dependencyDirectories].map(directory => byDirectory.get(directory).name).sort() }))
    .sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
}
function validateBoundaries(catalog, packages, { root, commit, taskIds, externalRoots = [] }) {
  if (catalog.schema_version !== 1 || catalog.component !== 'codex' || catalog.commit !== commit ||
      catalog.verification !== 'static_only' || typeof catalog.scope !== 'string' || !catalog.scope ||
      !Array.isArray(catalog.groups) || !catalog.groups.length || !Array.isArray(catalog.seams) || !catalog.seams.length) throw Error('Invalid boundary inventory identity or scope');
  const available = new Map(packages.map(item => [item.name, item]));
  const assigned = new Set(), groups = new Map(), seams = new Set();
  const capabilities = new Set(['data', 'instruction', 'model', 'scheduler', 'filesystem', 'process', 'network', 'credential', 'telemetry', 'clock']);
  for (const group of catalog.groups) {
    if (typeof group.id !== 'string' || !/^[a-z][a-z0-9-]+$/.test(group.id) || groups.has(group.id) ||
        !['retain-with-adapters', 'disable-upstream-route', 'retain-data', 'test-build-only'].includes(group.handling) ||
        !Array.isArray(group.packages) || !group.packages.length ||
        !Array.isArray(group.capabilities) || !group.capabilities.length || group.capabilities.some(value => !capabilities.has(value)) ||
        !Array.isArray(group.owner_tasks) || !group.owner_tasks.length || group.owner_tasks.some(id => !taskIds.has(id)) ||
        typeof group.gate !== 'string' || !group.gate.trim()) throw Error('Invalid boundary group');
    groups.set(group.id, new Set(group.packages));
    for (const name of group.packages) {
      if (!available.has(name) || assigned.has(name)) throw Error('Unknown or multiply assigned package: ' + name);
      assigned.add(name);
    }
  }
  if (assigned.size !== available.size) throw Error('Unassigned workspace packages: ' + [...available.keys()].filter(name => !assigned.has(name)).join(', '));
  const resolve = sourceResolver(root, externalRoots);
  for (const seam of catalog.seams) {
    const owner = available.get(seam.package);
    const source = resolve(seam.path);
    if (typeof seam.id !== 'string' || !/^[a-z][a-z0-9-]+$/.test(seam.id) || seams.has(seam.id) || !groups.get(seam.group)?.has(seam.package) || !owner ||
        !seam.path.startsWith(path.posix.dirname(owner.manifest) + '/') || typeof seam.symbol !== 'string' || !seam.symbol.trim() ||
        typeof seam.observation !== 'string' || !seam.observation.trim()) throw Error('Invalid effect seam');
    seams.add(seam.id);
    if (!fs.readFileSync(source, 'utf8').includes(seam.symbol)) throw Error('Stale source symbol: ' + seam.id);
  }
  return { packages: assigned.size, groups: groups.size, seams: seams.size, verification: 'static_only', status: 'pass' };
}
module.exports = { workspacePackages, validateBoundaries };
