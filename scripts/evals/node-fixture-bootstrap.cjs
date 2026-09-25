// SPDX-License-Identifier: Apache-2.0
'use strict';
// This entire process, including this adapter, is untrusted to the parent.
// No assertion results or observations about internal state are authoritative.
const fs = require('node:fs');
const request = JSON.parse(fs.readFileSync(0, 'utf8'));
if (!request || Object.keys(request).sort().join(',') !== 'id,input' ||
    typeof request.id !== 'string' || !/^[a-f0-9]{32}$/.test(request.id)) throw Error('Invalid request');
const candidate = require('./candidate.cjs');
Promise.resolve(candidate.compute(request.input)).then(result => {
  process.stdout.write(JSON.stringify({ id: request.id, result }) + '\n');
}).catch(() => { process.exitCode = 1; });
