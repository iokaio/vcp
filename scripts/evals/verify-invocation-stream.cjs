// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const {isDeepStrictEqual} = require('node:util');
const {analyze} = require('./analyze-execution-bundle.cjs');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function requireThat(value, message) { if (!value) throw Error(message); }
function sequence(value) {
  requireThat(typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value), 'Invalid decimal sequence');
  const result = BigInt(value);
  requireThat(result <= 18446744073709551615n, 'Sequence exceeds u64');
  return result;
}
function stream(frames) {
  requireThat(Array.isArray(frames) && frames.length > 2, 'Missing invocation frames');
  const accepted = frames.filter(row => row.type === 'accepted');
  const results = frames.filter(row => row.type === 'result');
  requireThat(accepted.length === 1 && results.length === 1 && frames[0] === accepted[0]
    && frames.at(-1) === results[0], 'Expected one leading acceptance and one terminal result');
  const start = accepted[0], end = results[0], scope = start.scope;
  requireThat(scope && ['workspace','session','task'].every(key => typeof scope[key] === 'string' && scope[key]), 'Missing task scope');
  requireThat(isDeepStrictEqual(scope,end.scope) && typeof start.correlation === 'string' && start.correlation
    && start.correlation === end.correlation, 'Invocation identity mismatch');
  requireThat(end.exit_code === 0 && end.conditions?.completed === true
    && Object.entries(end.conditions).every(([key,value]) => key === 'completed' || value === false), 'Invocation did not complete successfully');
  const events = new Map();
  let previous = -1n;
  for (const frame of frames) {
    requireThat(frame.schema_version === 1 && frame.correlation === start.correlation, 'Invalid frame identity');
    requireThat(['accepted','event','result','retention_notice'].includes(frame.type), 'Unexpected frame or cursor gap');
    if (frame.type !== 'event') continue;
    const envelope = frame.event, event = envelope?.event, seq = sequence(envelope?.sequence);
    requireThat(event?.workspace === scope.workspace && event?.session === scope.session, 'Event outside invocation session');
    requireThat(seq > previous, 'Event sequences are not unique and increasing');
    requireThat(previous < 0n || seq === previous + 1n, 'Missing interior invocation event');
    previous = seq;
    events.set(seq,event);
  }
  requireThat(events.size > 0, 'No invocation events');
  for (const [label,receipt] of [['accepted',start.receipt],['final',end.receipt]]) {
    requireThat(receipt?.version === 1 && receipt.workspace === scope.workspace
      && typeof receipt.command === 'string' && receipt.command, 'Invalid receipt scope');
    if (label === 'accepted') requireThat(receipt.command === start.correlation, 'Acceptance command mismatch');
    const first = sequence(receipt.first_event), last = sequence(receipt.last_event);
    requireThat(first > 0n && last >= first && last - first < BigInt(events.size), 'Invalid receipt range');
    for (let seq = first; seq <= last; seq++) {
      requireThat(events.get(seq)?.correlation === receipt.command, `${label} receipt event missing or mismatched`);
    }
  }
  return {scope,events,first:events.keys().next().value,last:previous,
    accepted_receipt:start.receipt,final_receipt:end.receipt};
}
function verify(firstFrames, secondFrames) {
  const first = stream(firstFrames), second = stream(secondFrames);
  requireThat(first.scope.workspace === second.scope.workspace && first.scope.session === second.scope.session
    && first.scope.task !== second.scope.task, 'Expected a new task in the same workspace and session');
  requireThat(second.first > first.last, 'Second invocation replays prior session events');
  // Newly appended authorized events about another task (including the first
  // task) are allowed. The sequence boundary excludes replay; this is not a task filter.
  return {schema:'vcp-invocation-stream-check/1',passed:true,first_scope:first.scope,second_scope:second.scope,
    first_last_sequence:String(first.last),second_first_sequence:String(second.first),second_last_sequence:String(second.last),
    second_event_count:second.events.size,accepted_receipt:second.accepted_receipt,final_receipt:second.final_receipt};
}
function encoding(bundle,scope) {
  requireThat(isDeepStrictEqual(bundle.task?.scope,scope), 'Inspection task scope mismatch');
  const analysis = analyze(bundle);
  const snapshots = analysis.facts.retained_lifecycle_statistics.map(row => row.encoding_statistics);
  requireThat(snapshots.some(snapshot => snapshot.available && snapshot.groups.some(group => group.purpose === 'final_assembly'
    && group.encode_calls > group.encode_failures) && snapshot.groups.some(group => group.purpose === 'sealed_validation'
    && group.validation_calls > group.validation_failures)), 'Missing retained successful assembly and sealed-validation measurements');
  // Preserve each owner window; no addition of overlapping validation/encoding time.
  return snapshots;
}
function read(file,jsonl=false) {
  requireThat(fs.statSync(file).size <= 64 * 1024 * 1024, 'Evidence exceeds this small-probe reader bound');
  const bytes = fs.readFileSync(file), text = new TextDecoder('utf-8',{fatal:true}).decode(bytes);
  let data;
  if (jsonl) {
    requireThat(text.endsWith('\n'), 'Incomplete JSONL tail');
    const lines = text.trimEnd().split(/\r?\n/);
    requireThat(lines.every(line => line.length && Buffer.byteLength(line) <= 1024 * 1024), 'Invalid JSONL frame bound');
    data = lines.map(line => JSON.parse(line));
  } else data = JSON.parse(text);
  return {data,sha256:sha(bytes),bytes:bytes.length};
}
if (require.main === module) {
  try {
    const [firstPath,secondPath,firstBundlePath,secondBundlePath,out,...extra] = process.argv.slice(2);
    requireThat(out && extra.length === 0, 'Usage: node verify-invocation-stream.cjs first.jsonl second.jsonl first-bundle.json second-bundle.json new-report.json');
    const first=read(firstPath,true), second=read(secondPath,true), firstBundle=read(firstBundlePath), secondBundle=read(secondBundlePath);
    const report=verify(first.data,second.data);
    report.encoding = {first:encoding(firstBundle.data,report.first_scope),second:encoding(secondBundle.data,report.second_scope)};
    report.inputs = [firstPath,secondPath,firstBundlePath,secondBundlePath].map((file,index) => {
      const {sha256,bytes} = [first,second,firstBundle,secondBundle][index];
      return {path:file,sha256,bytes};
    });
    report.evaluators = [__filename,require.resolve('./analyze-execution-bundle.cjs'),require.resolve('./encoding-statistics.cjs')]
      .map(file => ({path:file,sha256:sha(fs.readFileSync(file))}));
    report.limitations = 'Local retained framing and receipt joins; no independent cryptographic authentication or billing inference. Encoding windows remain partial and separate. Native execution and independent cart/source gates are reported by the scenario harness.';
    fs.writeFileSync(out,JSON.stringify(report,null,2)+'\n',{flag:'wx'});
    process.stdout.write('Second invocation stream and retained encoding checks passed.\n');
  } catch (error) { process.stderr.write(error.message+'\n'); process.exitCode=1; }
}
module.exports = {verify,encoding,read};
