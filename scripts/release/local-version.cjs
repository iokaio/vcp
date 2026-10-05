// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { compareTree } = require('../../src/tests/support/upstream-selection.cjs');
const check = (condition, message) => { if (!condition) throw Error(message); };
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const numeric = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const patchPath = 'src/third_party/patches/codex/local-candidate-product-version.patch';
function uniquePaths(paths) {
  const seen = new Set();
  return paths.filter(value => {
    const identity = process.platform === 'win32' ? value.toLowerCase() : value;
    if (seen.has(identity)) return false; seen.add(identity); return true;
  });
}
function version(value) {
  check(typeof value === 'string' && numeric.test(value), 'Numeric product version required: ' + value);
  const parts = value.split('.').map(Number);
  check(parts.every(Number.isSafeInteger), 'Product version exceeds integer range');
  return parts;
}
function compare(a, b) {
  const left = version(a), right = version(b);
  for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] < right[i] ? -1 : 1;
  return 0;
}
function ordinary(filename, missing = false) {
  for (let cursor = filename; ; cursor = path.dirname(cursor)) {
    if (fs.existsSync(cursor)) {
      const stat = fs.lstatSync(cursor);
      check(!stat.isSymbolicLink(), 'Linked version input rejected: ' + cursor);
      if (cursor === filename) check(stat.isFile(), 'Ordinary version input required: ' + filename);
    } else if (cursor === filename) check(missing, 'Missing version input: ' + filename);
    if (cursor === path.dirname(cursor)) break;
  }
}
function directoryPath(filename) {
  for (let cursor = filename; ; cursor = path.dirname(cursor)) {
    if (fs.existsSync(cursor)) {
      const stat = fs.lstatSync(cursor);
      check(stat.isDirectory() && !stat.isSymbolicLink(), 'Ordinary candidate directory required: ' + cursor);
    }
    if (cursor === path.dirname(cursor)) break;
  }
}
// Record JSON token locations so changing versions preserves all other bytes,
// including dependency pins, formatting and the optional UTF-8 BOM.
function jsonDocument(text) {
  const value = JSON.parse(text.replace(/^\uFEFF/, '')), locations = new Map();
  let cursor = text.startsWith('\uFEFF') ? 1 : 0;
  const space = () => { while (/\s/.test(text[cursor] || '') && cursor < text.length) cursor++; };
  function string() {
    const start = cursor++;
    while (cursor < text.length) {
      if (text[cursor++] === '"') return JSON.parse(text.slice(start, cursor));
      if (text[cursor - 1] === '\\') cursor++;
    }
    throw Error('Unterminated JSON string');
  }
  function visit(keys, depth = 0) {
    check(depth <= 100, 'JSON nesting exceeds version preparation bound');
    space(); const start = cursor;
    if (text[cursor] === '{') {
      cursor++; space(); const seen = new Set();
      if (text[cursor] !== '}') while (true) {
        const key = string(); check(!seen.has(key), 'Duplicate JSON key: ' + key); seen.add(key);
        space(); check(text[cursor++] === ':', 'Invalid JSON object'); visit([...keys, key], depth + 1); space();
        if (text[cursor] === '}') break;
        check(text[cursor++] === ',', 'Invalid JSON object'); space();
      }
      cursor++;
    } else if (text[cursor] === '[') {
      cursor++; space(); let index = 0;
      if (text[cursor] !== ']') while (true) {
        visit([...keys, index++], depth + 1); space(); if (text[cursor] === ']') break;
        check(text[cursor++] === ',', 'Invalid JSON array');
      }
      cursor++;
    } else if (text[cursor] === '"') string();
    else { const token = text.slice(cursor).match(/^(?:true|false|null|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/); check(token, 'Invalid JSON value'); cursor += token[0].length; }
    locations.set(JSON.stringify(keys), { start, end: cursor });
  }
  visit([]); space(); check(cursor === text.length, 'Unexpected trailing JSON');
  return { value, replace(changes) {
    const edits = changes.map(([keys, replacement]) => {
      const range = locations.get(JSON.stringify(keys)); check(range, 'Missing version field: ' + keys.join('.'));
      return { ...range, replacement: JSON.stringify(replacement) };
    }).sort((a, b) => b.start - a.start);
    let result = text;
    for (const edit of edits) result = result.slice(0, edit.start) + edit.replacement + result.slice(edit.end);
    return result;
  } };
}
function cargoVersion(text, locked) {
  const header = locked ? /^\[\[package\]\][ \t]*\r?$/gm : /^\[package\][ \t]*\r?$/gm;
  const headers = [...text.matchAll(header)], sections = [];
  for (const row of headers) {
    const end = text.indexOf('\n[', row.index + row[0].length);
    const finish = end < 0 ? text.length : end;
    const body = text.slice(row.index, finish);
    if (/^name\s*=\s*"vcp-cli"\s*$/m.test(body)) sections.push({ start: row.index, body });
  }
  check(sections.length === 1, 'Exactly one vcp-cli package required');
  const matches = [...sections[0].body.matchAll(/^version\s*=\s*"([^"]+)"[ \t]*\r?$/gm)];
  check(matches.length === 1, 'Exactly one native package version required');
  const match = matches[0], start = sections[0].start + match.index + match[0].indexOf('"') + 1;
  version(match[1]);
  return { value: match[1], start, replace: selected => text.slice(0, start) + selected + text.slice(start + match[1].length) };
}
function producedVersions(root, outputRoot) {
  const found = [];
  for (const directory of uniquePaths([path.join(root, 'artifacts/local-setup'), path.join(root, 'artifacts/local-vsix'), path.resolve(outputRoot)])) {
    directoryPath(directory);
    if (!fs.existsSync(directory)) continue;
    check(fs.lstatSync(directory).isDirectory() && !fs.lstatSync(directory).isSymbolicLink(), 'Ordinary candidate output root required');
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (!numeric.test(entry.name)) continue;
      check(!entry.isSymbolicLink(), 'Linked candidate version rejected');
      // Any numeric path is reserved, even if a failed build left a file.
      version(entry.name); found.push(entry.name);
    }
  }
  // Signed packets have fixed small identities; never walk their artifact trees.
  const candidates = path.join(root, 'artifacts/local-candidate');
  if (fs.existsSync(candidates)) {
    check(fs.lstatSync(candidates).isDirectory() && !fs.lstatSync(candidates).isSymbolicLink(), 'Ordinary signed candidate root required');
    for (const entry of fs.readdirSync(candidates, { withFileTypes: true })) {
      if (!entry.isDirectory() || entry.isSymbolicLink()) continue;
      const identity = path.join(candidates, entry.name, 'packet/pair.json');
      if (!fs.existsSync(identity)) continue;
      ordinary(identity); check(fs.statSync(identity).size < 1024 * 1024, 'Candidate identity exceeds bound');
      const value = jsonDocument(fs.readFileSync(identity, 'utf8')).value;
      for (const key of ['native_version', 'sdk_version', 'vsix_version']) {
        const selected = value.release?.[key];
        if (selected && numeric.test(selected)) { version(selected); found.push(selected); }
      }
    }
  }
  return found;
}
function prepare(root, outputRoot = path.join(root, 'artifacts/local-setup')) {
  root = path.resolve(root); outputRoot = path.resolve(outputRoot);
  const inputs = new Map(), edits = new Map();
  function read(relative) {
    const filename = path.join(root, relative); ordinary(filename);
    const bytes = fs.readFileSync(filename); inputs.set(filename, bytes); return bytes.toString('utf8');
  }
  const channelFile = 'release/internal-beta.json', channel = jsonDocument(read(channelFile));
  check(channel.value.schema === 'vcp-beta-channel/1' && channel.value.channel === 'internal-beta', 'Unsupported release channel');
  const declarations = ['native_version', 'sdk_version', 'vsix_version'].map(key => channel.value[key]);
  const nativeFile = 'src/crates/vcp-cli/Cargo.toml', lockFile = 'src/third_party/codex/codex-rs/Cargo.lock';
  const native = cargoVersion(read(nativeFile), false), lock = cargoVersion(read(lockFile), true);
  declarations.push(native.value, lock.value);
  const documents = [];
  for (const name of ['sdk-ts', 'vscode']) {
    const manifestFile = 'src/packages/' + name + '/package.json', packageLockFile = 'src/packages/' + name + '/package-lock.json';
    const manifest = jsonDocument(read(manifestFile)), packageLock = jsonDocument(read(packageLockFile));
    check(manifest.value.name === (name === 'sdk-ts' ? '@vcp/sdk' : 'vcp'), 'Unexpected package identity');
    check(packageLock.value.lockfileVersion === 3 && packageLock.value.name === manifest.value.name && packageLock.value.packages?.['']?.name === manifest.value.name, 'Unsupported package lock');
    const keys = [['version'], ['packages', '', 'version']];
    if (name === 'vscode') {
      check(packageLock.value.packages?.['../sdk-ts']?.name === '@vcp/sdk', 'Missing bundled SDK lock');
      keys.push(['packages', '../sdk-ts', 'version']);
    }
    declarations.push(manifest.value.version, ...keys.map(keys => keys.reduce((value, key) => value?.[key], packageLock.value)));
    documents.push({ manifestFile, packageLockFile, manifest, packageLock, keys });
  }
  declarations.forEach(version);
  const current = declarations.reduce((a, b) => compare(a, b) >= 0 ? a : b);
  const produced = producedVersions(root, outputRoot);
  let selected = current;
  if (produced.some(value => compare(value, current) >= 0)) {
    const latest = produced.reduce((a, b) => compare(a, b) >= 0 ? a : b, current), parts = version(latest);
    check(parts[2] < Number.MAX_SAFE_INTEGER, 'No next numeric patch version'); parts[2]++; selected = parts.join('.');
  }
  edits.set(channelFile, channel.replace(['native_version', 'sdk_version', 'vsix_version'].map(key => [[key], selected])));
  edits.set(nativeFile, native.replace(selected)); edits.set(lockFile, lock.replace(selected));
  for (const doc of documents) {
    edits.set(doc.manifestFile, doc.manifest.replace([[['version'], selected]]));
    edits.set(doc.packageLockFile, doc.packageLock.replace(doc.keys.map(keys => [keys, selected])));
  }
  const selectionFile = 'src/third_party/components/codex-selection.json', recordFile = 'src/third_party/components/codex-files.json';
  const selection = jsonDocument(read(selectionFile)), record = jsonDocument(read(recordFile));
  check(selection.value.component === 'codex' && selection.value.destination === 'src/third_party/codex' && selection.value.result_inventory === recordFile &&
    record.value.component === 'codex' && record.value.commit === selection.value.commit && record.value.selection_sha256 === hash(JSON.stringify(selection.value)), 'Codex provenance identity mismatch');
  check(Array.isArray(selection.value.patches), 'Missing Codex patch series');
  const seen = new Set();
  for (const patch of selection.value.patches) {
    check(typeof patch.path === 'string' && /^src\/third_party\/patches\/codex\/[a-zA-Z0-9.-]+\.patch$/.test(patch.path) && !seen.has(patch.path), 'Invalid Codex patch path');
    seen.add(patch.path); check(hash(Buffer.from(read(patch.path))) === patch.sha256, 'Codex patch digest mismatch');
  }
  const errors = compareTree(path.join(root, selection.value.destination), record.value);
  check(errors.length === 0, 'Codex source changed before version preparation: ' + errors.slice(0, 3).join(', '));
  if (selected !== lock.value) {
    const rowIndex = record.value.files.findIndex(row => row.path === 'codex-rs/Cargo.lock');
    check(rowIndex >= 0 && record.value.files[rowIndex].transformation === 'patch-series', 'Native lock provenance missing');
    const existing = selection.value.patches.findIndex(row => row.path === patchPath);
    const lockText = inputs.get(path.join(root, lockFile)).toString(), newline = lockText.includes('\r\n') ? '\r\n' : '\n';
    let base = lock.value;
    if (existing >= 0) {
      check(existing === selection.value.patches.length - 1, 'Automatic version patch must be final');
      const prior = inputs.get(path.join(root, patchPath)).toString();
      const normalized = prior.replaceAll('\r\n', '\n');
      const expected = versionPatch(normalized.match(/^-version = "([^"]+)"$/m)?.[1], lock.value, Number(normalized.match(/^@@ -(\d+),4 \+\d+,4 @@$/m)?.[1]), newline);
      check(prior === expected, 'Unexpected automatic version patch'); base = normalized.match(/^-version = "([^"]+)"$/m)[1]; version(base);
    } else {
      ordinary(path.join(root, patchPath), true);
      check(!fs.existsSync(path.join(root, patchPath)), 'Unregistered automatic version patch already exists');
    }
    const line = lockText.slice(0, lock.start).split('\n').length - 2;
    const patch = versionPatch(base, selected, line, newline);
    edits.set(patchPath, patch);
    if (existing < 0) {
      selection.value.patches.push({ path: patchPath, sha256: hash(patch) });
      const readme = 'src/third_party/patches/codex/README.md', before = read(readme);
      const newline = before.includes('\r\n') ? '\r\n' : '\n';
      edits.set(readme, before.replace(/\s*$/, '') + newline + newline +
        'Local setup preparation maintains the final ' + newline +
        '`local-candidate-product-version.patch` to synchronize only' + newline +
        'the VCP CLI Cargo.lock package version. Its destination version, selection' + newline +
        'digest and resulting lock inventory are refreshed together before each' + newline +
        'new local candidate; original upstream acquisition evidence is retained.' + newline);
    }
    else selection.value.patches[existing].sha256 = hash(patch);
    const selectionText = selection.replace([[['patches'], selection.value.patches]]);
    // Preserve conventional indentation in the inserted patch array only.
    edits.set(selectionFile, selectionText.replace(JSON.stringify(selection.value.patches), JSON.stringify(selection.value.patches, null, 2).replace(/\n/g, '\n  ')));
    const newLock = Buffer.from(edits.get(lockFile));
    record.value.files[rowIndex].result.bytes = newLock.length; record.value.files[rowIndex].result.sha256 = hash(newLock);
    edits.set(recordFile, record.replace([
      [['selection_sha256'], hash(JSON.stringify(selection.value))], [['files_sha256'], hash(JSON.stringify(record.value.files))],
      [['files', rowIndex, 'result', 'bytes'], newLock.length], [['files', rowIndex, 'result', 'sha256'], hash(newLock)],
    ]));
  }
  // All parsing, hashes, expected fields and destinations pass before writes.
  for (const [relative] of edits) ordinary(path.join(root, relative), relative === patchPath);
  for (const [filename, before] of inputs) check(fs.readFileSync(filename).equals(before), 'Version source changed during preparation');
  const changed = [], reservations = [];
  const canonicalReservation = path.join(root, 'artifacts/local-setup', selected), versionRoot = path.join(outputRoot, selected);
  for (const directory of uniquePaths([canonicalReservation, versionRoot])) {
    directoryPath(directory); check(!fs.existsSync(directory), 'Candidate reservation already exists: ' + directory);
  }
  // The caller holds the repository build lock until artifact construction ends.
  // Canonical reservations prevent reuse when a later build changes OutputRoot.
  try {
    for (const [relative, text] of edits) {
      const filename = path.join(root, relative), bytes = Buffer.from(text);
      if (inputs.get(filename)?.equals(bytes)) continue;
      if (inputs.has(filename)) changed.push(relative);
      fs.writeFileSync(filename, bytes, { flag: inputs.has(filename) ? 'w' : 'wx' });
      if (!inputs.has(filename)) changed.push(relative);
    }
    for (const directory of uniquePaths([canonicalReservation, versionRoot])) {
      fs.mkdirSync(path.dirname(directory), { recursive: true });
      check(!fs.existsSync(directory), 'Candidate reservation already exists: ' + directory);
      fs.mkdirSync(directory); reservations.push(directory);
    }
  } catch (error) {
    const rollbackErrors = [];
    for (const relative of changed.reverse()) {
      const filename = path.join(root, relative), before = inputs.get(filename);
      try { if (before) fs.writeFileSync(filename, before); else fs.unlinkSync(filename); }
      catch (rollback) { rollbackErrors.push(rollback.message); }
    }
    for (const directory of reservations.reverse()) {
      try { fs.rmdirSync(directory); } catch (rollback) { rollbackErrors.push(rollback.message); }
    }
    if (rollbackErrors.length) error.message += '; rollback failed: ' + rollbackErrors.join('; ');
    throw error;
  }
  return { version: selected, previous_version: channel.value.native_version, changed, canonicalReservation, versionRoot };
}
function versionPatch(base, selected, line, newline = '\n') {
  version(base); version(selected); check(Number.isSafeInteger(line) && line > 0, 'Invalid native version patch position');
  return 'diff --git a/codex-rs/Cargo.lock b/codex-rs/Cargo.lock\n--- a/codex-rs/Cargo.lock\n+++ b/codex-rs/Cargo.lock\n' +
    '@@ -' + line + ',4 +' + line + ',4 @@\n' + [' [[package]]', ' name = "vcp-cli"', '-version = "' + base + '"', '+version = "' + selected + '"', ' dependencies = ['].join(newline) + newline;
}
module.exports = { prepare, compare, jsonDocument, cargoVersion };
if (require.main === module) {
  try {
    const [command, repository, outputRoot, ...extra] = process.argv.slice(2);
    check(command === 'prepare' && repository && outputRoot && extra.length === 0, 'Expected prepare <repository> <outputRoot>');
    console.log(JSON.stringify(prepare(repository, outputRoot)));
  } catch (error) { console.error('Local version preparation failed: ' + error.message); process.exitCode = 1; }
}
