// SPDX-License-Identifier: Apache-2.0
'use strict';
// Local codec work only. These counters never establish sends, usage or cost.
const {isDeepStrictEqual} = require('node:util');
const counters = ['encode_calls','encode_failures','encoded_bytes','encode_micros',
  'validation_calls','validation_failures','validation_micros'];
const purposes = ['compaction_trial','candidate_fit','final_assembly','sealed_validation'];
const uint = value => Number.isSafeInteger(value) && value >= 0;
const digest = value => typeof value === 'string' && /^[a-fA-F0-9]{64}$/.test(value);
function add(a,b) {
  const sum = a + b;
  if (!uint(sum)) throw Error('Encoding aggregate exceeds exact integer range');
  return sum;
}
function encodingStatistics(snapshot,scope,source) {
  const base = {source,scope,owner:snapshot?.owner ?? null,groups:[]};
  if (!snapshot) return {...base,available:false,reason:'not_collected'};
  const available = snapshot.encoding_available === undefined ? false : snapshot.encoding_available;
  const rows = snapshot.encodings === undefined ? [] : snapshot.encodings;
  const dropped = snapshot.encodings_dropped === undefined ? 0 : snapshot.encodings_dropped;
  if (typeof available !== 'boolean' || !Array.isArray(rows) || rows.length > 64 || !uint(dropped)
      || (!available && (rows.length !== 0 || dropped !== 0))) throw Error('Invalid encoding availability or bound');
  if (!available) return {...base,available:false,
    reason:snapshot.encoding_available === undefined ? 'legacy_not_collected' : 'collector_unavailable'};
  if (snapshot.schema_version !== 1 || snapshot.available !== true || typeof snapshot.owner !== 'string'
      || !snapshot.owner.length || snapshot.window !== 'current_owner_only' || snapshot.complete_history !== false
      || !uint(snapshot.snapshot_micros)) throw Error('Invalid encoding owner window');
  const groups = new Map();
  for (const row of rows) {
    const work = row?.work;
    if (!isDeepStrictEqual(row?.scope,scope)) throw Error('Encoding scope mismatch');
    if (!purposes.includes(row.purpose) || !digest(row.catalog)
        || (row.reasoning != null && !['minimal','low','medium','high'].includes(row.reasoning))
        || (row.turn != null && (typeof row.turn !== 'string' || !row.turn.length))
        || row.attempt != null // Current schema observes only pre-admission operations.
        || (row.request_sha256 != null && !digest(row.request_sha256))) throw Error('Invalid encoding attribution');
    if (!uint(row.started_micros) || !uint(row.elapsed_micros)
        || add(row.started_micros,row.elapsed_micros) > snapshot.snapshot_micros
        || !work || counters.some(key => !uint(work[key]))
        || Object.keys(work).some(key => !counters.includes(key))) throw Error('Invalid encoding work counters');
    if (work.encode_failures > work.encode_calls || work.validation_failures > work.validation_calls
        || work.encode_micros > row.elapsed_micros || work.validation_micros > row.elapsed_micros
        || (work.encode_calls === 0 && (work.encoded_bytes !== 0 || work.encode_micros !== 0))
        || (work.encode_calls === work.encode_failures && work.encoded_bytes !== 0)
        || (work.validation_calls === 0 && work.validation_micros !== 0)
        || (work.encode_calls === 0 && work.validation_calls === 0)
        || (row.purpose !== 'sealed_validation' && work.validation_calls !== 0)
        || (row.purpose === 'sealed_validation' && (work.validation_calls !== 1
          || work.encode_calls > 1 || work.encode_failures > work.validation_failures
          || work.encode_micros > work.validation_micros))) throw Error('Inconsistent encoding work counters');
    const key = JSON.stringify([row.purpose,row.catalog,row.reasoning ?? null]);
    if (!groups.has(key)) groups.set(key,{purpose:row.purpose,catalog:row.catalog,reasoning:row.reasoning ?? null,
      observations:0,...Object.fromEntries(counters.map(name => [name,0])),attribution:[]});
    const group = groups.get(key);
    group.observations++;
    for (const name of counters) group[name] = add(group[name],work[name]);
    group.attribution.push({turn:row.turn ?? null,attempt:null,request_sha256:row.request_sha256 ?? null,
      started_micros:row.started_micros,elapsed_micros:row.elapsed_micros});
  }
  return {...base,available:true,window:snapshot.window,complete_history:false,partial_window:true,
    retained_rows:rows.length,capacity:64,dropped_rows:dropped,dropped_rows_scope:'owner_wide_before_scope_filter',
    interpretation:'Encoded bytes sum successful codec outputs including repeated trials; they are not tokens, unique request bytes or sent bytes. Validation time includes its inner encoding; never add validation and encoding time. Stage elapsed overlaps enclosing lifecycle phases. Pre-admission rows do not prove an attempt or send. Only aggregates from exited stages (including failures) in this bounded owner window are present; owners and snapshots are never merged.',
    groups:[...groups.values()]};
}
module.exports = {encodingStatistics};
