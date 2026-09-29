// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const { tasks } = require('../../src/evals/skills/cs3-comparison/cohort-doc-successor.cjs');
const oracle = require('./cs3-doc-successor-oracle.cjs');
const examples = {
  'DOC-cs3-inbox-decision-v2': `# Accepted pull decision
Consumers are outbound-only. Webhooks rejected for inbound endpoint; WebSockets rejected for persistent connections. Polling default 30 seconds, integer 15..120 inclusive; reject outside range. Delivery at-least-once, duplicate suppression by event_id. Acknowledge only after durable local storage. Retention seven days. Expired cursor requires explicit snapshot resync; do not silently drop events. Polling adds bounded delay. [Decision](../decision.md) [Alternatives](../alternatives.md)
Observed passes: duplicate suppression and offline reconnect. Not run: exact seven-day expiry and high concurrency. [Observations](../observations.json)
Unaccepted proposal: push and 24 hours retention; draft has not superseded the accepted decision. [Draft](../draft.md)
| Boundary | Required outcome |
| --- | --- |
| Invalid interval | Reject |
`,
  'DOC-cs3-window-upgrade-v2': `# Accepted v2.4 upgrade
window_minutes replaces window_hours; old window_hours removed, not an alias. Integer 15..240 inclusive; absent default60. v2.3 persisted data readable with no data migration. CSV column order unchanged. [Release](../release.md)
Multiply prior hours by60, use only integer within15..240. Reject both keys. Stop service and back up before replacing configuration. Restart and inspect effective value before reopening. On failure stop, restore configuration and v2.3 binary, verify previous effective value before reopening. [Upgrade](../upgrade.md)
Windows observed passes: one hour to60 minutes conversion and rollback. Linux restart not run; exact15 and240 boundaries not run. [Evidence](../evidence.json)
Future proposal, not accepted: automatic conversion and zero-downtime rollout. [Draft](../draft.md)
| Boundary | Required action |
| --- | --- |
| Both keys | Reject |
`,
};
test('new DOC normals accept complete source-grounded drafts and reject missing facts/links/table', () => {
  for (const task of tasks.filter(t => t.kind === 'normal')) {
    const text = examples[task.id], grade = value => oracle.textual(task, { report: 'Draft only.', not_run: ['No execution.'] }, { [task.outputs[0]]: value });
    assert.deepEqual(grade(text).checks.filter(c => !c.passed), []);
    for (const mutate of [value => value.replace('../draft.md', '../missing.md'), value => value.replace('| --- | --- |', 'no table'),
      value => value.replace(task.id.includes('inbox') ? 'event_id' : 'CSV', 'REMOVED'), value => value.replace(task.id.includes('inbox') ? '24' : 'zero-downtime', 'REMOVED')]) assert.equal(grade(mutate(text)).passed, false);
  }
});
