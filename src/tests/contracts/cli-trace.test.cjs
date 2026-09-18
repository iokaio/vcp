// SPDX-License-Identifier: Apache-2.0
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { ownedRoot } = require('../support/experiments.cjs');
const { fixtureProvider, verifyTrace, PROMPT, ANSWER, PATCH_TEXT, COMPACT_PROMPT, COMPACT_SUMMARY, USAGE } = require('../support/cli-trace.cjs');
const input = [{ type: 'message', role: 'user', content: [{ type: 'input_text', text: PROMPT }] }];
function request(status = 200) { return { response_status: status, response_usage: status === 200 ? { ...USAGE } : null, method: 'POST', path: '/v1/responses', synthetic_auth: true, body: { model: 'gpt-5.4', input: structuredClone(input) } }; }
function completed(count = 1) { return [
  { type: 'thread.started' }, { type: 'turn.started' },
  { type: 'item.completed', item: { type: 'agent_message', text: ANSWER } }, { type: 'turn.completed', usage: { input_tokens: 10 * count, output_tokens: 2 * count } }
]; }
const jsonl = items => items.map(item => JSON.stringify(item)).join('\n');
test('native trace reports missing platform or binary as not run', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const cli = path.resolve(__dirname, '../../../scripts/upstream/trace-cli.cjs');
    const result = spawnSync(process.execPath, [cli, '--binary', path.join(fixture.root, 'missing.exe')], { encoding: 'utf8', windowsHide: true });
    assert.equal(result.status, 3, result.stderr);
    const summary = JSON.parse(result.stdout);
    assert.equal(summary.status, 'not_run');
    const manifest = JSON.parse(fs.readFileSync(summary.manifest, 'utf8'));
    assert.deepEqual(manifest.attempts, []);
    assert.ok(manifest.reason.includes('required'));
  } finally { fixture.cleanup(); }
});
test('loopback fixture produces bounded transient retry and rejects unexpected routes', async () => {
  const recorded = [];
  const provider = await fixtureProvider('retry', observation => recorded.push(structuredClone(observation)));
  try {
    const post = () => fetch(provider.url + '/responses', { method: 'POST', headers: { authorization: 'Bearer synthetic-local-only' }, body: JSON.stringify({ model: 'gpt-5.4', input }) });
    const first = await post(); assert.equal(first.status, 503); await first.text();
    const second = await post(); assert.equal(second.status, 200); assert.match(await second.text(), /response.completed/);
    const third = await post(); assert.equal(third.status, 400); await third.text();
    assert.equal(provider.requests.length, 3); assert.deepEqual(provider.errors, ['Unexpected extra request']);
    assert.deepEqual(recorded.map(item => item.response_status), [503, 200, 400]);
    assert.deepEqual(recorded[1].response_usage, USAGE);
    assert.equal(recorded[2].response_usage, null);
    const wrong = await fetch(provider.url + '/unexpected', { method: 'POST', body: '{}' });
    assert.equal(wrong.status, 400); await wrong.text();
    assert.ok(provider.errors.includes('Unexpected provider route'));
    assert.equal(recorded[3].path, '/v1/unexpected', 'Rejected traffic remains in retained observations');
  } finally { await provider.close(); }
});
test('native trace oracle rejects hidden retries, wrong auth and false completion', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const state = { requests: [request()], errors: [], code: 0, stdout: jsonl(completed()), workspace: fixture.root };
    assert.throws(() => verifyTrace('unknown', state), /Known trace case/);
    assert.equal(verifyTrace('completion', state).requests, 1);
    assert.throws(() => verifyTrace('completion', { ...state, requests: [request(), request()] }), /Exact request count/);
    const wrongAuth = request(); wrongAuth.synthetic_auth = false;
    assert.throws(() => verifyTrace('completion', { ...state, requests: [wrongAuth] }));
    assert.throws(() => verifyTrace('completion', { ...state, stdout: jsonl(completed().slice(0, 3)) }));
    assert.throws(() => verifyTrace('provider-denied', state));
    assert.equal(verifyTrace('provider-denied', { ...state, requests: [request(401)], code: 1, stdout: jsonl([{ type: 'thread.started' }, { type: 'turn.started' }, { type: 'turn.failed' }]) }).requests, 1);
    assert.throws(() => verifyTrace('completion', { ...state, errors: ['unexpected traffic'] }));
  } finally { fixture.cleanup(); }
});
test('patch oracle requires both provider receipt and independently observed file bytes', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const first = request(); first.body.tools = [{ type: 'custom', name: 'apply_patch' }];
    const second = request(); second.body.input.push({ type: 'custom_tool_call_output', call_id: 'fixture-patch', output: 'Success. Updated synthetic.txt' });
    const output = completed(2); output.splice(2, 0, { type: 'item.completed', item: { type: 'file_change', status: 'completed' } });
    const state = { requests: [first, second], errors: [], code: 0, stdout: jsonl(output), workspace: fixture.root };
    assert.throws(() => verifyTrace('patch', state), /ENOENT/);
    fs.writeFileSync(path.join(fixture.root, 'synthetic.txt'), 'wrong content\n');
    assert.throws(() => verifyTrace('patch', state));
    fs.writeFileSync(path.join(fixture.root, 'synthetic.txt'), PATCH_TEXT);
    assert.equal(verifyTrace('patch', state).tool_effects, 1);
    second.body.input.pop(); assert.throws(() => verifyTrace('patch', state), /Observed tool receipt/);
    second.body.input.push({ type: 'custom_tool_call_output', call_id: 'fixture-patch', output: 'patch rejected: read-only sandbox' });
    const denied = { ...state, stdout: jsonl(completed(2)) };
    assert.throws(() => verifyTrace('patch-read-only', denied));
    fs.unlinkSync(path.join(fixture.root, 'synthetic.txt'));
    assert.equal(verifyTrace('patch-read-only', denied).tool_effects, 0);
  } finally { fixture.cleanup(); }
});

test('review oracle distinguishes parent usage from a real helper request and rejects false success', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const review = request(); review.body.instructions = '# Review guidelines: synthetic'; review.body.tools = [];
    const state = { requests: [review], errors: [], code: 0, stdout: jsonl(completed(0)), workspace: fixture.root };
    const result = verifyTrace('review', state);
    assert.deepEqual(result.provider_usage, { input_tokens: 10, output_tokens: 2 });
    assert.equal(result.usage_matches_provider, false);
    assert.throws(() => verifyTrace('review', { ...state, requests: [review, review] }), /Exact request count/);
    const wrong = structuredClone(review); wrong.body.instructions = 'ordinary coding prompt';
    assert.throws(() => verifyTrace('review', { ...state, requests: [wrong] }), /Review delegate/);
    wrong.body.instructions = review.body.instructions; wrong.response_usage.input_tokens = 0;
    assert.throws(() => verifyTrace('review', { ...state, requests: [wrong] }), /supplied usage/);
    const rejected = { ...review, response_status: 401, response_usage: null };
    const failedOutput = [{ type: 'thread.started' }, { type: 'turn.started' }, { type: 'turn.failed' }];
    const failure = { ...state, requests: [rejected], code: 1, stdout: jsonl(failedOutput) };
    assert.equal(verifyTrace('review-denied', failure).cli_usage, null);
    assert.throws(() => verifyTrace('review-denied', { ...failure, code: 0 }), /nonzero exit/);
    failedOutput.splice(2, 0, { type: 'item.completed', item: { type: 'agent_message', text: ANSWER } });
    assert.throws(() => verifyTrace('review-denied', { ...failure, stdout: jsonl(failedOutput) }), /cannot report synthetic success/);
  } finally { fixture.cleanup(); }
});
test('compaction oracle requires a separate tool-free request, summary consumption and failed-helper termination', () => {
  const fixture = ownedRoot(os.tmpdir());
  try {
    const first = request(); first.body.tools = [{ type: 'custom', name: 'apply_patch' }];
    const summary = request(); summary.body.tools = [];
    summary.body.input.push({ type: 'custom_tool_call_output', call_id: 'fixture-patch', output: 'read-only sandbox' },
      { type: 'message', role: 'user', content: [{ type: 'input_text', text: COMPACT_PROMPT }] });
    const next = request(); next.body.tools = structuredClone(first.body.tools);
    next.body.input.push({ type: 'message', role: 'user', content: [{ type: 'input_text', text: COMPACT_SUMMARY }] });
    const state = { requests: [first, summary, next], errors: [], code: 0, stdout: jsonl(completed(3)), workspace: fixture.root };
    const result = verifyTrace('compaction', state);
    assert.equal(result.usage_matches_provider, true);
    assert.deepEqual(result.provider_usage, { input_tokens: 30, output_tokens: 6 });
    let wrong = structuredClone(state); wrong.requests[1].body.tools = first.body.tools;
    assert.throws(() => verifyTrace('compaction', wrong), /cannot invoke tools/);
    wrong = structuredClone(state); wrong.requests[2].body.input.pop();
    assert.throws(() => verifyTrace('compaction', wrong), /consumes summary/);
    wrong = structuredClone(state); wrong.requests[2].body.input.push(summary.body.input[1]);
    assert.throws(() => verifyTrace('compaction', wrong), /drops the earlier tool receipt/);
    wrong = structuredClone(state); wrong.requests[1].body.input.pop();
    assert.throws(() => verifyTrace('compaction', wrong), /Separate compaction prompt/);
    const failedSummary = { ...summary, response_status: 401, response_usage: null };
    const failure = { ...state, requests: [first, failedSummary], code: 1,
      stdout: jsonl([{ type: 'thread.started' }, { type: 'turn.started' }, { type: 'turn.failed' }]) };
    assert.equal(verifyTrace('compaction-denied', failure).provider_usage.input_tokens, 10);
    assert.throws(() => verifyTrace('compaction-denied', { ...failure, requests: [...failure.requests, next] }), /Exact request count/);
    assert.throws(() => verifyTrace('compaction-denied', { ...failure, stdout: jsonl(completed(3)) }));
  } finally { fixture.cleanup(); }
});
