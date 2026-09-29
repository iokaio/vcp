// SPDX-License-Identifier: Apache-2.0
'use strict';
// New DOC cases only; the qualified historical Node oracle remains unchanged.
const original = require('./cs3-comparison-oracle.cjs'), path = require('node:path');
const coverage = {
  'DOC-cs3-inbox-decision-v2': [
    ['accepted pull decision', /accept[^.\n]*pull|pull[^.\n]*accept/i],
    ['outbound only', /outbound[- ]only|outbound (?:connections )?only/i],
    ['webhook inbound rejection', /webhook[^.\n]*(?:reject|inbound)|inbound[^.\n]*webhook/i],
    ['websocket persistent rejection', /websocket[^.\n]*(?:reject|persistent)|persistent[^.\n]*websocket/i],
    ['polling default 30', /default[^.\n]*30|30[^.\n]*default/i],
    ['integer inclusive polling range', /integer/i, /15/, /120/, /inclusive/i, /reject/i],
    ['at least once and event deduplication', /at[- ]least[- ]once/i, /event_id/, /duplicat/i],
    ['ack only after durable local storage', /acknowledge[^.\n]*(?:only )?after[^.\n]*durab[^.\n]*local|acknowledge[^.\n]*(?:only )?after[^.\n]*local[^.\n]*durab/i],
    ['seven day retention', /(?:seven|7)[- ]day|(?:seven|7) days/i],
    ['expired cursor snapshot resync and no silent drop', /expired? cursor/i, /snapshot/i, /resync/i, /(?:no|not|never)[^.\n]*silent/i],
    ['observed duplication and reconnect passes', /(?:pass|observ)[^.\n]*(?:duplicat|event_id)|(?:duplicat|event_id)[^.\n]*(?:pass|observ)/i, /(?:pass|observ)[^.\n]*reconnect|reconnect[^.\n]*(?:pass|observ)/i],
    ['expiry and concurrency not run', /(?:not.run|unrun)[^.\n]*(?:expiry|expiration)|(?:expiry|expiration)[^.\n]*(?:not.run|unrun)/i, /(?:not.run|unrun)[^.\n]*concurr|concurr[^.\n]*(?:not.run|unrun)/i],
    ['unaccepted push and 24h proposal', /unaccepted|not accepted|not supersed/i, /push/i, /24/, /draft|propos/i],
    ['polling consequence', /delay|latency/i],
  ],
  'DOC-cs3-window-upgrade-v2': [
    ['new key and removed old key', /window_minutes/, /window_hours[^.\n]*(?:removed|not an alias)|(?:removed|not an alias)[^.\n]*window_hours/i],
    ['integer inclusive range and absent default', /integer/i, /15/, /240/, /inclusive/i, /(?:absent|omit|missing|default)[^.\n]*60|60[^.\n]*(?:absent|omit|missing|default)/i],
    ['persisted data compatible without migration', /v?2\.3/, /readable|read.compatib/i, /no (?:data )?migration/i],
    ['CSV order unchanged', /CSV[^.\n]*column[^.\n]*unchanged|unchanged[^.\n]*CSV/i],
    ['conversion and conflict rejection', /(?:multiply|multiplying|times|×|\*)[^.\n]*60|60[^.\n]*(?:multiply|multiplying|times|×|\*)/i, /both[^.\n]*(?:reject|invalid)|(?:reject|invalid)[^.\n]*both/i],
    ['stop and backup before replacement', /stop[^.\n]*service|service[^.\n]*stop/i, /back[ -]?up/i, /before[^.\n]*(?:replac|config)|(?:replac|config)[^.\n]*before/i],
    ['restart effective value before reopening', /restart/i, /inspect[^.\n]*effective|effective[^.\n]*inspect/i, /before[^.\n]*reopen/i],
    ['rollback configuration and binary', /restor[^.\n]*config/i, /v?2\.3[^.\n]*binary|binary[^.\n]*v?2\.3/i, /verif[^.\n]*previous|previous[^.\n]*verif/i],
    ['Windows observed conversion and rollback', /Windows/i, /(?:one|1) hour/i, /rollback/i, /pass|observ/i],
    ['Linux restart and both exact endpoints not run', /Linux[^.\n]*(?:not.run|unrun)|(?:not.run|unrun)[^.\n]*Linux/i, /(?:not.run|unrun)[^.\n]*15|15[^.\n]*(?:not.run|unrun)/i, /(?:not.run|unrun)[^.\n]*240|240[^.\n]*(?:not.run|unrun)/i],
    ['automation and zero downtime not accepted', /automatic/i, /zero[- ]downtime/i, /not (?:accept|approv)|unaccepted|propos/i],
  ],
};
function textual(task, answer, files) {
  if (!Object.hasOwn(coverage, task.id)) return original.textual(task, answer, files);
  const text = Object.values(files).join('\n'), checks = [];
  const check = (name, passed) => checks.push({ name, passed: Boolean(passed) });
  const targets = [...text.matchAll(/\[[^\]]+\]\(([^)]+)\)/g)].map(m => path.posix.normalize(path.posix.join(path.posix.dirname(task.outputs[0]), m[1])));
  check('all supplied sources cited', Object.keys(task.files).every(name => targets.includes(name)));
  check('every local citation resolves to supplied source', targets.length > 0 && targets.every(name => Object.hasOwn(task.files, name)));
  check('boundary table', /\|[^\n]+\|\s*\n\s*\|?\s*:?-{2,}/.test(text));
  for (const [name, ...patterns] of coverage[task.id]) check(name, patterns.every(pattern => pattern.test(text)));
  return { passed: checks.every(c => c.passed), checks };
}
module.exports = { textual, coverage };
