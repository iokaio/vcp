// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const yauzl = require('yauzl');
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
exports.sha256 = sha256;
exports.inventory = filename => new Promise((resolve, reject) => {
  yauzl.open(filename, { lazyEntries: true, strictFileNames: true }, (error, zip) => {
    if (error) return reject(error);
    const rows = [], seen = new Set();
    let total = 0;
    const fail = error => { zip.close(); reject(error); };
    zip.on('error', fail);
    zip.on('end', () => resolve(rows.sort((a, b) => a.path.localeCompare(b.path))));
    zip.on('entry', entry => {
      const name = entry.fileName;
      if (seen.has(name.toLowerCase()) || name.includes('\\') || name.split('/').some(x => !x || x === '.' || x === '..') || name.startsWith('/') || name.includes(':') || ((entry.externalFileAttributes >>> 16) & 0xf000) === 0xa000 || ++total > 256 || entry.uncompressedSize > 8 * 1024 * 1024) return fail(new Error('Invalid archive inventory'));
      seen.add(name.toLowerCase());
      zip.openReadStream(entry, (error, stream) => {
        if (error) return fail(error);
        const chunks = []; let length = 0;
        stream.on('error', fail);
        stream.on('data', bytes => { length += bytes.length; if (length > 8 * 1024 * 1024) { stream.destroy(); fail(new Error('Archive entry exceeds limit')); } else chunks.push(bytes); });
        stream.on('end', () => { rows.push({ path: name, bytes: length, sha256: sha256(Buffer.concat(chunks)) }); zip.readEntry(); });
      });
    });
    zip.readEntry();
  });
});
exports.fileHash = filename => new Promise((resolve, reject) => {
  const hash = crypto.createHash('sha256');
  const stream = fs.createReadStream(filename);
  stream.on('error', reject); stream.on('data', bytes => hash.update(bytes));
  stream.on('end', () => resolve(hash.digest('hex')));
});
