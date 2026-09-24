// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import schema from '@vcp/protocol/schema.json' with { type: 'json' };
import { validateWire, assertSupportedSchema } from '../dist/validation.js';
import { RpcFailure, classifyRpcError } from '../dist/errors.js';

test('bundled schema subset is supported and assertion drift fails closed', () => {
  assert.doesNotThrow(() => assertSupportedSchema(schema));
  for (const changed of [{ type: 'string', minLenght: 1 }, { format: 'email' }, { $ref: 'https://peer/schema' }, { $ref: '#/definitions/missing' }, { properties: { field: { not: {} } } }]) assert.throws(() => assertSupportedSchema(changed), /unsupported/);
  assert.throws(() => validateWire('__proto__', {}), /Unknown/);
});

test('Counter is an exact canonical u64 string and request IDs are safe', () => {
  for (const value of ['0', '9007199254740993', '18446744073709551615']) validateWire('Counter', value);
  for (const value of ['18446744073709551616', '00', '-1', '+1', '1.0', '1\n', 1, Number.MAX_SAFE_INTEGER + 1]) assert.throws(() => validateWire('Counter', value));
  for (const id of [null, 'sdk-1', Number.MAX_SAFE_INTEGER]) validateWire('RequestId', id);
  for (const id of [Number.MAX_SAFE_INTEGER + 1, 1.5, true]) assert.throws(() => validateWire('RequestId', id));
});

test('routing quality observations preserve the canonical unsigned 16-bit range', () => {
  const policy = {
    id: 'policy', parent_id: null, profile: 'low', quality_floor_bps: 7000,
    minimum_samples: 1, maximum_evidence_age_ms: '9007199254740993',
    deny_data_collection: true, require_zdr: true, ordering: ['quality'], pin: null,
    input_tokens: null, output_tokens: null, reasoning_effort: null,
    retrieval_limits: null, escalation_limits: null, broader_task_class: null,
    allowed_models_count: '1', allowed_endpoints_count: '1',
    allowed_groups_count: '1', pin_fallback_count: '0',
  };
  validateWire('RoutingStatusPolicySummary', policy);
  for (const quality_floor_bps of [-1, 65536, 0.5, '7000']) {
    assert.throws(() => validateWire('RoutingStatusPolicySummary', { ...policy, quality_floor_bps }));
  }
});

test('workspace binding projection accepts legacy shape and preserves opaque IDs and exact counters', () => {
  const legacy = { workspace: 'ws', host: 'host', root: 'C:\\project', trust: 'untrusted', revision: '13', authority_revision: '7' };
  validateWire('WorkspaceView', legacy);
  const binding = { ...legacy, root_id: 'opaque-root', binding_revision: '9007199254740993' };
  validateWire('WorkspaceView', binding);
  for (const fields of [{ root_id: 'bad/id' }, { binding_revision: 9007199254740992 }, { binding_revision: '18446744073709551616' }]) {
    assert.throws(() => validateWire('WorkspaceView', { ...binding, ...fields }));
  }
});

test('strict envelopes preserve null IDs and reject ambiguous successes and authority fields', () => {
  validateWire('ResultEnvelope', { jsonrpc: '2.0', id: null, result: null });
  const error = { code: -32602, message: 'invalid', data: { kind: 'validation', details: { secret: 'not a log message' } } };
  validateWire('ErrorEnvelope', { jsonrpc: '2.0', id: '1', error });
  for (const value of [{ jsonrpc: '2.0', result: 1 }, { jsonrpc: '2.0', id: 1, result: 1, error }, { jsonrpc: '2.0', id: 1, result: 1, params: {} }]) assert.throws(() => validateWire('ResultEnvelope', value));
  const call = { method: 'task/read', params: { scope: { workspace: 'ws', session: 's' }, task: 't' } };
  validateWire('Call', call);
  assert.throws(() => validateWire('Call', { ...call, params: { ...call.params, actor: 'owner' } }));
  const failure = new RpcFailure(error);
  assert.equal(failure.rpcCode, -32602);
  assert.deepEqual(failure.error, error);
  assert.ok(!failure.stack.includes('not a log message'));
  assert.ok(!JSON.stringify(failure).includes('not a log message'));
});

test('tagged unions, safe JSON objects and structural limits are enforced', () => {
  validateWire('ResultValue', { kind: 'unsubscribed', value: { subscription: 's' } });
  for (const value of [{ kind: 'unsubscribed', value: { subscription: 's', actor: 'owner' } }, { kind: 'unsubscribed', value: null }, { kind: 'secret', value: {} }]) assert.throws(() => validateWire('ResultValue', value));
  const inherited = Object.create({ workspace: 'ws' }); inherited.session = 's';
  const getter = Object.defineProperty({}, 'workspace', { enumerable: true, get() { throw Error('getter executed'); } });
  for (const value of [inherited, getter, JSON.parse('{"workspace":"ws","session":"s","__proto__":{}}')]) assert.throws(() => validateWire('Scope', value), error => error.name === 'SdkError' && !error.message.includes('executed'));
  assert.equal({}.polluted, undefined);
  let deep = {}; for (let i = 0; i < 70; i++) deep = { nested: deep };
  assert.throws(() => validateWire('ResultEnvelope', { jsonrpc: '2.0', id: '1', result: deep }), /budget/);
  assert.throws(() => validateWire('ResultEnvelope', { jsonrpc: '2.0', id: '1', result: '\ud800' }), /Unicode/);
});

test('oneOf requires exactly one branch and string length counts Unicode scalars', () => {
  // Test the interpreter semantics without introducing a second production schema.
  const branches = schema.definitions.ResultValue.oneOf;
  const unsubscribed = branches.find(branch => branch.properties.kind.enum[0] === 'unsubscribed');
  try {
    schema.definitions.ResultValue.oneOf = [unsubscribed, unsubscribed];
    assert.throws(() => validateWire('ResultValue', { kind: 'unsubscribed', value: { subscription: 's' } }));
  } finally { schema.definitions.ResultValue.oneOf = branches; }
  const resultSchema = schema.definitions.ResultEnvelope.properties.result;
  try {
    schema.definitions.ResultEnvelope.properties.result = { type: 'string', maxLength: 1 };
    validateWire('ResultEnvelope', { jsonrpc: '2.0', id: '1', result: '😀' });
    assert.throws(() => validateWire('ResultEnvelope', { jsonrpc: '2.0', id: '1', result: '😀a' }));
  } finally { schema.definitions.ResultEnvelope.properties.result = resultSchema; }
});

test('error classification preserves durable reconciliation without guessing unknown codes', () => {
  const application = details => ({ code: -32000, message: 'peer text ignored', data: { kind: 'application', details } });
  const details = { code: 'OUTCOME_UNKNOWN', retry: 'reconcile_original', operation: 'original-command', explanation: 'safe', reconciliation: null };
  const failure = new RpcFailure(application(details));
  assert.deepEqual(failure.classification, { category: 'outcome_unknown', applicationCode: 'OUTCOME_UNKNOWN', retry: 'reconcile_original', operation: 'original-command' });
  assert.equal(failure.error.data.details, details);
  for (const [code, category] of [['AUTHORITY_STALE', 'authority_stale'], ['APPROVAL_STALE', 'approval_stale'], ['INPUT_REQUIRED', 'input_required'], ['VERSION_CONFLICT', 'version_conflict'], ['CAPABILITY_UNAVAILABLE', 'unsupported_capability']]) {
    assert.equal(classifyRpcError(application({ ...details, code })).category, category);
  }
  for (const changed of [{ ...details, code: 'FUTURE_FAILURE' }, { ...details, retry: 'automatically' }, { ...details, operation: 'bad/id' }, { ...details, operation: 'a'.repeat(97) }, { ...details, explanation: '\ud800' }, { ...details, reconciliation: '\udfff' }, { ...details, actor: 'owner' }]) {
    const raw = application(changed);
    assert.equal(classifyRpcError(raw).category, 'unknown');
    assert.equal(new RpcFailure(raw).error.data.details, changed);
  }
  assert.equal(classifyRpcError({ ...application(details), code: -32123 }).category, 'unknown');
  assert.equal(classifyRpcError({ code: -32601, message: 'unknown' }).category, 'unsupported_method');
  assert.equal(classifyRpcError({ code: -32000, message: '', data: { kind: 'unsupported_version', details: { requested: '2.0', supported: ['1.0'] } } }).category, 'unsupported_version');
});
