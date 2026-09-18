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
  ],
  scoped: [
    'scoped_admission_holds_child_independently_of_parent_and_sibling',
    'scoped_admission_uses_review_delegate_identity',
    'scoped_admission_checks_mailbox_recipient_before_consuming_mail'
  ],
  host: [
    'independent_child_hold_survives_parent_readmission',
    'sealing_interrupts_both_streams_and_keeps_controllers_inspectable',
    'cancelled_waiter_preserves_seal_and_drains_issued_permits',
    'timed_out_drain_requires_successful_interruption_before_resume',
    'stale_foreign_and_unregistered_scope_cannot_gain_authority',
    'owner_loss_interrupts_active_work_and_cannot_be_resumed',
    'released_controller_is_not_retained_or_reported_as_interrupted',
    'checkpoint_reopen_requires_binding_interruption_and_deliberate_resume',
    'aborted_model_request_requires_explicit_receipt_reconciliation',
    'retained_patch_dispatch_records_intent_and_completion',
    'pause_at_actual_tool_dispatch_prevents_file_effect',
    'private_controls_are_revision_checked_idempotent_and_revalidated',
    'startup_requires_explicit_single_use_workspace_authority',
    'graceful_close_fences_late_receipts_before_releasing_writer_lock',
    'native_pause_stops_grandchild_and_releases_exclusive_lock',
    'native_argv_environment_crlf_and_bounded_output_are_observed'
  ]
};
function validateResults(text, group) {
  if (/\bpanicked at\b/.test(text)) throw Error('Background Rust panic in lifecycle evidence');
  const expected = cases[group]?.map(name => (group === 'host' ? '' : prefix) + name);
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
function testBinary(log, group = 'core') {
  if (!['core', 'host'].includes(group)) throw Error('Unknown lifecycle artifact group');
  const target = group === 'host' ? 'controller' : 'all';
  const packageName = group === 'host' ? 'vcp-lifecycle' : 'codex-core';
  const messages = log.replace(/^\uFEFF/, '').split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  const matches = messages.filter(m => m.reason === 'compiler-artifact' && m.target?.name === target &&
    m.target.kind?.includes('test') && m.profile?.test === true && m.executable &&
    new RegExp('(?:#' + packageName + '@|/' + packageName + '#)\\d').test(m.package_id));
  if (matches.length !== 1 || !messages.some(m => m.reason === 'build-finished' && m.success === true)) {
    throw Error('Expected exactly one successfully compiled lifecycle integration artifact: ' + packageName);
  }
  return matches[0].executable;
}
module.exports = { cases, validateResults, testBinary };
