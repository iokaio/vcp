// SPDX-License-Identifier: Apache-2.0
'use strict';
// Read selected metadata/license bytes directly from checksum-verified Cargo
// archives. Never extract untrusted tar paths into the filesystem.
const zlib = require('node:zlib');
const { hash } = require('./provenance.cjs');
function crateFiles(compressed, expected, visit) {
  if (hash(compressed) !== expected) throw Error('Cargo archive checksum mismatch');
  const tar = zlib.gunzipSync(compressed, { maxOutputLength: 256 * 1024 * 1024 });
  const text = bytes => bytes.toString('utf8').replace(/\0.*$/s, '');
  let nextName = null, count = 0;
  for (let offset = 0; offset < tar.length;) {
    if (offset + 512 > tar.length) throw Error('Truncated Cargo tar header');
    const header = tar.subarray(offset, offset + 512); offset += 512;
    if (header.every(byte => byte === 0)) break;
    const octal = bytes => { const value = text(bytes).trim(); if (!/^[0-7]+$/.test(value)) throw Error('Unsupported Cargo tar number'); return parseInt(value, 8); };
    const checksum = octal(header.subarray(148, 156));
    const actual = header.reduce((sum, byte, index) => sum + (index >= 148 && index < 156 ? 32 : byte), 0);
    if (actual !== checksum || ++count > 100000) throw Error('Invalid Cargo tar header');
    const size = octal(header.subarray(124, 136)), type = String.fromCharCode(header[156]);
    if (!Number.isSafeInteger(size) || size > tar.length - offset) throw Error('Truncated Cargo tar entry');
    const bytes = tar.subarray(offset, offset + size); offset += Math.ceil(size / 512) * 512;
    let name = text(header.subarray(0, 100));
    if (text(header.subarray(257, 263)) === 'ustar' && text(header.subarray(345, 500))) name = text(header.subarray(345, 500)) + '/' + name;
    if (type === 'L') { nextName = text(bytes); continue; }
    if (type === 'x') {
      // Cargo may use a PAX path for a long filename. Other extended metadata
      // has no bearing on the bytes consumed here.
      let cursor = 0;
      while (cursor < bytes.length) {
        const space = bytes.indexOf(32, cursor), length = Number(bytes.subarray(cursor, space).toString());
        if (space < cursor || !Number.isSafeInteger(length) || length <= space - cursor + 1 || cursor + length > bytes.length) throw Error('Invalid Cargo PAX record');
        const row = bytes.subarray(space + 1, cursor + length - 1).toString('utf8');
        if (row.startsWith('path=')) nextName = row.slice(5);
        cursor += length;
      }
      continue;
    }
    if (nextName !== null) { name = nextName; nextName = null; }
    if (type === '5') continue;
    if (type !== '0' && type !== '\0') throw Error('Linked or unsupported Cargo archive entry');
    if (!name || name.includes('\\') || name.startsWith('/') || name.split('/').some(part => !part || part === '.' || part === '..')) throw Error('Invalid Cargo archive path');
    visit(name, bytes);
  }
}
module.exports = { crateFiles };
