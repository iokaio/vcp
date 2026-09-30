// SPDX-License-Identifier: Apache-2.0
'use strict';
// Build-time asset inventory. Product reads remain behind the native skill boundary.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const repository = path.resolve(__dirname, '../..');
const source = path.join(repository, 'src/skills/builtin');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function portable(relative) {
  if (typeof relative !== 'string' || relative.length > 512 || !relative ||
      relative.split('/').some(part => !part || part === '.' || part === '..' ||
        /[<>:"\\|?*\x00-\x1f]/.test(part) || /[. ]$/.test(part) ||
        /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part))) {
    throw Error('Invalid portable asset path');
  }
  return relative;
}
function bounded(file, maximum = 1024 * 1024) {
  const info = fs.lstatSync(file);
  if (!info.isFile() || info.isSymbolicLink()) throw Error('Asset must be an ordinary file');
  const fd = fs.openSync(file, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
  try {
    const buffer = Buffer.alloc(maximum + 1);
    let length = 0, count;
    do { count = fs.readSync(fd, buffer, length, buffer.length - length, null); length += count; }
    while (count && length < buffer.length);
    if (length > maximum) throw Error('Asset byte limit exceeded');
    return buffer.subarray(0, length);
  } finally { fs.closeSync(fd); }
}
function enumerate(root) {
  const found = [];
  let entries = 0;
  function visit(relative, depth) {
    if (depth > 5 || ++entries > 256) throw Error('Asset traversal limit exceeded');
    const absolute = path.join(root, relative), stat = fs.lstatSync(absolute);
    if (stat.isSymbolicLink()) throw Error('Linked asset paths are not allowed');
    if (stat.isDirectory()) {
      const directory = fs.opendirSync(absolute);
      try {
        for (let entry; (entry = directory.readSync());) {
          visit(portable(relative ? relative + '/' + entry.name : entry.name), depth + 1);
        }
      } finally { directory.closeSync(); }
    } else if (stat.isFile()) found.push(relative);
    else throw Error('Unsupported asset file type');
  }
  visit('', 0);
  return found.sort();
}
function inspectAssets(root, expectedCatalog = bounded(path.join(source, 'catalog.json'))) {
  root = path.resolve(root);
  const paths = enumerate(root);
  const catalogBytes = bounded(path.join(root, 'catalog.json'));
  if (!catalogBytes.equals(expectedCatalog)) throw Error('Catalog does not match selected source inventory');
  const catalog = JSON.parse(catalogBytes);
  if (catalog.schema_version !== 1 || typeof catalog.version !== 'string' ||
      !Array.isArray(catalog.skills) || !catalog.skills.length || catalog.skills.length > 64) {
    throw Error('Unsupported builtin catalog');
  }
  const expected = new Map([['catalog.json', digest(catalogBytes)]]), identities = new Set();
  function add(relative, sha256) {
    portable(relative);
    if (!/^[a-f0-9]{64}$/.test(sha256) || expected.has(relative)) throw Error('Duplicate asset or invalid digest');
    expected.set(relative, sha256);
  }
  add(catalog.coverage.path, catalog.coverage.sha256);
  // ADR-070: an omitted resource role means context; bodies are always context.
  const role = content => {
    const value = content?.use ?? 'context';
    if (!['context', 'file'].includes(value)) throw Error('Invalid builtin resource role');
    return value;
  };
  const sameContent = (left, right) => left?.path === right?.path && left?.sha256 === right?.sha256 &&
    role(left) === role(right);
  for (const skill of catalog.skills) {
    if (!/^[a-z][a-z0-9-]*$/.test(skill.id) || identities.has(skill.id) ||
        skill.descriptor !== skill.id + '/skill.json' || !Array.isArray(skill.resources)) {
      throw Error('Invalid or duplicate builtin skill identity');
    }
    identities.add(skill.id);
    add(skill.descriptor, skill.descriptor_sha256);
    if (role(skill.body) !== 'context') throw Error('Builtin skill body must be context');
    add(skill.id + '/' + portable(skill.body.path), skill.body.sha256);
    for (const resource of skill.resources) add(skill.id + '/' + portable(resource.path), resource.sha256);
  }
  const wanted = [...expected.keys()].sort();
  if (JSON.stringify(paths) !== JSON.stringify(wanted)) throw Error('Missing or unexpected builtin assets');
  const captured = new Map();
  let total = 0;
  for (const relative of wanted) {
    const bytes = bounded(path.join(root, relative));
    if (digest(bytes) !== expected.get(relative)) throw Error('Asset hash mismatch: ' + relative);
    if ((total += bytes.length) > 16 * 1024 * 1024) throw Error('Total asset byte limit exceeded');
    captured.set(relative, bytes);
  }
  for (const skill of catalog.skills) {
    const descriptor = JSON.parse(captured.get(skill.descriptor));
    if (descriptor.id !== skill.id || descriptor.version !== skill.version ||
        descriptor.source !== skill.source || descriptor.license !== skill.license ||
        !sameContent(descriptor.body, skill.body) || !Array.isArray(descriptor.resources) ||
        descriptor.resources.length !== skill.resources.length ||
        descriptor.resources.some((resource, index) => !sameContent(resource, skill.resources[index]))) {
      throw Error('Descriptor differs from catalog attribution or content');
    }
  }
  const inventory = {
    schema: 'vcp-builtin-asset-inventory/1', version: catalog.version,
    catalog_sha256: digest(catalogBytes), skills: catalog.skills.length, total_bytes: total,
    files: wanted.map(relative => ({ path: relative, bytes: captured.get(relative).length, sha256: expected.get(relative) })),
  };
  return { inventory, captured };
}
function stageAssets(root, destination, expectedCatalog) {
  const { inventory, captured } = inspectAssets(root, expectedCatalog);
  destination = path.resolve(destination);
  // The caller creates only the parent; an existing destination is never replaced.
  fs.mkdirSync(destination);
  for (const [relative, bytes] of captured) {
    const target = path.join(destination, relative);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, bytes, { flag: 'wx' });
  }
  inspectAssets(destination, captured.get('catalog.json'));
  return inventory;
}
// Authoring aid: recompute content, descriptor and coverage digests after edits.
// Versions stay explicit author decisions; coverage family versions follow descriptors.
function rehashAssets(root) {
  root = path.resolve(root);
  const json = value => JSON.stringify(value, null, 2) + '\n';
  const read = relative => bounded(path.join(root, portable(relative)));
  const catalog = JSON.parse(read('catalog.json'));
  const coverage = JSON.parse(read(catalog.coverage.path));
  const writes = new Map();
  for (const entry of catalog.skills) {
    const descriptor = JSON.parse(read(entry.descriptor));
    if (descriptor.id !== entry.id) throw Error('Descriptor id differs from catalog entry: ' + entry.id);
    for (const content of [descriptor.body, ...descriptor.resources]) {
      content.sha256 = digest(read(entry.id + '/' + content.path));
    }
    const bytes = Buffer.from(json(descriptor));
    writes.set(entry.descriptor, bytes);
    Object.assign(entry, {
      version: descriptor.version, descriptor_sha256: digest(bytes), body: descriptor.body,
      source: descriptor.source, license: descriptor.license, resources: descriptor.resources,
    });
    const family = coverage.families?.find(item => item.id === entry.id);
    if (family) family.version = descriptor.version;
  }
  const coverageBytes = Buffer.from(json(coverage));
  writes.set(catalog.coverage.path, coverageBytes);
  catalog.coverage.sha256 = digest(coverageBytes);
  writes.set('catalog.json', Buffer.from(json(catalog)));
  const changed = [];
  for (const [relative, bytes] of writes) {
    if (!read(relative).equals(bytes)) { fs.writeFileSync(path.join(root, relative), bytes); changed.push(relative); }
  }
  return { changed, inventory: inspectAssets(root, writes.get('catalog.json')).inventory };
}
if (require.main === module) {
  try {
    const [command, root, destination, ...extra] = process.argv.slice(2);
    if (extra.length || !root || !['verify', 'stage', 'rehash'].includes(command) ||
        (command === 'stage') !== Boolean(destination)) throw Error('Use verify <assets>, rehash <assets> or stage <assets> <new-directory>');
    const result = command === 'verify' ? inspectAssets(root).inventory
      : command === 'rehash' ? { changed: rehashAssets(root).changed } : stageAssets(root, destination);
    process.stdout.write(JSON.stringify(result) + '\n');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { inspectAssets, stageAssets, rehashAssets, portable };
