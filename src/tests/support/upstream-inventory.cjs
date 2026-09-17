// SPDX-License-Identifier: Apache-2.0
'use strict';
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const MAX_BYTES = 256 * 1024 * 1024;
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function validatePath(file) {
  if (typeof file !== 'string' || !file || file.includes('\\') || file.startsWith('/') ||
      file.split('/').some(part => !part || part === '.' || part === '..' || /[<>:"|?*\x00-\x1f]/.test(part) ||
        /[. ]$/.test(part) || /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part) || part.toLowerCase() === '.git')) {
    throw Error('Unsafe or nonportable source path: ' + file);
  }
  return file;
}
function parseTree(bytes) {
  const entries = [], names = new Set(), prefixes = new Map();
  let total = 0;
  for (const record of new TextDecoder('utf-8', { fatal: true }).decode(bytes).split('\0').filter(Boolean)) {
    const match = record.match(/^(100644|100755|120000) blob ([a-f0-9]{40})\s+(\d+)\t([\s\S]+)$/);
    if (!match) throw Error('Unsupported tree entry (submodules and special files are forbidden)');
    const [, mode, object, sizeText, file] = match;
    validatePath(file);
    const normalized = file.normalize('NFC').toLowerCase();
    if (names.has(normalized)) throw Error('Duplicate or case-colliding source path: ' + file);
    names.add(normalized);
    const parts = file.split('/');
    for (let i = 1; i <= parts.length; i++) {
      const name = parts.slice(0, i).join('/');
      const key = name.normalize('NFC').toLowerCase();
      const kind = i === parts.length ? 'entry' : 'directory';
      const prior = prefixes.get(key);
      if (prior && (prior.name !== name || prior.kind !== kind)) throw Error('Case-colliding directory or file: ' + file);
      prefixes.set(key, { name, kind });
    }
    const size = Number(sizeText);
    if (!Number.isSafeInteger(size) || size < 0 || (total += size) > MAX_BYTES) throw Error('Source inventory exceeds byte limit');
    entries.push({ path: file, mode, object, bytes: size });
  }
  return entries.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
}
function attachContent(entries, batch) {
  let offset = 0;
  const result = [];
  for (const entry of entries) {
    const end = batch.indexOf(10, offset);
    if (end < 0) throw Error('Truncated object header');
    const header = batch.subarray(offset, end).toString('ascii');
    if (header !== entry.object + ' blob ' + entry.bytes) throw Error('Unexpected object identity or size');
    offset = end + 1;
    if (offset + entry.bytes >= batch.length || batch[offset + entry.bytes] !== 10) throw Error('Truncated object bytes');
    const content = batch.subarray(offset, offset + entry.bytes);
    const actualObject = crypto.createHash('sha1').update('blob ' + content.length + '\0').update(content).digest('hex');
    if (actualObject !== entry.object) throw Error('Git blob identity mismatch');
    const item = { ...entry, sha256: sha256(content), kind: entry.mode === '120000' ? 'symlink' : 'file' };
    if (item.kind === 'symlink') item.link_target = content.toString('utf8');
    result.push(item);
    offset += entry.bytes + 1;
  }
  if (offset !== batch.length) throw Error('Unexpected trailing object data');
  return result;
}
function inventory(root, commit) {
  if (!/^[a-f0-9]{40}$/.test(commit)) throw Error('An immutable full commit ID is required');
  const git = (args, input) => execFileSync('git', args, {
    cwd: root, input, stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: MAX_BYTES + 16 * 1024 * 1024
  });
  if (git(['cat-file', '-t', commit]).toString().trim() !== 'commit') throw Error('Revision is not a commit');
  const tree = git(['rev-parse', commit + '^{tree}']).toString().trim();
  const entries = parseTree(git(['ls-tree', '-rlz', commit]));
  const content = git(['cat-file', '--batch'], entries.map(e => e.object + '\n').join(''));
  const files = attachContent(entries, content);
  return { schema_version: 1, commit, tree, encoding: 'UTF-8 paths; original Git blob bytes, no normalization',
    digest: 'SHA-256', files_sha256: sha256(JSON.stringify(files)), files };
}
module.exports = { validatePath, parseTree, attachContent, inventory };
