// SPDX-License-Identifier: Apache-2.0
'use strict';
// Dependency-free translation of the selected schemars draft-07 wire schema.
const fs = require('node:fs');
const path = require('node:path');

const MAX_SCHEMA_BYTES = 8 * 1024 * 1024;
const MAX_DEPTH = 100;
const OUTPUT = path.resolve(__dirname, '../../src/packages/protocol-ts');
const RESERVED = new Set(('any as asserts async await bigint boolean break case catch class const constructor continue debugger declare default delete do else enum export extends false finally for from function get if implements import in infer instanceof interface intrinsic is keyof let module namespace never new null number object of package private protected public readonly require return set static string super switch symbol this throw true try type typeof undefined unique unknown var void while with yield').split(' '));
const ANNOTATIONS = new Set(['title', 'description', 'default', 'examples', 'deprecated', 'readOnly', 'writeOnly', '$comment']);
// These remain enforced by the JSON Schema/runtime. TypeScript cannot express
// arbitrary regexes, numeric bounds, lengths, or array uniqueness.
const CONSTRAINTS = new Set(['format', 'pattern', 'minLength', 'maxLength', 'minimum', 'maximum', 'exclusiveMinimum', 'exclusiveMaximum', 'multipleOf', 'minItems', 'maxItems', 'uniqueItems', 'minProperties', 'maxProperties']);
const STRUCTURAL = new Set(['$schema', '$id', '$ref', 'definitions', 'type', 'enum', 'const', 'properties', 'required', 'additionalProperties', 'items', 'oneOf', 'anyOf', 'allOf']);

function fail(at, message) { throw Error(`${at}: ${message}`); }
function object(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function name(value, at) {
  if (typeof value !== 'string' || !/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(value) || RESERVED.has(value)) fail(at, 'unsafe TypeScript type name');
  return value;
}
function stable(value, depth = 0) {
  if (depth > MAX_DEPTH) fail('schema', 'nesting limit exceeded');
  if (Array.isArray(value)) return value.map(item => stable(item, depth + 1));
  if (!object(value)) return value;
  const result = Object.create(null);
  for (const key of Object.keys(value).sort()) result[key] = stable(value[key], depth + 1);
  return result;
}
function literal(value, at) {
  if (value === null || typeof value === 'boolean' || typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'number' && Number.isFinite(value) && (!Number.isInteger(value) || Number.isSafeInteger(value))) return String(value);
  fail(at, 'only exact scalar enum/const literals are supported');
}
function join(parts, separator) { return parts.length === 1 ? parts[0] : `(${parts.join(separator)})`; }

function compile(schema) {
  if (!object(schema)) fail('schema', 'root must be a schema object');
  if (schema.$schema !== 'http://json-schema.org/draft-07/schema#') fail('schema', 'expected schemars JSON Schema draft-07');
  const rootName = name(schema.title, 'schema.title');
  const definitions = schema.definitions ?? {};
  if (!object(definitions)) fail('schema.definitions', 'expected object');
  for (const key of Object.keys(definitions)) name(key, `definitions.${key}`);
  if (Object.hasOwn(definitions, rootName)) fail('schema.title', 'root type collides with a definition');

  function type(node, at, depth = 0) {
    if (depth > MAX_DEPTH) fail(at, 'nesting limit exceeded');
    if (node === true) return 'unknown';
    if (node === false) return 'never';
    if (!object(node)) fail(at, 'expected schema object or boolean');
    for (const key of Object.keys(node)) {
      if (!ANNOTATIONS.has(key) && !CONSTRAINTS.has(key) && !STRUCTURAL.has(key)) fail(at, `unsupported schema keyword ${key}`);
      if (['$schema', '$id', 'definitions'].includes(key) && at !== 'schema') fail(at, `nested ${key} is unsupported`);
    }
    if (Object.hasOwn(node, '$ref')) {
      const ref = node.$ref;
      if (typeof ref !== 'string' || !ref.startsWith('#/definitions/')) fail(at, 'only local definition references are supported');
      const target = ref.slice('#/definitions/'.length);
      name(target, at);
      if (!Object.hasOwn(definitions, target)) fail(at, `unresolved reference ${ref}`);
      if (Object.keys(node).some(key => key !== '$ref' && !ANNOTATIONS.has(key))) fail(at, 'draft-07 reference siblings with validation semantics are unsupported');
      return target;
    }
    const parts = [];
    for (const key of ['oneOf', 'anyOf', 'allOf']) {
      if (!Object.hasOwn(node, key)) continue;
      if (!Array.isArray(node[key]) || !node[key].length) fail(at, `${key} must be nonempty`);
      parts.push(join(node[key].map((child, i) => type(child, `${at}.${key}[${i}]`, depth + 1)), key === 'allOf' ? ' & ' : ' | '));
    }
    if (Object.hasOwn(node, 'enum')) {
      if (!Array.isArray(node.enum) || !node.enum.length) fail(at, 'enum must be nonempty');
      parts.push(join([...new Set(node.enum.map(value => literal(value, at)))], ' | '));
    }
    if (Object.hasOwn(node, 'const')) parts.push(literal(node.const, at));
    if (Array.isArray(node.type)) {
      if (!node.type.length || new Set(node.type).size !== node.type.length) fail(at, 'type list must be nonempty and unique');
      const basic = { ...node };
      for (const key of ['oneOf', 'anyOf', 'allOf', 'enum', 'const']) delete basic[key];
      parts.push(join(node.type.map(value => {
        const branch = { ...basic, type: value };
        // JSON Schema applies these keywords only to their matching instance
        // type, including nullable objects and arrays emitted by schemars.
        if (value !== 'object') for (const key of ['properties', 'required', 'additionalProperties']) delete branch[key];
        if (value !== 'array') delete branch.items;
        return type(branch, at, depth + 1);
      }), ' | '));
    } else if (Object.hasOwn(node, 'type')) {
      switch (node.type) {
        case 'string': parts.push('string'); break;
        case 'number': case 'integer': parts.push('number'); break;
        case 'boolean': parts.push('boolean'); break;
        case 'null': parts.push('null'); break;
        case 'array': {
          if (Array.isArray(node.items)) fail(at, 'tuple schemas are unsupported');
          parts.push(`Array<${type(node.items ?? true, `${at}.items`, depth + 1)}>`);
          break;
        }
        case 'object': {
          const properties = node.properties ?? {};
          if (!object(properties)) fail(at, 'properties must be an object');
          const required = node.required ?? [];
          if (!Array.isArray(required) || required.some(key => typeof key !== 'string') || new Set(required).size !== required.length) fail(at, 'required must contain unique property names');
          if (required.some(key => !Object.hasOwn(properties, key))) fail(at, 'required properties must have explicit schemas');
          const fields = Object.keys(properties).sort().map(key => `${JSON.stringify(key)}${required.includes(key) ? '' : '?'}: ${type(properties[key], `${at}.properties.${key}`, depth + 1)};`);
          const additional = node.additionalProperties ?? true;
          if (additional !== false) {
            const extra = type(additional, `${at}.additionalProperties`, depth + 1);
            // A typed dictionary with named exceptions cannot be represented
            // faithfully by a TypeScript index signature.
            if (fields.length && extra !== 'unknown') fail(at, 'typed additionalProperties with named properties is unsupported');
            fields.push(`[key: string]: ${extra};`);
          }
          parts.push(fields.length ? `{ ${fields.join(' ')} }` : 'Record<string, never>');
          break;
        }
        default: fail(at, `unsupported type ${JSON.stringify(node.type)}`);
      }
    } else if (['properties', 'required', 'additionalProperties', 'items'].some(key => Object.hasOwn(node, key))) {
      fail(at, 'structural validation requires an explicit object or array type');
    }
    // Reject misspelled/misplaced structure rather than silently discard it.
    if (node.type !== undefined && !Array.isArray(node.type)) {
      if (node.type !== 'object' && ['properties', 'required', 'additionalProperties'].some(key => Object.hasOwn(node, key))) fail(at, 'object keywords on non-object type');
      if (node.type !== 'array' && Object.hasOwn(node, 'items')) fail(at, 'items on non-array type');
    }
    return parts.length ? join(parts, ' & ') : 'unknown';
  }

  const lines = [
    '// SPDX-License-Identifier: Apache-2.0',
    '// Generated by scripts/protocol/generate-types.cjs; do not edit.',
    '// Rust wire definitions and schema.json are canonical. These types do not',
    '// validate runtime data, exclusive oneOf branches, extra properties, integer',
    '// ranges, patterns, or other JSON Schema value constraints.',
    '',
    `export type ${rootName} = ${type(schema, 'schema')};`,
  ];
  for (const key of Object.keys(definitions).sort()) lines.push('', `export type ${key} = ${type(definitions[key], `definitions.${key}`)};`);
  const pkg = {
    name: '@vcp/protocol', version: '0.1.0', private: true,
    description: 'Generated VCP public wire types and JSON Schema; no runtime client.',
    license: 'Apache-2.0', types: './index.ts',
    exports: { '.': { types: './index.ts' }, './schema.json': './schema.json' },
  };
  return {
    'schema.json': JSON.stringify(stable(schema), null, 2) + '\n',
    'index.ts': lines.join('\n') + '\n',
    'package.json': JSON.stringify(pkg, null, 2) + '\n',
  };
}

function generate(schema, directory = OUTPUT, check = false) {
  const files = compile(schema); // Validate everything before writing anything.
  if (check) {
    const drift = Object.entries(files).filter(([name, contents]) => {
      try { return fs.readFileSync(path.join(directory, name), 'utf8') !== contents; }
      catch (error) { if (error.code === 'ENOENT') return true; throw error; }
    }).map(([name]) => name);
    if (drift.length) throw Error(`generated protocol drift: ${drift.join(', ')}`);
  } else {
    fs.mkdirSync(directory, { recursive: true });
    for (const [name, contents] of Object.entries(files)) fs.writeFileSync(path.join(directory, name), contents);
  }
  return Object.keys(files);
}

function main(args) {
  const check = args.includes('--check');
  const paths = args.filter(arg => arg !== '--check');
  if (paths.length !== 1 || args.length !== (check ? 2 : 1) || paths[0].startsWith('--')) throw Error('Usage: node scripts/protocol/generate-types.cjs <schema.json> [--check]');
  const fd = fs.openSync(paths[0], 'r');
  let input;
  try {
    const stat = fs.fstatSync(fd);
    if (!stat.isFile() || stat.size > MAX_SCHEMA_BYTES) throw Error('schema must be a regular file within the 8 MiB limit');
    const bytes = Buffer.alloc(stat.size + 1);
    let used = 0;
    while (used < bytes.length) {
      const count = fs.readSync(fd, bytes, used, bytes.length - used, null);
      if (!count) break;
      used += count;
    }
    if (used > stat.size) throw Error('schema changed while being read');
    input = bytes.subarray(0, used);
  } finally { fs.closeSync(fd); }
  const decoder = new TextDecoder('utf-8', { fatal: true });
  generate(JSON.parse(decoder.decode(input)), OUTPUT, check);
}

module.exports = { compile, generate, main };
if (require.main === module) {
  try { main(process.argv.slice(2)); }
  catch (error) { process.stderr.write(`protocol generation failed: ${error.message}\n`); process.exitCode = 1; }
}
