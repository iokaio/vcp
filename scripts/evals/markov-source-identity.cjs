// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const { sourceIdentity } = require('./memory-source-identity.cjs');
const identity = sourceIdentity(path.resolve(__dirname, '../..'), ['src/evals/markov']);
identity.schema = 'p6-markov-bounded-source-identity/1';
process.stdout.write(JSON.stringify(identity) + '\n');
