// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const generator = require('../../../scripts/protocol/generate-types.cjs');

const repository = path.resolve(__dirname, '../../..');
const committedSchema = path.join(repository, 'src/packages/protocol-ts/schema.json');
const nativeRegenerate = 'Native protocol schema regeneration is required; rebuild vcp-protocol-schema with --features schema, then run scripts/protocol/generate-types.cjs on its output.';

const schema = () => ({
  $schema: 'http://json-schema.org/draft-07/schema#', title: 'PublicApi',
  type: 'object', additionalProperties: false, required: ['call'],
  properties: { call: { $ref: '#/definitions/Call' }, 'optional-name': { type: ['string', 'null'] } },
  definitions: {
    Counter: { type: 'string', pattern: '^(0|[1-9][0-9]*)$' },
    Call: { oneOf: [
      { type: 'object', additionalProperties: false, required: ['method', 'cursor'], properties: { method: { const: 'read' }, cursor: { $ref: '#/definitions/Counter' } } },
      { type: 'object', additionalProperties: false, required: ['method', 'values'], properties: { method: { enum: ['write'] }, values: { type: 'array', items: { type: 'integer' } } } },
    ] },
    Dictionary: { type: 'object', additionalProperties: { type: 'boolean' } },
    Extended: { allOf: [{ $ref: '#/definitions/Counter' }, { anyOf: [{ const: '0' }, { const: '1' }] }] },
  },
});

test('draft-07 tagged unions, dictionaries, refs and intersections emit deterministic standalone types', () => {
  const input = schema(), files = generator.compile(input);
  assert.match(files['index.ts'], /export type PublicApi =/);
  assert.match(files['index.ts'], /"optional-name"\?: \(string \| null\)/);
  assert.match(files['index.ts'], /export type Dictionary = \{ \[key: string\]: boolean; \};/);
  assert.match(files['index.ts'], /export type Extended = \(Counter & \("0" \| "1"\)\);/);
  assert.match(files['index.ts'], /Array<number>/);
  assert.doesNotMatch(files['index.ts'], /\bany\b/);
  assert.deepEqual(JSON.parse(files['schema.json']), input);
  const reversed = JSON.parse(JSON.stringify(input));
  reversed.definitions = Object.fromEntries(Object.entries(reversed.definitions).reverse());
  reversed.properties = Object.fromEntries(Object.entries(reversed.properties).reverse());
  assert.deepEqual(generator.compile(reversed), files);
  assert.equal(JSON.parse(files['package.json']).exports['./schema.json'], './schema.json');
});

test('unknown semantics, malicious names, foreign refs and unsound dictionary exceptions fail closed', () => {
  const mutations = [
    s => { s.definitions.Counter.not = { type: 'number' }; },
    s => { s.definitions['Bad; export type Injected'] = { type: 'string' }; },
    s => { s.title = 'class'; },
    s => { s.properties.call.$ref = 'https://example.invalid/schema'; },
    s => { s.properties.call.$ref = '#/definitions/Missing'; },
    s => { s.properties.call.type = 'string'; },
    s => { s.definitions.Dictionary.properties = { fixed: { type: 'string' } }; },
    s => { s.definitions.Counter.type = 'strnig'; },
    s => { s.definitions.Counter.enum = [9007199254740992]; },
    s => { s.required = ['untyped']; },
    s => { s.definitions.Tuple = { type: 'array', items: [{ type: 'string' }] }; },
    s => { s.$schema = 'https://json-schema.org/draft/2020-12/schema'; },
    s => { s.definitions.Counter.if = { type: 'number' }; },
  ];
  for (const mutate of mutations) { const input = schema(); mutate(input); assert.throws(() => generator.compile(input)); }
});

test('explicit unrestricted and impossible schemas translate to unknown and never without invented precision', () => {
  const input = schema(); input.definitions.Open = true; input.definitions.Closed = false;
  input.definitions.Empty = { type: 'object', additionalProperties: false };
  input.definitions.Recursive = { type: 'array', items: { $ref: '#/definitions/Recursive' } };
  input.definitions.NullableArray = { type: ['array', 'null'], items: { type: 'string' } };
  input.definitions.NullableObject = { type: ['object', 'null'], properties: { id: { type: 'string' } }, additionalProperties: false };
  const text = generator.compile(input)['index.ts'];
  assert.match(text, /export type Open = unknown;/);
  assert.match(text, /export type Closed = never;/);
  assert.match(text, /export type Empty = Record<string, never>;/);
  assert.match(text, /export type Recursive = Array<Recursive>;/);
  assert.match(text, /export type NullableArray = \(Array<string> \| null\);/);
  assert.match(text, /export type NullableObject = \(\{ "id"\?: string; \} \| null\);/);
});

test('drift checks are read-only and generation validates before replacing existing artifacts', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-protocol-generation-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  generator.generate(schema(), directory);
  generator.generate(schema(), directory, true);
  const types = path.join(directory, 'index.ts');
  fs.appendFileSync(types, '// changed\n');
  const changed = fs.readFileSync(types, 'utf8');
  assert.throws(() => generator.generate(schema(), directory, true), /drift: index.ts/);
  assert.equal(fs.readFileSync(types, 'utf8'), changed);
  const invalid = schema(); invalid.definitions.Counter.not = {};
  assert.throws(() => generator.generate(invalid, directory));
  assert.equal(fs.readFileSync(types, 'utf8'), changed);
  generator.generate(schema(), directory);
  fs.unlinkSync(path.join(directory, 'schema.json'));
  assert.throws(() => generator.generate(schema(), directory, true), /drift: schema.json/);
  assert.equal(fs.existsSync(path.join(directory, 'schema.json')), false);
});

test('committed schema is bound to compiled canonical sources and all generated outputs are current', () => {
  const input = JSON.parse(fs.readFileSync(committedSchema, 'utf8'));
  assert.equal(typeof input.$comment, 'string', nativeRegenerate);
  let provenance;
  try { provenance = JSON.parse(input.$comment); }
  catch { assert.fail(nativeRegenerate); }
  assert.equal(provenance.format, 'vcp-schema-sources/1', nativeRegenerate);
  assert.deepEqual(Object.keys(provenance).sort(), ['format', 'sources'], nativeRegenerate);
  assert.ok(provenance.sources && typeof provenance.sources === 'object' && !Array.isArray(provenance.sources), nativeRegenerate);
  const expected = [
    'src/crates/vcp-protocol/Cargo.toml',
    'src/crates/vcp-protocol/src/bin/vcp-protocol-schema.rs',
    'src/crates/vcp-protocol/src/editor.rs',
    'src/crates/vcp-protocol/src/errors.rs',
    'src/crates/vcp-protocol/src/handshake.rs',
    'src/crates/vcp-protocol/src/jsonrpc.rs',
    'src/crates/vcp-protocol/src/lib.rs',
    'src/crates/vcp-protocol/src/memory.rs',
    'src/crates/vcp-protocol/src/memory_governance.rs',
    'src/crates/vcp-protocol/src/memory_query.rs',
    'src/crates/vcp-protocol/src/memory_retention.rs',
    'src/crates/vcp-protocol/src/methods.rs',
    'src/third_party/codex/codex-rs/Cargo.lock',
  ];
  assert.deepEqual(Object.keys(provenance.sources).sort(), expected, nativeRegenerate);
  for (const source of expected) {
    const normalized = fs.readFileSync(path.join(repository, source), 'utf8').replaceAll('\r\n', '\n');
    const digest = crypto.createHash('sha256').update(normalized, 'utf8').digest('hex');
    assert.equal(provenance.sources[source], digest, `${source}: ${nativeRegenerate}`);
  }
  const checked = spawnSync(process.execPath, [path.join(repository, 'scripts/protocol/generate-types.cjs'), committedSchema, '--check'], {
    cwd: repository, encoding: 'utf8', windowsHide: true, timeout: 10000, maxBuffer: 2 * 1024 * 1024,
  });
  assert.equal(checked.error, undefined, nativeRegenerate);
  assert.equal(checked.status, 0, `${nativeRegenerate}\n${checked.stderr}`);
});

test('committed Counter schema accepts exactly canonical unsigned 64-bit decimal boundary fixtures', () => {
  const counter = JSON.parse(fs.readFileSync(committedSchema, 'utf8')).definitions.Counter;
  assert.equal(counter.type, 'string');
  assert.equal(counter.minLength, 1);
  assert.equal(counter.maxLength, 20);
  const pattern = new RegExp(counter.pattern, 'u');
  const maximum = 18446744073709551615n;
  // This oracle uses BigInt arithmetic and a decimal grammar independently of
  // the schema generator's lexicographic regular-expression construction.
  const expected = text => typeof text === 'string'
    && /^(0|[1-9][0-9]*)(?![\s\S])/u.test(text)
    && BigInt(text) <= maximum;
  const accepts = text => typeof text === 'string' && text.length >= counter.minLength
    && text.length <= counter.maxLength && pattern.test(text);
  const candidates = [
    '', '00', '01', '+1', '-1', '-0', ' 1', '1 ', '1.0', '1e0', 'NaN',
    '1\n', '1\r', '1\r\n', '1\u2028', '1\u2029', '1\u0000', '\n1', '１',
    '9007199254740991', '9007199254740992', '9007199254740993',
    maximum.toString(), (maximum - 1n).toString(), (maximum + 1n).toString(),
    '99999999999999999999', '100000000000000000000', 0, 1, null,
  ];
  for (let digits = 0n; digits <= 20n; digits++) {
    const power = 10n ** digits;
    for (const delta of [-1n, 0n, 1n]) candidates.push((power + delta).toString());
    // Probe every decimal-prefix cutoff in the maximum, not just its final digit.
    const truncated = maximum / power * power;
    for (const delta of [-1n, 0n, 1n]) candidates.push((truncated + delta).toString());
  }
  let random = 0x76543210abcdefn;
  const mask = (1n << 65n) - 1n;
  for (let i = 0; i < 512; i++) {
    random = (random * 6364136223846793005n + 1442695040888963407n) & mask;
    candidates.push(random.toString(), (random & maximum).toString());
  }
  for (const candidate of candidates) assert.equal(accepts(candidate), expected(candidate), `Counter ${JSON.stringify(candidate)}`);
});

test('committed page and artifact-range schemas preserve numerical admission limits', () => {
  const definitions = JSON.parse(fs.readFileSync(committedSchema, 'utf8')).definitions;
  const cases = [
    ['SessionList', 'limit', 128], ['EventsSubscribe', 'limit', 128],
    ['Inspect', 'limit', 128], ['MemoryQuery', 'limit', 128],
    ['ArtifactRead', 'length', 65536], ['DiffRead', 'length', 65536],
  ];
  for (const [name, field, maximum] of cases) {
    const definition = definitions[name], constraint = definition.properties[field];
    assert.equal(constraint.type, 'integer', `${name}.${field}`);
    assert.equal(constraint.minimum, 1, `${name}.${field}`);
    assert.equal(constraint.maximum, maximum, `${name}.${field}`);
    assert.ok(definition.required.includes(field), `${name}.${field} must be present`);
    const accepts = value => Number.isInteger(value) && value >= constraint.minimum && value <= constraint.maximum;
    for (const value of [-1, 0, 0.5, maximum + 1, maximum + 0.5]) assert.equal(accepts(value), false);
    for (const value of [1, maximum - 1, maximum]) assert.equal(accepts(value), true);
  }
});

test('committed opaque IDs reject trailing line terminators and non-ASCII characters', () => {
  const id = JSON.parse(fs.readFileSync(committedSchema, 'utf8')).definitions.Id;
  assert.equal(id.type, 'string');
  assert.equal(id.minLength, 1);
  assert.equal(id.maxLength, 96);
  const pattern = new RegExp(id.pattern, 'u');
  const accepts = value => typeof value === 'string' && value.length >= id.minLength
    && value.length <= id.maxLength && pattern.test(value);
  for (const value of ['a', 'task_1-ABC', 'x'.repeat(96)]) assert.equal(accepts(value), true);
  for (const value of ['', 'x'.repeat(97), 'x\n', 'x\r', 'x\r\n', 'x\u2028', 'x\u2029', '\nx', 'x\u0000', 'é', '１', 'x y', 'x.y', 'x/y']) {
    assert.equal(accepts(value), false, `Id ${JSON.stringify(value)}`);
  }
});

test('optional controller schemas expose scoped revisions without wire credentials', () => {
  const definitions = JSON.parse(fs.readFileSync(committedSchema, 'utf8')).definitions;
  const calls = definitions.Call.oneOf.map(branch => branch.properties.method.enum[0]);
  for (const method of ['controller/read', 'controller/acquire', 'controller/release', 'controller/recover']) {
    assert.ok(calls.includes(method), `${method} has a canonical call schema`);
  }
  for (const name of ['ControllerRead', 'ControllerAcquire', 'ControllerRelease', 'ControllerRecover']) {
    const dto = definitions[name];
    assert.equal(dto.additionalProperties, false);
    assert.ok(dto.required.includes('scope'));
    for (const field of ['actor', 'connection', 'token', 'write', 'steering_revision']) {
      assert.equal(Object.hasOwn(dto.properties, field), false, `${name}.${field} cannot grant authority`);
    }
    if (name !== 'ControllerRead') assert.ok(dto.required.includes('command_id'));
    if (name === 'ControllerRelease' || name === 'ControllerRecover') {
      assert.ok(dto.required.includes('expected_revision'));
      assert.ok(dto.required.includes('generation'));
    }
  }
  assert.deepEqual(definitions.ControllerOwnership.enum, ['unclaimed', 'this_connection', 'other_connection', 'previous_process', 'released']);
});
