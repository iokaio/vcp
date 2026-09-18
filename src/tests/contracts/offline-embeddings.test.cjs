// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { blocked, validate, validateTraffic } = require('../support/offline-embeddings.cjs');
const nonce = 'a'.repeat(32), model = 'b'.repeat(64);
const denial = () => ({ outcome: 'blocked', nonce, diagnosis_status: 0, missing_capability: 2, timed_out: true, os_error: 10060 });
const attempt = () => ({ exit_code: 0, capture_complete: true, status: 'pass', reason: null });
const outcome = () => ({ cleanup: 'completed', result: { ExitCode: 0, AppContainer: true, CapabilityCount: 0, TokenSidMatchesProfile: true, PeakJobCommittedBytes: 250000000, WallMilliseconds: 5000 } });
const inference = () => ({ status: 'pass', device: 'cpu', dimensions: 384, cases: 4, checks: 4, asset_spec_sha256: model,
  checks_passed: ['independent_reference', 'batch_padding', 'truncation_reporting', 'model_reopen'],
  reference_max_delta: 1e-7, batch_max_delta: 1e-7, reopen_max_delta: 1e-7, network: { before: denial(), after: denial() } });
test('network timeout alone, wrong capability diagnostic and refused connection cannot prove denial', () => {
  blocked(denial(), nonce);
  blocked({ ...denial(), timed_out: false, os_error: 10013 }, nonce);
  for (const change of [{ missing_capability: 0 }, { diagnosis_status: 5 }, { nonce: 'f'.repeat(32) },
    { timed_out: false, os_error: 10061 }, { outcome: 'connected' }]) assert.throws(() => blocked({ ...denial(), ...change }, nonce));
});
test('passing model output requires observed containment, complete capture and profile cleanup', () => {
  validate('inference', nonce, attempt(), inference(), outcome(), model, '');
  for (const change of [{ AppContainer: false }, { CapabilityCount: 1 }, { TokenSidMatchesProfile: false }, { ExitCode: 1 }, { PeakJobCommittedBytes: 0 }]) {
    const bad = outcome(); Object.assign(bad.result, change);
    assert.throws(() => validate('inference', nonce, attempt(), inference(), bad, model, ''));
  }
  assert.throws(() => validate('inference', nonce, attempt(), inference(), { ...outcome(), cleanup: 'pending' }, model, ''));
  for (const change of [{ capture_complete: false }, { reason: 'timeout' }, { exit_code: 1 }])
    assert.throws(() => validate('inference', nonce, { ...attempt(), ...change }, inference(), outcome(), model, ''));
  for (const change of [{ device: 'gpu' }, { reference_max_delta: NaN }, { reopen_max_delta: 1 }, { asset_spec_sha256: 'c'.repeat(64) }, { checks: 3 }])
    assert.throws(() => validate('inference', nonce, attempt(), { ...inference(), ...change }, outcome(), model, ''));
});
test('asset faults must be the intended structured rejection from the contained process', () => {
  for (const [mode, code, kind] of [['missing', 3, 'missing_asset'], ['corrupt', 1, 'invalid_asset']]) {
    const trial = { ...attempt(), exit_code: code, status: 'fail', reason: 'child_failed' };
    const observed = outcome(); observed.result.ExitCode = code;
    const result = { status: 'error', network_before: denial(), failure: { kind, file: 'config.json', reason: 'SHA-256 mismatch' } };
    validate(mode, nonce, trial, result, observed, model, kind);
    assert.throws(() => validate(mode, nonce, { ...trial, reason: 'timeout' }, result, observed, model, kind));
    assert.throws(() => validate(mode, nonce, trial, { ...result, failure: { kind: 'runtime' } }, observed, model, kind));
    assert.throws(() => validate(mode, nonce, trial, result, observed, model, ''));
  }
});
test('independent traffic requires exactly both unique controls and rejects restricted traffic', () => {
  const controls = [nonce, 'c'.repeat(32)], traffic = controls.map(n => 'VCP_LOCAL_CANARY ' + n + '\n');
  validateTraffic(traffic, controls);
  for (const wrong of [[], traffic.slice(0, 1), [traffic[0], traffic[0]], [...traffic, 'VCP_LOCAL_CANARY ' + 'd'.repeat(32) + '\n']])
    assert.throws(() => validateTraffic(wrong, controls));
  const normal = outcome(); normal.result.AppContainer = false; normal.result.TokenSidMatchesProfile = false;
  const result = { status: 'pass', phase: 'network-control', observation: { outcome: 'connected', nonce } };
  validate('control-before', nonce, attempt(), result, normal, model, '');
  assert.throws(() => validate('control-before', nonce, attempt(), { ...result, observation: denial() }, normal, model, ''));
});
