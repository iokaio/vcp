const test=require('node:test'),assert=require('node:assert/strict');test('amount',()=>assert.equal(require('./amount.cjs').parseAmount('12'),12));
