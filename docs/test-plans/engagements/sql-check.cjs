// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const { sqlAgreement } = require('./acceptance.cjs');
try { sqlAgreement(...process.argv.slice(2).map(file => JSON.parse(fs.readFileSync(file, 'utf8')))); }
catch (error) { console.error(error.message); process.exitCode = 1; }
