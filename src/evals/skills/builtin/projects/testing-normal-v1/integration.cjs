const test=require('node:test'),assert=require('node:assert/strict');test('receipt',()=>assert.equal('USD '+require('./amount.cjs').parseAmount('12'),'USD 12'));
