// SPDX-License-Identifier: Apache-2.0
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const Module = require('node:module');
const { createHash, randomUUID } = require('node:crypto');
const hash = text => createHash('sha256').update(text).digest('hex');
const scope = { workspace: 'workspace', session: 'session' };
let current;
class Range { constructor(a, b, c, d) { this.start = { line: a, character: b }; this.end = { line: c, character: d }; } }
class EventEmitter { constructor() { this.listeners = []; this.event = listener => { this.listeners.push(listener); return { dispose() {} }; }; } fire(value) { this.listeners.forEach(listener => listener(value)); } dispose() {} }
const uri = (value, scheme = 'file') => ({ scheme, authority: '', query: '', fragment: '', fsPath: value, toString: () => `${scheme}:${value}` });
const vscode = { Range, EventEmitter, Uri: { parse: value => ({ toString: () => value }) }, languages: { getDiagnostics: () => [] },
  window: { get activeTextEditor() { return current.active; }, async showTextDocument(document) { return current.editor(document); },
    async showQuickPick(items, options) { return options?.canPickMany ? items : items[0]; }, async showInformationMessage() {}, async showWarningMessage() { return 'Apply'; } },
  workspace: { isTrusted: true, async openTextDocument(options) { const document = current.doc(`draft-${randomUUID()}`, options.content, 'untitled'); await current.beforeOpenReply?.(); return document; } },
  commands: { async executeCommand(command, left, right) { assert.equal(command, 'vscode.diff'); current.previews.push([left, right]); } },
};
const load = Module._load;
Module._load = function (request, parent, isMain) { return request === 'vscode' ? vscode : load.call(this, request, parent, isMain); };
let EditorChanges;
try { ({ EditorChanges } = require('../dist/editor_changes.js')); } finally { Module._load = load; }
const { EditorJournal } = require('../dist/editor_journal.js');

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vcp-editor-workflow-'));
  const f = { root, docs: [], calls: [], previews: [], applied: 0, observed: new Map(), changes: new Map(), saved: [], receipts: new Map(), taskRevision: 1, generation: 1 };
  current = f;
  f.doc = (name, text, scheme = 'file') => {
    const document = { uri: uri(scheme === 'file' ? path.join(root, name) : name, scheme), text, version: 1, isDirty: false, isClosed: false, languageId: 'plaintext', eol: 1, getText() { return this.text; } };
    if (scheme === 'file') fs.writeFileSync(document.uri.fsPath, text);
    f.docs.push(document); return document;
  };
  f.type = (document, text) => { document.text = text; document.version++; document.isDirty = true; f.view.invalidate(document); };
  f.editor = document => ({ document, selections: [new Range(0, 0, 0, 0)], async edit(callback) {
    let text;
    callback({ replace(range, value) { assert.deepEqual(range.start, { line: 0, character: 0 }); text = value; } });
    f.type(document, text); f.applied++; f.onApplied?.(document); return true;
  } });
  f.client = { scope, initialized: { capabilities: ['editor/prepared-edits/1'] }, async call(method, params) {
    f.calls.push({ method, params: structuredClone(params) });
    if (method === 'task/read') return { kind: 'task', value: { scope, task: 'task', state: 'running', revision: String(f.taskRevision), steering_revision: '0' } };
    if (method === 'command/read') { const receipt = f.receipts.get(params.command_id); if (!receipt) throw Error('missing'); return { kind: 'acceptance', value: receipt }; }
    const accept = () => f.receipts.set(params.mutation.command_id, { scope, task: 'task', command_id: params.mutation.command_id, outcome: 'accepted' });
    if (method === 'editor/context') {
      if (params.closed?.length && f.closeConflicts > 0) { f.closeConflicts--; f.taskRevision++; throw Object.assign(Error('revision changed'), { code: 'rpc', classification: { applicationCode: 'VERSION_CONFLICT', retry: 'never' } }); }
      assert.equal(params.mutation.expected_revision, String(f.taskRevision)); f.taskRevision++;
      for (const id of params.closed ?? []) { assert.ok(f.observed.has(id), 'only current observation may close'); f.observed.delete(id); }
      const observations = params.documents.map(document => {
        for (const [id, existing] of f.observed) if (existing.document.uri === document.uri) f.observed.delete(id);
        const row = { id: randomUUID(), document: { ...structuredClone(document), content: null }, root: 'root', disk_fingerprint: 'native-disk', artifact: null };
        f.observed.set(row.id, row); return row;
      });
      accept(); if (params.closed?.length && f.loseClose) { f.loseClose = false; throw Error('lost close reply'); } return { kind: 'editor_context', value: { generation: 'editor-generation', revision: String(f.taskRevision), observations } };
    }
    if (method === 'editor/prepare') {
      assert.equal(params.mutation.expected_revision, String(f.taskRevision));
      const view = { scope, task: 'task', change: params.mutation.command_id, revision: '0', generation: 'editor-generation', root: 'root', binding_revision: '1', authority_revision: '1', policy_revision: '1', steering_revision: '0', buffers_unverified: true,
        files: params.files.map((file, index) => {
          const observed = f.observed.get(file.observation); assert.ok(observed);
          const document = observed.document;
          return { file: index, observation: observed.id, effect: `effect-${index}`, uri: document.uri, relative_path: document.relative_path, host: document.host, open_id: document.open_id, version: document.version,
            content_sha256: document.content_sha256, disk_sha256: hash(fs.readFileSync(f.docs.find(doc => doc.uri.toString() === document.uri).uri.fsPath)), disk_fingerprint: 'native-disk', after_sha256: hash(file.edits[0].text), edits_digest: hash(JSON.stringify(file.edits)), state: 'prepared', execution: null, observed_version: null, observed_sha256: null, observed_observation: null, dirty: document.dirty };
        }) };
      f.changes.set(view.change, view); accept(); await f.beforePrepareReply?.(); if (f.losePrepare) throw Error('lost response'); return { kind: 'editor_change', value: structuredClone(view) };
    }
    const view = f.changes.get(params.change); assert.ok(view);
    if (method === 'editor/changeRead') return { kind: 'editor_change', value: structuredClone(view) };
    assert.equal(params.mutation.expected_revision, view.revision); view.revision = String(Number(view.revision) + 1);
    const file = view.files[params.file];
    if (method === 'editor/dispatch') {
      assert.equal(file.state, 'prepared'); file.state = 'dispatched'; file.execution = randomUUID(); accept();
      if (f.loseDispatch) throw Error('lost dispatch response');
      return { kind: 'editor_dispatch', value: { change: structuredClone(view), file: params.file, execution: file.execution, apply: true } };
    }
    assert.equal(method, 'editor/changeResult'); assert.equal(params.execution, file.execution);
    file.state = params.outcome; file.observed_version = params.document.version; file.observed_sha256 = params.document.content_sha256; file.dirty = params.document.dirty;
    for (const [id, row] of f.observed) if (row.document.uri === params.document.uri) f.observed.delete(id);
    file.observed_observation = randomUUID(); f.observed.set(file.observed_observation, { id: file.observed_observation, document: params.document });
    accept(); return { kind: 'editor_change', value: structuredClone(view) };
  } };
  f.connection = { currentClient: () => f.client, state: () => ({ phase: 'connected', role: 'controller', engineTrust: 'trusted', host: { id: 'host' }, rootId: 'root', workspaceRoot: root, bindingRevision: '1', generation: f.generation }) };
  f.journal = new EditorJournal(async records => { f.saved = structuredClone(records); f.onSave?.(records); });
  f.view = new EditorChanges(f.connection, () => ({ detail: { task: { scope, task: 'task' } } }), () => f.journal);
  f.run = async action => { await f.view.suspendObservations(); try { return await action(); } finally { f.view.resumeObservations(); } };
  f.draft = async (source, text) => { f.active = f.editor(source); await f.run(() => f.view.createDraft()); f.docs.at(-1).text = text; };
  f.cleanup = () => { f.view.dispose(); fs.rmSync(root, { recursive: true, force: true }); };
  return f;
}

test('review uses exact transient text, records intent before edit, and closes latest receipt observation after success', async () => {
  const f = fixture();
  try {
    const source = f.doc('one.txt', 'before\n'); await f.draft(source, 'after\n');
    await f.run(() => f.view.review());
    assert.equal(f.view.provideTextDocumentContent(f.previews[0][0]), 'before\n');
    assert.equal(f.view.provideTextDocumentContent(f.previews[0][1]), 'after\n');
    f.onApplied = () => assert.ok(f.saved.some(record => record.kind === 'dispatch'), 'durable dispatch identity precedes edit');
    await f.run(() => f.view.apply());
    assert.equal(source.text, 'after\n'); assert.equal(fs.readFileSync(source.uri.fsPath, 'utf8'), 'before\n');
    assert.equal([...f.changes.values()][0].files[0].state, 'applied');
    assert.ok(f.saved.every(record => !JSON.stringify(record).includes('before') && !JSON.stringify(record).includes('after')));
    source.isClosed = true; f.view.invalidate(source);
    await new Promise(resolve => setTimeout(resolve, 240)); await f.view.suspendObservations();
    assert.equal(f.observed.size, 0, 'close retires latest receipt handle even after draft completion');
  } finally { f.cleanup(); }
});

test('typing after review rejects before dispatch and preserves user text', async () => {
  const f = fixture();
  try {
    const source = f.doc('one.txt', 'before'); await f.draft(source, 'proposed'); await f.run(() => f.view.review());
    f.type(source, 'human typing');
    await assert.rejects(f.run(() => f.view.apply()), /source changed/);
    assert.equal(source.text, 'human typing'); assert.equal(f.applied, 0);
    assert.equal(f.calls.some(call => call.method === 'editor/dispatch'), false);
  } finally { f.cleanup(); }
});

test('multi-file conflict preserves the first receipt and does not dispatch the changed sibling', async () => {
  const f = fixture();
  try {
    const first = f.doc('one.txt', 'one'), second = f.doc('two.txt', 'two');
    await f.draft(first, 'first proposed'); await f.draft(second, 'second proposed'); await f.run(() => f.view.review());
    f.onApplied = document => { if (document === first) f.type(second, 'human sibling'); };
    await assert.rejects(f.run(() => f.view.apply()), /source changed/);
    assert.equal(first.text, 'first proposed'); assert.equal(second.text, 'human sibling');
    assert.deepEqual([...f.changes.values()][0].files.map(file => file.state), ['applied', 'prepared']);
    assert.equal(f.calls.filter(call => call.method === 'editor/dispatch').length, 1);
  } finally { f.cleanup(); }
});

test('undo before receipt persistence is reported unknown with actual current observation', async () => {
  const f = fixture();
  try {
    const source = f.doc('one.txt', 'before'); await f.draft(source, 'after'); await f.run(() => f.view.review());
    let undone = false;
    f.onSave = records => { if (!undone && records.some(record => record.kind === 'result' && record.phase === 'pending')) { undone = true; f.type(source, 'before'); } };
    await f.run(() => f.view.apply());
    const receipt = f.calls.find(call => call.method === 'editor/changeResult').params;
    assert.equal(receipt.outcome, 'unknown'); assert.equal(receipt.document.content_sha256, hash('before'));
    assert.equal([...f.changes.values()][0].files[0].state, 'unknown');
    await assert.rejects(f.run(() => f.view.apply()), /never replayed/); assert.equal(f.applied, 1);
  } finally { f.cleanup(); }
});

test('lost prepare and dispatch replies retain original identities without a second mutation', async () => {
  for (const kind of ['prepare', 'dispatch']) {
    const f = fixture();
    try {
      const source = f.doc('one.txt', 'before'); await f.draft(source, 'after');
      if (kind === 'prepare') f.losePrepare = true;
      else { await f.run(() => f.view.review()); f.loseDispatch = true; }
      await assert.rejects(f.run(() => kind === 'prepare' ? f.view.review() : f.view.apply()));
      const record = f.saved.find(record => record.kind === kind);
      assert.equal(record.phase, 'unknown'); assert.ok(record.change);
      if (kind === 'prepare') assert.equal(record.change, record.id);
      const reloaded = new EditorJournal(async () => {}, f.saved);
      await reloaded.reconcile(scope, f.client.call.bind(f.client));
      assert.equal(reloaded.records().find(row => row.id === record.id).phase, 'accepted');
      assert.equal(f.applied, 0); assert.equal(f.calls.filter(call => call.method === `editor/${kind}`).length, 1);
    } finally { f.cleanup(); }
  }
});

test('late prepare after mapping invalidation retains its ID without resurrecting previews or apply handles', async () => {
  const f = fixture();
  let release; let ready;
  const gate = new Promise(resolve => { release = resolve; }), entered = new Promise(resolve => { ready = resolve; });
  try {
    const source = f.doc('one.txt', 'private original'); await f.draft(source, 'private proposal');
    f.beforePrepareReply = async () => { ready(); await gate; };
    const reviewing = f.run(() => f.view.review()); await entered;
    f.view.invalidateMapping(); release();
    await assert.rejects(reviewing, /mapping changed/);
    assert.equal(f.previews.length, 0);
    assert.equal(f.saved.find(record => record.kind === 'prepare').phase, 'accepted');
    await f.run(() => f.view.apply()); assert.equal(f.applied, 0);
    assert.match(f.view.provideTextDocumentContent({ toString: () => 'vcp-editor-preview:old' }), /unavailable/);
  } finally { release(); f.cleanup(); }
});

test('late draft creation after same-connection rename invalidation cannot register a stale draft', async () => {
  const f = fixture();
  let release; let ready;
  const gate = new Promise(resolve => { release = resolve; }), entered = new Promise(resolve => { ready = resolve; });
  try {
    f.active = f.editor(f.doc('one.txt', 'original'));
    f.beforeOpenReply = async () => { ready(); await gate; };
    const creating = f.run(() => f.view.createDraft()); await entered;
    f.view.invalidateMapping(); release(); await assert.rejects(creating, /mapping changed/);
    await f.run(() => f.view.review()); assert.equal(f.calls.length, 0); assert.equal(f.previews.length, 0);
  } finally { release(); f.cleanup(); }
});

test('close retries only proven revision conflicts and unknown close outcomes reconcile without replay', async () => {
  for (const mode of ['conflict', 'unknown']) {
    const f = fixture();
    try {
      const source = f.doc('one.txt', 'before'); await f.draft(source, 'after'); await f.run(() => f.view.review());
      source.isClosed = true; f.view.invalidate(source);
      if (mode === 'conflict') f.closeConflicts = 1; else f.loseClose = true;
      await f.run(() => f.view.refreshObservations());
      const count = f.calls.filter(call => call.method === 'editor/context' && call.params.closed?.length).length;
      assert.equal(count, mode === 'conflict' ? 2 : 1);
      await f.run(() => f.view.refreshObservations());
      assert.equal(f.calls.filter(call => call.method === 'editor/context' && call.params.closed?.length).length, count);
      assert.equal(f.observed.size, 0);
      if (mode === 'unknown') assert.equal(f.calls.filter(call => call.method === 'command/read').length, 1);
    } finally { f.cleanup(); }
  }
});
