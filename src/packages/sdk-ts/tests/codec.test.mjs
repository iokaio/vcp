// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { LineDecoder, encodeFrame } from '../dist/codec.js';

test('every byte split preserves UTF-8, CRLF and escaped newlines', () => {
  const value = { text: 'é雪😀\nnext' };
  const frame = Buffer.from(`${JSON.stringify(value)}\r\n`);
  for (let split = 0; split <= frame.length; split++) {
    const decoder = new LineDecoder(frame.length - 1);
    assert.deepEqual([...decoder.push(frame.subarray(0, split)), ...decoder.push(frame.subarray(split))], [value]);
    decoder.finish();
  }
  const decoder = new LineDecoder(128);
  const received = [];
  for (const byte of frame) received.push(...decoder.push(Uint8Array.of(byte)));
  assert.deepEqual(received, [value]);
});

test('malformed UTF-8, malformed JSON, oversized and truncated frames poison decoder', () => {
  for (const bytes of [Buffer.from([34, 0xc0, 0xaf, 34, 10]), Buffer.from('{\n'), Buffer.from('123456789\n'), Buffer.from([0xef, 0xbb, 0xbf, 49, 10])]) {
    const decoder = new LineDecoder(8);
    assert.throws(() => decoder.push(bytes), /frame|limit/i);
    assert.throws(() => decoder.push(Buffer.from('1\n')), /closed/);
  }
  const partial = new LineDecoder(8);
  partial.push(Buffer.from('1'));
  assert.throws(() => partial.finish(), /Incomplete/);
  assert.throws(() => partial.finish(), /closed/);
  const decoder = new LineDecoder(8);
  decoder.push(Buffer.from('12345'));
  assert.throws(() => decoder.setMaximum(4), /limit/);
  assert.throws(() => decoder.push(Buffer.from('\n')), /closed/);
});

test('frame accounting excludes LF and includes CR and multibyte bytes', () => {
  assert.deepEqual(new LineDecoder(1).push(Buffer.from('1\n')), [1]);
  assert.throws(() => new LineDecoder(1).push(Buffer.from('1\r\n')));
  assert.equal(encodeFrame('é', 4).length, 5);
  assert.throws(() => encodeFrame('é', 3), /limit/);
  const decoder = new LineDecoder(20);
  assert.deepEqual(decoder.push(Buffer.from('1\nnull\n{}\n')), [1, null, {}]);
  decoder.finish();
  assert.throws(() => decoder.push(Buffer.alloc(0)), /closed/);
});

test('bounded encoding rejects cycles, accessors, hidden values and unsafe numbers', () => {
  const cycle = {}; cycle.self = cycle;
  const getter = Object.defineProperty({}, 'secret', { enumerable: true, get() { throw Error('must not execute'); } });
  const holeWithProperty = new Array(1); holeWithProperty.extra = 1;
  for (const value of [cycle, getter, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, undefined, 1n, holeWithProperty, '\ud800', new Date(), Object.defineProperty({}, 'hidden', { value: 1 })]) {
    assert.throws(() => encodeFrame(value, 1024), error => error.name === 'SdkError' && !error.message.includes('must not execute'));
  }
  let invoked = false;
  const tooLarge = { first: 'x'.repeat(1024), get last() { invoked = true; return 1; } };
  assert.throws(() => encodeFrame(tooLarge, 32), /limit/);
  assert.equal(invoked, false);
  const safe = JSON.parse('{"__proto__":{"polluted":true},"counter":"18446744073709551615"}');
  assert.deepEqual(new LineDecoder(1024).push(encodeFrame(safe, 1024)), [safe]);
  assert.equal({}.polluted, undefined);
});
