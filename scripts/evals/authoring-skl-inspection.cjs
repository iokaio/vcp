// SPDX-License-Identifier: Apache-2.0
'use strict';
// Strict adapter for canonical read-only inspection. Callers keep the frozen
// 30-second API; only an authenticated `inspect` command receives 120 seconds.
const path = require('node:path');
const { isDeepStrictEqual: equal } = require('node:util');
const prior = require('./p6-live-runner.cjs');

const limits = Object.freeze({
  per_call_ms: 120000,
  aggregate_ms: 7200000,
  calls: 128,
  output_bytes: 268435456,
  artifact_bytes: 1048576,
  page_limit: 128,
  range_bytes: 65536,
});
const views = Object.freeze(['costs', 'routing', 'outputs', 'context', 'tools', 'verification']);
const descriptorViews = new Set(['outputs', 'context', 'tools']);
const emptySha256 = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
const identifier = value => typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
const must = (ok, message) => { if (!ok) throw Error(message); };

function createInspection({ executable, base, task, invoke, save, now = Date.now }) {
  must(path.isAbsolute(executable) && path.isAbsolute(base) && identifier(task) && typeof invoke === 'function' && typeof save === 'function', 'Exact inspection inputs required');
  const started = now(), calls = [], attempted = new Set(), cached = new Map(), cursors = new Set(), descriptors = new Map();
  let totalBytes = 0;

  function register(view, page) {
    if (!descriptorViews.has(view)) return;
    must(Array.isArray(page.items) && Array.isArray(page.gaps) && page.gaps.every(gap => prior.privacyGap(gap, gap.artifact)), 'Artifact descriptor page has unavailable evidence');
    for (const item of page.items.filter(item => item.collection === 'artifact')) {
      const descriptor = item.record, length = Number(descriptor?.length);
      must(identifier(item.id) && item.visibility === 'available' && descriptor?.spec?.id === item.id && descriptor.spec.scope?.task === task &&
        descriptor.state === 'complete' && Number.isSafeInteger(length) && length >= 0 && length <= limits.artifact_bytes && /^[a-f0-9]{64}$/.test(descriptor.sha256) &&
        (length !== 0 || descriptor.sha256 === emptySha256), 'Unapproved artifact descriptor');
      const key = view + ':' + item.id, old = descriptors.get(key);
      must(!old || equal(old, item), 'Canonical artifact descriptor changed');
      descriptors.set(key, item);
    }
  }

  function call(file, args, originalTimeout) {
    const prefix = ['--format', 'jsonl', '--non-interactive', '--workspace', path.join(base, 'workspace'), '--data-dir', path.join(base, 'data'), 'inspect'];
    must(file === executable && originalTimeout === 30000 && Array.isArray(args) && equal(args.slice(0, 8), prefix) &&
      identifier(args[8]) && args[9] === '--view' && views.includes(args[10]) && args[11] === '--limit' && args[12] === String(limits.page_limit), 'Only exact canonical inspection is allowed');
    const id = args[8], view = args[10], extra = args.slice(13), descriptor = descriptors.get(view + ':' + id);
    if (id === task) {
      must(extra.length === 0 || extra.length === 2 && extra[0] === '--cursor' && cursors.has(JSON.stringify([view, extra[1]])), 'Unapproved inspection pagination');
    } else {
      must(descriptor && extra.length === 4 && extra[0] === '--offset' && /^(0|[1-9][0-9]*)$/.test(extra[1]) && extra[2] === '--length' && extra[3] === String(limits.range_bytes), 'Unapproved artifact range');
      const offset = Number(extra[1]), length = Number(descriptor.record.length);
      must(Number.isSafeInteger(offset) && offset % limits.range_bytes === 0 && (length === 0 ? offset === 0 : offset < length), 'Artifact range outside descriptor');
    }
    const key = JSON.stringify(args);
    must(now() - started < limits.aggregate_ms, 'Inspection aggregate deadline exceeded');
    if (cached.has(key)) return cached.get(key);
    must(!attempted.has(key), 'Failed inspection cannot be retried');
    must(calls.length < limits.calls && totalBytes < limits.output_bytes, 'Inspection call/output ceiling exceeded');
    const timeout = Math.min(limits.per_call_ms, limits.aggregate_ms - (now() - started));
    must(timeout > 0, 'Inspection aggregate deadline exceeded');
    const metadata = { index: calls.length + 1, id, view, args: [...args], original_timeout_ms: originalTimeout, timeout_ms: timeout };
    attempted.add(key); calls.push(metadata);
    const began = now();
    let result;
    try { result = invoke(file, args, timeout); }
    catch (error) { result = { status: null, stdout: '', stderr: '', error: 'invocation_exception', exception: String(error?.message || error).slice(0, 2048) }; }
    metadata.elapsed_ms = now() - began; metadata.status = result.status; metadata.error = result.error ?? null;
    totalBytes += Buffer.byteLength(JSON.stringify(result)); metadata.cumulative_output_bytes = totalBytes;
    save(`inspection-${String(metadata.index).padStart(3, '0')}.json`, { metadata, result });
    must(totalBytes <= limits.output_bytes && now() - started <= limits.aggregate_ms, 'Inspection aggregate output/deadline exceeded');
    if (result.status !== 0 || result.error) return result;
    let frames;
    try { frames = prior.boundaries.frames(result.stdout).filter(frame => frame.type === 'result'); }
    catch { throw Error('Malformed canonical inspection output'); }
    must(frames.length === 1 && frames[0].data && Array.isArray(frames[0].data.items) && Array.isArray(frames[0].data.gaps), 'Ambiguous canonical inspection result');
    const page = frames[0].data;
    if (id === task) {
      must(page.schema_version === 1 && page.view === view && page.scope?.task === task, 'Canonical inspection page identity differs');
      if (page.next_cursor != null) {
        const cursor = JSON.stringify(page.next_cursor); must(cursor.length <= 4096, 'Inspection cursor ceiling exceeded');
        cursors.add(JSON.stringify([view, cursor]));
      }
      register(view, page);
    } else {
      const offset = Number(extra[1]), end = Math.min(offset + limits.range_bytes, Number(descriptor.record.length)), row = page.items[0];
      must(page.schema_version === 1 && page.view === view && equal(page.scope, descriptor.record.spec.scope) && page.items.length === 1 && page.next_cursor == null &&
        row?.artifact === id && row.visibility === 'available' && equal(row.descriptor, descriptor.record) && row.range?.start === offset && row.range.end === end &&
        Array.isArray(row.bytes) && row.bytes.length === end - offset && row.bytes.every(byte => Number.isInteger(byte) && byte >= 0 && byte <= 255) &&
        page.gaps.every(gap => prior.privacyGap(gap, id)), 'Artifact range identity/privacy differs');
    }
    cached.set(key, result);
    return result;
  }

  return { call, calls, descriptors };
}

module.exports = { createInspection, limits, views };
