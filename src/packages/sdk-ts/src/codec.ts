// SPDX-License-Identifier: Apache-2.0
import { SdkError } from './errors.js';

export const ABSOLUTE_FRAME_BYTES = 16 * 1024 * 1024;
function maximum(value: number): number {
  if (!Number.isSafeInteger(value) || value < 1 || value > ABSOLUTE_FRAME_BYTES) {
    throw new SdkError('limit', 'Invalid frame byte limit');
  }
  return value;
}

/** Byte limit excludes LF, includes an optional CR. A failed decoder stays failed. */
export class LineDecoder {
  #maximum: number;
  #buffer = Buffer.alloc(0);
  #length = 0;
  #failed = false;
  #ended = false;
  constructor(maxBytes: number) { this.#maximum = maximum(maxBytes); }
  setMaximum(value: number): void {
    this.#check();
    this.#maximum = maximum(value);
    if (this.#length > this.#maximum) this.#fail('Frame exceeds negotiated limit');
    if (this.#buffer.length > this.#maximum) {
      this.#buffer = Buffer.from(this.#buffer.subarray(0, this.#length));
    }
  }
  #check(): void {
    if (this.#failed || this.#ended) throw new SdkError('frame', 'Decoder is closed');
  }
  #fail(message: string): never {
    this.#failed = true;
    this.#buffer = Buffer.alloc(0);
    this.#length = 0;
    throw new SdkError('frame', message);
  }
  #append(bytes: Uint8Array): void {
    const needed = this.#length + bytes.length;
    if (needed > this.#maximum) this.#fail('Frame exceeds byte limit');
    if (needed > this.#buffer.length) {
      const next = Buffer.allocUnsafe(Math.min(this.#maximum, Math.max(needed, this.#buffer.length * 2, 256)));
      this.#buffer.copy(next, 0, 0, this.#length);
      this.#buffer = next;
    }
    this.#buffer.set(bytes, this.#length);
    this.#length = needed;
  }
  push(bytes: Uint8Array): unknown[] {
    this.#check();
    const output: unknown[] = [];
    let start = 0;
    for (let i = 0; i < bytes.length; i++) {
      if (bytes[i] !== 10) {
        if (this.#length + i - start + 1 > this.#maximum) this.#fail('Frame exceeds byte limit');
        continue;
      }
      this.#append(bytes.subarray(start, i));
      let end = this.#length;
      if (end > 0 && this.#buffer[end - 1] === 13) end--;
      try {
        // ignoreBOM preserves BOM as a character, which JSON.parse rejects.
        const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(this.#buffer.subarray(0, end));
        output.push(JSON.parse(text));
      } catch { this.#fail('Invalid UTF-8 JSON frame'); }
      if (output.length > 4096) this.#fail('Too many frames in one read');
      this.#length = 0;
      start = i + 1;
    }
    this.#append(bytes.subarray(start));
    return output;
  }
  finish(): void {
    this.#check();
    if (this.#length !== 0) this.#fail('Incomplete frame at end of stream');
    this.#ended = true;
  }
}

/** Bounded writer; does not stringify/clone an entire arbitrary object first. */
export function encodeFrame(value: unknown, maxBytes: number): Buffer {
  const limit = maximum(maxBytes);
  const chunks: Buffer[] = [];
  let length = 0;
  let nodes = 0;
  const ancestors = new Set<object>();
  const append = (text: string): void => {
    const bytes = Buffer.byteLength(text);
    if (length + bytes > limit) throw new SdkError('limit', 'Frame exceeds byte limit');
    chunks.push(Buffer.from(text));
    length += bytes;
  };
  const string = (text: string): void => {
    if (text.length > limit - length) throw new SdkError('limit', 'Frame exceeds byte limit');
    for (const character of text) {
      const scalar = character.codePointAt(0)!;
      if (scalar >= 0xd800 && scalar <= 0xdfff) throw new SdkError('validation', 'Invalid Unicode scalar');
    }
    append(JSON.stringify(text));
  };
  const write = (item: unknown, depth: number): void => {
    if (++nodes > 100_000 || depth > 64) throw new SdkError('limit', 'JSON structure exceeds limit');
    if (item === null) { append('null'); return; }
    if (typeof item === 'string') { string(item); return; }
    if (typeof item === 'boolean') { append(item ? 'true' : 'false'); return; }
    if (typeof item === 'number' && Number.isFinite(item) && (!Number.isInteger(item) || Number.isSafeInteger(item))) { append(String(item)); return; }
    if (typeof item !== 'object') throw new SdkError('validation', 'Value is not a JSON value');
    if (ancestors.has(item)) throw new SdkError('validation', 'Cyclic JSON value');
    const array = Array.isArray(item);
    const proto = Object.getPrototypeOf(item);
    if (!array && proto !== Object.prototype && proto !== null) throw new SdkError('validation', 'JSON object must have a plain prototype');
    if (Object.getOwnPropertySymbols(item).length) throw new SdkError('validation', 'JSON symbol properties are unsupported');
    ancestors.add(item);
    append(array ? '[' : '{');
    const keys = Object.getOwnPropertyNames(item).filter(key => !array || key !== 'length');
    if (array && keys.length !== item.length) throw new SdkError('validation', 'Sparse or extended JSON array');
    keys.forEach((key, index) => {
      const descriptor = Object.getOwnPropertyDescriptor(item, key);
      if ((array && key !== String(index)) || !descriptor?.enumerable || !('value' in descriptor)) throw new SdkError('validation', 'JSON accessors or hidden properties are unsupported');
      if (index) append(',');
      if (!array) { string(key); append(':'); }
      write(descriptor.value, depth + 1);
    });
    append(array ? ']' : '}');
    ancestors.delete(item);
  };
  write(value, 0);
  chunks.push(Buffer.from('\n'));
  return Buffer.concat(chunks, length + 1);
}
