// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');
function blocked(row, nonce) {
  if (row?.outcome !== 'blocked' || row.nonce !== nonce || row.diagnosis_status !== 0 ||
      ![1, 2, 3].includes(row.missing_capability) || !(row.timed_out === true || row.os_error === 10013)) throw Error('Insufficient network denial evidence');
}
function validate(mode, nonce, attempt, result, outcome, modelHash, stderr) {
  const control = mode.startsWith('control-'), fault = ['missing', 'corrupt'].includes(mode);
  const expectedExit = mode === 'missing' ? 3 : mode === 'corrupt' ? 1 : 0;
  if (!attempt || attempt.exit_code !== expectedExit || attempt.capture_complete !== true ||
      attempt.status !== (fault ? 'fail' : 'pass') || attempt.reason !== (fault ? 'child_failed' : null) ||
      outcome?.cleanup !== 'completed' || outcome.result?.ExitCode !== expectedExit ||
      outcome.result.AppContainer !== !control || outcome.result.CapabilityCount !== 0 ||
      outcome.result.TokenSidMatchesProfile !== !control ||
      !Number.isSafeInteger(outcome.result.PeakJobCommittedBytes) || outcome.result.PeakJobCommittedBytes <= 0 ||
      !Number.isSafeInteger(outcome.result.WallMilliseconds) || outcome.result.WallMilliseconds < 0) throw Error('Incomplete process, capture or cleanup evidence');
  if (control) {
    if (result?.status !== 'pass' || result.phase !== 'network-control' || result.observation?.outcome !== 'connected' ||
        result.observation.nonce !== nonce) throw Error('Control did not connect');
  } else if (fault) {
    const kind = mode === 'missing' ? 'missing_asset' : 'invalid_asset';
    blocked(result?.network_before, nonce);
    if (result.status !== 'error' || result.failure?.kind !== kind || result.failure.file !== 'config.json' ||
        (mode === 'corrupt' && result.failure.reason !== 'SHA-256 mismatch') || !stderr.includes(kind)) throw Error('Wrong asset rejection');
  } else {
    blocked(result?.network?.before, nonce); blocked(result?.network?.after, nonce);
    if (result.status !== 'pass' || result.device !== 'cpu' || result.dimensions !== 384 || result.cases !== 4 || result.checks !== 4 ||
        result.asset_spec_sha256 !== modelHash || JSON.stringify(result.checks_passed) !==
        JSON.stringify(['independent_reference', 'batch_padding', 'truncation_reporting', 'model_reopen'])) throw Error('Incomplete real inference evidence');
    for (const key of ['reference_max_delta', 'batch_max_delta', 'reopen_max_delta']) {
      if (!Number.isFinite(result[key]) || result[key] < 0 || result[key] > 1e-5) throw Error('Embedding result differs from reference');
    }
  }
}
function validateTraffic(received, controls) {
  if (received.length !== 2 || JSON.stringify(received.slice().sort()) !== JSON.stringify(controls.map(n => 'VCP_LOCAL_CANARY ' + n + '\n').sort())) throw Error('Unexpected or missing independently observed canary traffic');
}
// Called only inside the harness's allowlisted environment. Windows caches its
// profile paths on process startup; restore these two paths for the trusted
// broker. Its native child receives a separate explicit environment in C#.
if (require.main === module) {
  const config = process.argv[2], request = JSON.parse(fs.readFileSync(config));
  const result = spawnSync('pwsh', ['-NoProfile', '-File', request.worker, '-Config', config], {
    env: { ...process.env, USERPROFILE: request.userProfile, LOCALAPPDATA: request.localAppData },
    stdio: 'inherit', windowsHide: true
  });
  process.exitCode = result.error || result.signal || result.status === null ? 1 : result.status;
}
module.exports = { blocked, validate, validateTraffic };
