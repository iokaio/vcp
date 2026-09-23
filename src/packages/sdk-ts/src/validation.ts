// SPDX-License-Identifier: Apache-2.0
import bundled from '@vcp/protocol/schema.json' with { type: 'json' };
import { SdkError } from './errors.js';

type Schema = boolean | { [key: string]: unknown };
const root = bundled as unknown as Record<string, unknown>;
const definitions = root['definitions'] as Record<string, Schema>;
const allowed = new Set(['$comment', '$ref', '$schema', 'additionalProperties', 'allOf', 'anyOf', 'default', 'definitions', 'description', 'enum', 'format', 'items', 'maxItems', 'maxLength', 'maximum', 'minItems', 'minLength', 'minimum', 'oneOf', 'pattern', 'properties', 'required', 'title', 'type']);
const regexes = new WeakMap<object, RegExp>();
const own = (object: object, key: string): boolean => Object.hasOwn(object, key);
function invalidSchema(): never { throw new SdkError('schema', 'Bundled protocol schema is unsupported'); }
function resolve(reference: unknown): Schema {
  if (typeof reference !== 'string' || !/^#\/definitions\/[A-Za-z0-9_]+$/.test(reference)) return invalidSchema();
  const name = reference.slice('#/definitions/'.length);
  if (!own(definitions, name)) return invalidSchema();
  return definitions[name]!;
}
function inspect(schema: Schema): void {
  if (typeof schema === 'boolean') return;
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) return invalidSchema();
  for (const key of Object.keys(schema)) if (!allowed.has(key)) invalidSchema();
  if (own(schema, '$ref')) resolve(schema['$ref']);
  if (own(schema, 'format') && !['uint32', 'int16', 'int32'].includes(schema['format'] as string)) invalidSchema();
  if (own(schema, 'pattern')) {
    if (typeof schema['pattern'] !== 'string') invalidSchema();
    try { regexes.set(schema, new RegExp(schema['pattern'] as string, 'u')); } catch { invalidSchema(); }
  }
  if (own(schema, 'type')) {
    const types = Array.isArray(schema['type']) ? schema['type'] : [schema['type']];
    if (!types.length || types.some(type => !['null', 'boolean', 'object', 'array', 'number', 'integer', 'string'].includes(type as string))) invalidSchema();
  }
  for (const key of ['minimum', 'maximum', 'minItems', 'maxItems', 'minLength', 'maxLength']) {
    if (own(schema, key) && (typeof schema[key] !== 'number' || !Number.isFinite(schema[key]))) invalidSchema();
  }
  if (own(schema, 'required') && (!Array.isArray(schema['required']) || schema['required'].some(item => typeof item !== 'string'))) invalidSchema();
  if (own(schema, 'enum') && !Array.isArray(schema['enum'])) invalidSchema();
  for (const key of ['properties', 'definitions']) {
    if (!own(schema, key)) continue;
    const map = schema[key];
    if (!map || typeof map !== 'object' || Array.isArray(map)) invalidSchema();
    for (const child of Object.values(map)) inspect(child as Schema);
  }
  for (const key of ['oneOf', 'anyOf', 'allOf']) {
    if (!own(schema, key)) continue;
    const children = schema[key];
    if (!Array.isArray(children) || !children.length) invalidSchema();
    for (const child of children) inspect(child as Schema);
  }
  for (const key of ['items', 'additionalProperties']) if (own(schema, key)) inspect(schema[key] as Schema);
}
// Fail at module load on assertion drift, before any client accepts unvalidated data.
inspect(root);

/** Schema drift check used by package qualification; never installs a peer schema. */
export function assertSupportedSchema(schema: unknown): void { inspect(schema as Schema); }

function jsonShape(value: unknown): void {
  let nodes = 0;
  let bytes = 0;
  const ancestors = new Set<object>();
  const walk = (item: unknown, depth: number): void => {
    if (++nodes > 100_000 || depth > 64 || bytes > 16 * 1024 * 1024) throw new SdkError('limit', 'JSON validation budget exceeded');
    if (item === null || typeof item === 'boolean') return;
    if (typeof item === 'string') {
      if (item.length > 16 * 1024 * 1024 - bytes) throw new SdkError('limit', 'JSON validation budget exceeded');
      for (const character of item) {
        const scalar = character.codePointAt(0)!;
        if (scalar >= 0xd800 && scalar <= 0xdfff) throw new SdkError('validation', 'Invalid Unicode scalar');
      }
      bytes += Buffer.byteLength(item);
      return;
    }
    if (typeof item === 'number') {
      if (!Number.isFinite(item) || (Number.isInteger(item) && !Number.isSafeInteger(item))) throw new SdkError('validation', 'Unsafe JSON number');
      return;
    }
    if (typeof item !== 'object' || ancestors.has(item)) throw new SdkError('validation', 'Invalid JSON value');
    const array = Array.isArray(item);
    const prototype = Object.getPrototypeOf(item);
    if ((!array && prototype !== Object.prototype && prototype !== null) || Object.getOwnPropertySymbols(item).length) throw new SdkError('validation', 'Invalid JSON object');
    ancestors.add(item);
    const keys = Object.getOwnPropertyNames(item).filter(key => !array || key !== 'length');
    if (array && keys.length !== item.length) throw new SdkError('validation', 'Invalid JSON array');
    for (let index = 0; index < keys.length; index++) {
      const key = keys[index]!;
      const descriptor = Object.getOwnPropertyDescriptor(item, key)!;
      if ((array && key !== String(index)) || !descriptor.enumerable || !('value' in descriptor)) throw new SdkError('validation', 'JSON accessors or hidden properties are unsupported');
      bytes += Buffer.byteLength(key);
      walk(descriptor.value, depth + 1);
    }
    ancestors.delete(item);
  };
  walk(value, 0);
  if (bytes > 16 * 1024 * 1024) throw new SdkError('limit', 'JSON validation budget exceeded');
}

function equal(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (!left || !right || typeof left !== 'object' || typeof right !== 'object' || Array.isArray(left) !== Array.isArray(right)) return false;
  const a = left as Record<string, unknown>;
  const b = right as Record<string, unknown>;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every(key => own(b, key) && equal(a[key], b[key]));
}

/** Validate one canonical definition. Errors never include the rejected value. */
export function validateWire(definition: string, value: unknown): void {
  const schema = definition === 'PublicApi' ? root : own(definitions, definition) ? definitions[definition] : undefined;
  if (schema === undefined) throw new SdkError('schema', 'Unknown protocol definition');
  jsonShape(value);
  let evaluations = 0;
  const matches = (current: Schema, item: unknown, depth: number): boolean => {
    if (++evaluations > 1_000_000 || depth > 128) throw new SdkError('limit', 'Schema validation budget exceeded');
    if (typeof current === 'boolean') return current;
    // draft-07: siblings of $ref are ignored.
    if (own(current, '$ref')) return matches(resolve(current['$ref']), item, depth + 1);
    const type = (name: unknown): boolean => {
      switch (name) {
        case 'null': return item === null;
        case 'boolean': return typeof item === 'boolean';
        case 'string': return typeof item === 'string';
        case 'number': return typeof item === 'number' && Number.isFinite(item);
        case 'integer': return typeof item === 'number' && Number.isSafeInteger(item);
        case 'array': return Array.isArray(item);
        case 'object': return item !== null && typeof item === 'object' && !Array.isArray(item);
        default: return false;
      }
    };
    if (own(current, 'type') && !(Array.isArray(current['type']) ? current['type'].some(type) : type(current['type']))) return false;
    if (Array.isArray(current['enum']) && !current['enum'].some(variant => equal(variant, item))) return false;
    if (typeof item === 'number') {
      if (typeof current['minimum'] === 'number' && item < current['minimum']) return false;
      if (typeof current['maximum'] === 'number' && item > current['maximum']) return false;
      const format = current['format'];
      if (format === 'uint32' && (!Number.isInteger(item) || item < 0 || item > 4294967295)) return false;
      if (format === 'int32' && (!Number.isInteger(item) || item < -2147483648 || item > 2147483647)) return false;
      if (format === 'int16' && (!Number.isInteger(item) || item < -32768 || item > 32767)) return false;
    }
    if (typeof item === 'string') {
      let length = 0;
      for (const _ of item) length++;
      if (typeof current['minLength'] === 'number' && length < current['minLength']) return false;
      if (typeof current['maxLength'] === 'number' && length > current['maxLength']) return false;
      const pattern = regexes.get(current);
      if (pattern && !pattern.test(item)) return false;
    }
    if (Array.isArray(item)) {
      if (typeof current['minItems'] === 'number' && item.length < current['minItems']) return false;
      if (typeof current['maxItems'] === 'number' && item.length > current['maxItems']) return false;
      if (own(current, 'items') && !item.every(child => matches(current['items'] as Schema, child, depth + 1))) return false;
    } else if (item !== null && typeof item === 'object') {
      const object = item as Record<string, unknown>;
      if (Array.isArray(current['required']) && !current['required'].every(key => own(object, key as string))) return false;
      const properties = (current['properties'] ?? {}) as Record<string, Schema>;
      for (const key of Object.keys(object)) {
        if (own(properties, key)) {
          if (!matches(properties[key]!, object[key], depth + 1)) return false;
        } else if (own(current, 'additionalProperties') && !matches(current['additionalProperties'] as Schema, object[key], depth + 1)) return false;
      }
    }
    if (Array.isArray(current['allOf']) && !current['allOf'].every(child => matches(child as Schema, item, depth + 1))) return false;
    if (Array.isArray(current['anyOf']) && !current['anyOf'].some(child => matches(child as Schema, item, depth + 1))) return false;
    if (Array.isArray(current['oneOf'])) {
      let count = 0;
      for (const child of current['oneOf']) if (matches(child as Schema, item, depth + 1)) count++;
      if (count !== 1) return false;
    }
    return true;
  };
  if (!matches(schema, value, 0)) throw new SdkError('validation', 'Value does not match protocol schema');
}
