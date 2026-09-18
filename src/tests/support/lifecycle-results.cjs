// SPDX-License-Identifier: Apache-2.0
'use strict';
const prefix = 'suite::turn_input_submission::';
const cases = {
  drain: [
    'host_drain_allows_mailbox_work_to_start_a_turn',
    'host_drain_allows_running_review_to_finish_its_delegate',
    'host_drain_allows_spawned_agent_input_but_not_automatic_work',
    'host_drain_closes_realtime_after_handoff_error',
    'host_drain_rejects_turn_start_paths_without_recording_input'
  ],
  continuation: [
    'continuation_seal_rejects_delegated_input_without_recording_it::ordinary_closed',
    'continuation_seal_rejects_delegated_input_without_recording_it::ordinary_open',
    'continuation_seal_rejects_review_delegate_without_a_model_call::ordinary_closed',
    'continuation_seal_rejects_review_delegate_without_a_model_call::ordinary_open',
    'continuation_seal_retains_mail_until_explicit_readmission'
  ]
};
function validateResults(text, group) {
  const expected = cases[group]?.map(name => prefix + name);
  if (!expected) throw Error('Unknown lifecycle group');
  const rows = [...text.matchAll(/^test (\S+) \.\.\. (\S+)\s*$/gm)];
  const summaries = [...text.matchAll(/^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; \d+ filtered out;/gm)];
  if (rows.length !== expected.length || new Set(rows.map(row => row[1])).size !== expected.length ||
      rows.some(row => !expected.includes(row[1]) || row[2] !== 'ok') || summaries.length !== 1 ||
      summaries[0][1] !== String(expected.length) || summaries[0].slice(2, 5).some(value => value !== '0')) {
    throw Error('Incomplete or unexpected lifecycle test evidence');
  }
  return expected.length;
}
function testBinary(log) {
  const messages = log.replace(/^\uFEFF/, '').split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  const matches = messages.filter(m => m.reason === 'compiler-artifact' && m.target?.name === 'all' &&
    m.target.kind?.includes('test') && m.profile?.test === true && m.executable &&
    /(?:#|\/)codex-core@/.test(m.package_id));
  if (matches.length !== 1 || !messages.some(m => m.reason === 'build-finished' && m.success === true)) {
    throw Error('Expected exactly one successfully compiled codex-core integration test binary');
  }
  return matches[0].executable;
}
module.exports = { cases, validateResults, testBinary };
