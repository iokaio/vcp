// SPDX-License-Identifier: Apache-2.0
'use strict';
const test=require('node:test'), assert=require('node:assert/strict'), {normalize}=require('./normalize.cjs');
test('normalizes ASCII labels without changing internal space',()=>{ assert.equal(normalize(' READY '),'ready'); assert.equal(normalize('A  B'),'a  b'); });
