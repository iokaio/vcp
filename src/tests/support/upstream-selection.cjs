// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const os = require('node:os');
const { execFileSync } = require('node:child_process');
const TOML = require('@iarna/toml');
const { inventory, validatePath, attachContent, parseTree } = require('./upstream-inventory.cjs');
const { ownedRoot } = require('./experiments.cjs');
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const canonical = value => JSON.stringify(value);
function readComponent(repository, id) {
  const manifest = TOML.parse(fs.readFileSync(path.join(repository, 'src/third_party/upstreams.toml'), 'utf8'));
  if (manifest.schema_version !== 1 || !Array.isArray(manifest.upstream)) throw Error('Unsupported upstream manifest');
  const ids = new Set();
  for (const component of manifest.upstream) {
    if (!/^[a-z][a-z0-9-]*$/.test(component.id) || ids.has(component.id) ||
        !/^https:\/\/github\.com\/[a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+$/.test(component.origin) ||
        !/^[a-f0-9]{40}$/.test(component.commit) ||
        !['candidate', 'imported_unqualified', 'qualified'].includes(component.state)) throw Error('Invalid or duplicate upstream identity');
    ids.add(component.id);
  }
  const component = manifest.upstream.find(item => item.id === id);
  if (!component?.selection) throw Error('No selected source for component: ' + id);
  validatePath(component.selection);
  const selection = JSON.parse(fs.readFileSync(path.join(repository, 'src/third_party', component.selection), 'utf8'));
  validateSelection(selection, component);
  return { component, selection };
}
function validateSelection(selection, component) {
  const permitted = ['schema_version', 'component', 'commit', 'destination', 'include', 'materialize_links', 'patches', 'result_inventory', 'licenses', 'closure', 'owner_task'];
  if (Object.keys(selection).some(key => !permitted.includes(key)) || selection.schema_version !== 1 ||
      selection.component !== component.id || selection.commit !== component.commit ||
      selection.destination !== 'src/third_party/' + component.id || selection.owner_task !== 'P0-07') throw Error('Invalid selection identity');
  validatePath(selection.result_inventory);
  if (!selection.result_inventory.startsWith('src/third_party/components/')) throw Error('Inventory must live in the component record directory');
  if (!Array.isArray(selection.include) || !selection.include.length || new Set(selection.include).size !== selection.include.length) throw Error('Invalid includes');
  for (const item of selection.include) validatePath(item.endsWith('/') ? item.slice(0, -1) : item);
  if (!selection.materialize_links || typeof selection.materialize_links !== 'object' || Array.isArray(selection.materialize_links)) throw Error('Invalid link policy');
  for (const [from, to] of Object.entries(selection.materialize_links)) { validatePath(from); validatePath(to); }
  if (!Array.isArray(selection.patches) || new Set(selection.patches.map(p => p.path)).size !== selection.patches.length) throw Error('Invalid patch series');
  for (const patch of selection.patches) {
    validatePath(patch.path);
    if (!patch.path.startsWith('src/third_party/patches/' + component.id + '/') || !/^[a-f0-9]{64}$/.test(patch.sha256)) throw Error('Invalid patch identity');
  }
  for (const key of ['licenses', 'closure']) {
    if (!Array.isArray(selection[key]) || !selection[key].length) throw Error('Missing ' + key + ' records');
    selection[key].forEach(validatePath);
  }
}
function prepareSelection(source, component, selection) {
  validateSelection(selection, component);
  const blobs = new Map();
  const original = inventory(source, component.commit, (entry, bytes) => blobs.set(entry.path, bytes));
  if (original.tree !== component.tree || original.files_sha256 !== component.inventory_sha256) throw Error('Candidate inventory does not match manifest');
  const selected = original.files.filter(entry => selection.include.some(item => item.endsWith('/') ? entry.path.startsWith(item) : entry.path === item));
  for (const item of selection.include) if (!selected.some(entry => item.endsWith('/') ? entry.path.startsWith(item) : entry.path === item)) throw Error('Empty include: ' + item);
  const names = new Set(selected.map(entry => entry.path));
  for (const required of [...selection.licenses, ...selection.closure]) if (!names.has(required)) throw Error('Required selection input missing: ' + required);
  const result = [];
  for (const entry of selected) {
    let bytes = blobs.get(entry.path), mode = entry.mode, transformation = 'none';
    if (entry.kind === 'symlink') {
      const target = path.posix.normalize(path.posix.join(path.posix.dirname(entry.path), entry.link_target));
      validatePath(target);
      if (selection.materialize_links[entry.path] !== target || !names.has(target) ||
          original.files.find(file => file.path === target)?.kind !== 'file') throw Error('Unqualified symlink: ' + entry.path);
      bytes = blobs.get(target); mode = '100644'; transformation = 'materialize:' + target;
    } else if (Object.hasOwn(selection.materialize_links, entry.path)) throw Error('Link policy points to a regular file');
    result.push({ path: entry.path, original: { mode: entry.mode, bytes: entry.bytes, sha256: entry.sha256 },
      result: { mode, bytes: bytes.length, sha256: sha256(bytes) }, transformation, content: bytes });
  }
  for (const link of Object.keys(selection.materialize_links)) if (!names.has(link)) throw Error('Unselected link policy');
  return result;
}
function applyPatches(output, prepared, patches) {
  if (!patches.length) return prepared.map(entry => ({ path: entry.path, ...entry.result }));
  // Git's index preserves executable modes on Windows. Metadata is separate from
  // the reconstructed source and needs no commit, author identity or nested repo.
  const scratch = ownedRoot(os.tmpdir());
  try {
    const gitDir = path.join(scratch.root, 'objects.git');
    execFileSync('git', ['init', '--bare', gitDir], { stdio: ['ignore', 'pipe', 'pipe'] });
    const base = ['--git-dir=' + gitDir, '--work-tree=' + output, '-c', 'core.bare=false', '-c', 'core.filemode=false', '-c', 'core.autocrlf=false'];
    const git = (args, input) => execFileSync('git', [...base, ...args], {
      cwd: output, input, stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: 272 * 1024 * 1024
    });
    const objects = git(['hash-object', '-w', '--no-filters', '--stdin-paths'], prepared.map(entry => entry.path + '\n').join('')).toString().trim().split('\n');
    if (objects.length !== prepared.length) throw Error('Incomplete patch index');
    for (let i = 0; i < prepared.length; i++) {
      const bytes = prepared[i].content;
      const expected = crypto.createHash('sha1').update('blob ' + bytes.length + '\0').update(bytes).digest('hex');
      if (objects[i].trim() !== expected) throw Error('Source changed before patch staging');
    }
    git(['update-index', '-z', '--index-info'], prepared.map((entry, i) => entry.result.mode + ' ' + objects[i].trim() + '\t' + entry.path + '\0').join(''));
    git(['update-index', '--refresh']);
    for (let i = 0; i < patches.length; i++) {
      const file = path.join(scratch.root, String(i) + '.patch');
      fs.writeFileSync(file, patches[i], { flag: 'wx' });
      git(['apply', '--check', '--index', '--', file]);
      git(['apply', '--index', '--', file]);
    }
    const entries = git(['ls-files', '--stage', '-z']).toString().split('\0').filter(Boolean).map(record => {
      const match = record.match(/^(100644|100755) ([a-f0-9]{40}) 0\t(.+)$/);
      if (!match) throw Error('Patch produced an unsupported file kind');
      validatePath(match[3]);
      return { mode: match[1], object: match[2], path: match[3] };
    });
    const input = entries.map(entry => entry.object + '\n').join('');
    const sizes = git(['cat-file', '--batch-check'], input).toString().trim().split('\n');
    let total = 0;
    entries.forEach((entry, i) => {
      const match = sizes[i]?.match(/^([a-f0-9]{40}) blob (\d+)$/);
      if (!match || match[1] !== entry.object) throw Error('Invalid patched object');
      entry.bytes = Number(match[2]); total += entry.bytes;
    });
    if (total > 256 * 1024 * 1024) throw Error('Patched source exceeds byte limit');
    // Reuse full-tree validation, including case/directory collisions, for patch
    // additions as well as the original source selection.
    parseTree(Buffer.from(entries.map(entry => `${entry.mode} blob ${entry.object} ${entry.bytes}\t${entry.path}\0`).join('')));
    return attachContent(entries, git(['cat-file', '--batch'], input)).map(({ path: file, mode, bytes, sha256: hash }) => ({ path: file, mode, bytes, sha256: hash }));
  } finally { scratch.cleanup(); }
}
function diskFiles(root) {
  const files = [];
  function visit(directory, prefix) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const relative = prefix + entry.name;
      validatePath(relative);
      if (entry.isSymbolicLink()) throw Error('Unexpected link in reconstructed source: ' + relative);
      if (entry.isDirectory()) visit(path.join(directory, entry.name), relative + '/');
      else if (entry.isFile()) {
        const file = path.join(directory, entry.name), content = fs.readFileSync(file);
        files.push({ path: relative, bytes: content.length, sha256: sha256(content), mode: (fs.statSync(file).mode & 0o111) ? '100755' : '100644' });
      } else throw Error('Unsupported reconstructed file: ' + relative);
    }
  }
  visit(root, '');
  return files.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
}
function reconstruct({ repository, source, output, component, selection }) {
  // Verify the complete pinned inputs before allocating output; never overwrite a tree.
  if (fs.existsSync(output)) throw Error('Reconstruction destination already exists');
  const prepared = prepareSelection(source, component, selection);
  const patches = selection.patches.map(patch => {
    const bytes = fs.readFileSync(path.join(repository, patch.path));
    if (sha256(bytes) !== patch.sha256) throw Error('Patch digest mismatch: ' + patch.path);
    return bytes;
  });
  fs.mkdirSync(output);
  for (const entry of prepared) {
    const destination = path.join(output, entry.path);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, entry.content, { flag: 'wx', mode: entry.result.mode === '100755' ? 0o755 : 0o644 });
    if (process.platform !== 'win32') fs.chmodSync(destination, entry.result.mode === '100755' ? 0o755 : 0o644);
  }
  const expectedFiles = applyPatches(output, prepared, patches).sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  const resultingNames = new Set(expectedFiles.map(entry => entry.path));
  for (const required of [...selection.licenses, ...selection.closure]) {
    if (!resultingNames.has(required)) throw Error('Patch series removed required selection input: ' + required);
  }
  const originals = new Map(prepared.map(entry => [entry.path, entry]));
  const files = expectedFiles.map(entry => {
    const previous = originals.get(entry.path);
    return { path: entry.path, original: previous?.original || null,
      result: { bytes: entry.bytes, sha256: entry.sha256, mode: entry.mode },
      transformation: selection.patches.length ? 'patch-series' : previous.transformation };
  });
  const removed = prepared.filter(entry => !expectedFiles.some(file => file.path === entry.path)).map(entry => entry.path);
  const result = { schema_version: 1, component: component.id, commit: component.commit,
    selection_sha256: sha256(canonical(selection)), files_sha256: sha256(canonical(files)), files, removed };
  const errors = compareTree(output, result);
  if (errors.length) throw Error('Reconstruction differs from declared inputs: ' + errors.join(', '));
  return result;
}
function compareTree(root, expected) {
  if (expected.schema_version !== 1 || expected.files_sha256 !== sha256(canonical(expected.files))) throw Error('Invalid result inventory');
  for (const entry of expected.files) {
    validatePath(entry.path);
    if (!['100644', '100755'].includes(entry.result?.mode) || !Number.isSafeInteger(entry.result.bytes) || entry.result.bytes < 0 ||
        !/^[a-f0-9]{64}$/.test(entry.result.sha256)) throw Error('Invalid result file');
  }
  const actual = diskFiles(root), errors = [], desired = new Map(expected.files.map(entry => [entry.path, entry.result]));
  if (desired.size !== expected.files.length) throw Error('Duplicate result inventory path');
  for (const file of actual) {
    const result = desired.get(file.path);
    if (!result) errors.push('Unexpected: ' + file.path);
    else if (file.bytes !== result.bytes || file.sha256 !== result.sha256 || (process.platform !== 'win32' && file.mode !== result.mode)) errors.push('Changed: ' + file.path);
    desired.delete(file.path);
  }
  for (const file of desired.keys()) errors.push('Missing: ' + file);
  return errors;
}
function compareIndex(repository, destination, expected) {
  validatePath(destination);
  const git = (args, input) => execFileSync('git', args, { cwd: repository, input, maxBuffer: 272 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'] });
  const records = git(['ls-files', '--stage', '-z', '--', destination + '/']).toString().split('\0').filter(Boolean);
  const desired = new Map(expected.files.map(entry => [entry.path, entry.result]));
  const entries = records.map(record => {
    const match = record.match(/^(100644|100755) ([a-f0-9]{40}) 0\t(.+)$/);
    if (!match || !match[3].startsWith(destination + '/')) throw Error('Unsupported index entry');
    const file = match[3].slice(destination.length + 1), result = desired.get(file);
    if (!result || result.mode !== match[1]) throw Error('Unexpected index path or mode: ' + file);
    desired.delete(file);
    return { path: file, mode: match[1], object: match[2], bytes: result.bytes };
  });
  if (desired.size) throw Error('Missing index paths: ' + desired.size);
  const files = attachContent(entries, git(['cat-file', '--batch'], entries.map(entry => entry.object + '\n').join('')));
  const hashes = new Map(expected.files.map(entry => [entry.path, entry.result.sha256]));
  for (const file of files) if (file.sha256 !== hashes.get(file.path)) throw Error('Changed indexed bytes: ' + file.path);
  return files.length;
}
module.exports = { readComponent, validateSelection, prepareSelection, reconstruct, compareTree, compareIndex, diskFiles, applyPatches, sha256 };
