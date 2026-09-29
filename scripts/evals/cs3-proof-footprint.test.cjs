// SPDX-License-Identifier: Apache-2.0
'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
const Module = require('node:module'), child = require('node:child_process');
const { createProofReuse } = require('./cs3-proof-footprint.cjs');
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'cs3-footprint-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const file = path.join(root, 'evidence.txt'); fs.writeFileSync(file, 'retained-one');
  return { root, file, reuse: { run: createProofReuse() }, guard: () => {} };
}
function hooks() {
  return { read: fs.readFileSync, open: fs.openSync, readSync: fs.readSync, fstat: fs.fstatSync, opendir: fs.opendirSync, stat: fs.statSync,
    lstat: fs.lstatSync, readdir: fs.readdirSync, realpath: fs.realpathSync, realpathNative: fs.realpathSync.native,
    write: fs.writeFileSync, load: Module._load, exec: child.execFileSync, spawn: child.spawnSync };
}

test('reuse reruns guards and fresh byte checks while cloning the original JSON result', t => {
  const f = fixture(t); let calls = 0, guards = 0;
  const action = () => { calls++; return { text: fs.readFileSync(f.file, 'utf8'), nested: { retained: true } }; };
  const guard = () => { guards++; };
  const first = f.reuse.run('same-proof', action, guard); first.nested.retained = false;
  assert.deepEqual(f.reuse.run('same-proof', action, guard), { text: 'retained-one', nested: { retained: true } });
  assert.equal(calls, 1); assert(guards >= 2);
  fs.writeFileSync(f.file, 'retained-two');
  assert.throws(() => f.reuse.run('same-proof', action, guard));
  assert.equal(calls, 1, 'changed evidence must not replay the original proof action');
});

test('complete observed files and negative existence checks reject layered evidence drift', t => {
  for (const kind of ['top', 'nested', 'missing']) {
    const f = fixture(t), nested = path.join(f.root, 'nested'), absent = path.join(f.root, 'absent.json');
    fs.mkdirSync(nested); const inner = path.join(nested, 'source.txt'); fs.writeFileSync(inner, 'nested retained');
    const action = () => ({ top: fs.readFileSync(f.file, 'utf8'), nested: fs.readFileSync(inner, 'utf8'), absent: fs.existsSync(absent) });
    f.reuse.run(kind, action, f.guard);
    fs.writeFileSync(kind === 'top' ? f.file : kind === 'nested' ? inner : absent, 'changed');
    assert.throws(() => f.reuse.run(kind, action, f.guard));
  }
});

test('unobserved ancestor sibling changes are allowed but exact directory enumeration changes are not', t => {
  const f = fixture(t); let calls = 0;
  const action = () => { calls++; assert(fs.lstatSync(f.root).isDirectory()); return { text: fs.readFileSync(f.file, 'utf8') }; };
  f.reuse.run('parent-kind-only', action, f.guard);
  fs.writeFileSync(path.join(f.root, 'unrelated.txt'), 'unrelated');
  assert.deepEqual(f.reuse.run('parent-kind-only', action, f.guard), { text: 'retained-one' }); assert.equal(calls, 1);
  const enumerate = () => ({ names: fs.readdirSync(f.root).sort() });
  f.reuse.run('enumerated', enumerate, f.guard);
  fs.writeFileSync(path.join(f.root, 'another.txt'), 'another');
  assert.throws(() => f.reuse.run('enumerated', enumerate, f.guard));
});

test('observed stat properties, hardlinks and redirected parent chains cannot reuse stale proofs', t => {
  const f = fixture(t), extra = path.join(f.root, 'hardlink.txt');
  const action = () => { const stat = fs.lstatSync(f.file); assert(stat.isFile()); assert.equal(stat.nlink, 1); return { text: fs.readFileSync(f.file, 'utf8') }; };
  f.reuse.run('hardlink', action, f.guard); fs.linkSync(f.file, extra);
  assert.throws(() => f.reuse.run('hardlink', action, f.guard));
  const g = fixture(t), direct = path.join(g.root, 'direct'), renamed = path.join(g.root, 'retained-directory'), target = path.join(g.root, 'target');
  fs.mkdirSync(direct); fs.mkdirSync(target); fs.writeFileSync(path.join(direct, 'input.txt'), 'same'); fs.writeFileSync(path.join(target, 'input.txt'), 'same');
  const read = () => { assert.equal(fs.lstatSync(direct).isSymbolicLink(), false); return { text: fs.readFileSync(path.join(direct, 'input.txt'), 'utf8') }; };
  g.reuse.run('junction', read, g.guard); fs.renameSync(direct, renamed); fs.symlinkSync(target, direct, 'junction');
  assert.throws(() => g.reuse.run('junction', read, g.guard));
});

test('realpathSync.native stays callable and its resolved identity is observed', t => {
  const f = fixture(t), before = hooks();
  const action = () => ({ resolved: fs.realpathSync.native(f.file), text: fs.readFileSync(f.file, 'utf8') });
  const first = f.reuse.run('native-realpath', action, f.guard);
  assert.equal(first.resolved, fs.realpathSync.native(f.file)); assert.deepEqual(f.reuse.run('native-realpath', action, f.guard), first);
  assert.deepEqual(hooks(), before);
});

test('read-only positional and sequential descriptor observations bind actual bytes', t => {
  for (const mode of ['positional', 'sequential']) {
    const f = fixture(t);
    const action = () => {
      const fd = fs.openSync(f.file, 'r');
      try {
        const bytes = Buffer.alloc(8), count = fs.readSync(fd, bytes, 1, 4, mode === 'sequential' ? null : 2);
        const next = fs.readSync(fd, bytes, 5, 2, mode === 'sequential' ? null : 0);
        return { count, next, bytes: bytes.toString('hex') };
      } finally { fs.closeSync(fd); }
    };
    const first = f.reuse.run(mode, action, f.guard); assert.deepEqual(f.reuse.run(mode, action, f.guard), first);
    fs.writeFileSync(f.file, 'mutated-data'); assert.throws(() => f.reuse.run(mode, action, f.guard));
  }
});

test('opendirSync/readSync Dirent observations preserve exact entries and types', t => {
  const f = fixture(t);
  const action = () => {
    const directory = fs.opendirSync(f.root), entries = [];
    try { let item; while ((item = directory.readSync()) !== null) entries.push({ name: item.name, file: item.isFile(), directory: item.isDirectory() }); }
    finally { directory.closeSync(); }
    return { entries: entries.sort((a, b) => a.name.localeCompare(b.name)) };
  };
  const first = f.reuse.run('opendir', action, f.guard); assert.deepEqual(f.reuse.run('opendir', action, f.guard), first);
  fs.mkdirSync(path.join(f.root, 'extra-directory')); assert.throws(() => f.reuse.run('opendir', action, f.guard));
});

test('caller module integrity guards run on reuse and reject injected cached exports', t => {
  const f = fixture(t), moduleFile = path.join(f.root, 'module.cjs'); fs.writeFileSync(moduleFile, 'module.exports={answer:42};\n');
  const exports = require(moduleFile), originalModule = require.cache[moduleFile];
  t.after(() => { delete require.cache[moduleFile]; });
  const guard = () => { assert.equal(require.cache[moduleFile], originalModule); assert.equal(require.cache[moduleFile].exports, exports); };
  const action = () => { assert.equal(fs.readFileSync(moduleFile, 'utf8'), 'module.exports={answer:42};\n'); return { answer: require(moduleFile).answer }; };
  assert.equal(f.reuse.run('module', action, guard).answer, 42);
  require.cache[moduleFile].exports = { answer: 999 }; assert.throws(() => f.reuse.run('module', action, guard));
  require.cache[moduleFile].exports = exports;
});

test('environment changes cannot reuse a proof from another captured process context', t => {
  const f = fixture(t), key = 'VCP_CS3_FOOTPRINT_TEST_CONTEXT', before = process.env[key];
  t.after(() => { if (before === undefined) delete process.env[key]; else process.env[key] = before; });
  process.env[key] = 'first'; const action = () => ({ value: process.env[key], text: fs.readFileSync(f.file, 'utf8') });
  f.reuse.run('environment', action, f.guard); process.env[key] = 'second';
  assert.throws(() => f.reuse.run('environment', action, f.guard));
});

test('throws, async actions, unsupported filesystem and processes always restore hooks and cannot publish a proof', t => {
  const f = fixture(t), before = hooks(), marker = path.join(f.root, 'must-not-exist');
  for (const [key, action] of [
    ['throw', () => { fs.readFileSync(f.file); throw Error('synthetic proof failure'); }],
    ['async', async () => ({ text: fs.readFileSync(f.file, 'utf8') })],
    ['write', () => { fs.writeFileSync(marker, 'forbidden'); return {}; }],
    ['process', () => { child.spawnSync(process.execPath, ['--version']); return {}; }]
  ]) {
    assert.throws(() => f.reuse.run(key, action, f.guard)); assert.deepEqual(hooks(), before); assert(!fs.existsSync(marker));
  }
  const action = () => ({ text: fs.readFileSync(f.file, 'utf8') });
  assert.deepEqual(f.reuse.run('after-failure', action, f.guard), { text: 'retained-one' }); assert.deepEqual(hooks(), before);
});

test('caught unsupported filesystem/process operations and stat reflection cannot create reusable proofs', t => {
  for (const kind of ['open-options', 'read-options', 'git-options', 'stat-reflection', 'fd-readFile', 'unknown-fstat', 'nested-reuse', 'missing-open', 'missing-directory']) {
    const f = fixture(t), before = hooks();
    assert.deepEqual(f.reuse.run('baseline', () => ({ text: fs.readFileSync(f.file, 'utf8') }), f.guard), { text: 'retained-one' });
    const action = () => {
      try {
        if (kind === 'open-options') fs.openSync(f.file, 'r+');
        else if (kind === 'read-options') fs.readFileSync(f.file, { encoding: 'utf8', flag: 'r+' });
        else if (kind === 'git-options') child.execFileSync('git', ['--version'], { encoding: 'utf8' });
        else if (kind === 'stat-reflection') Object.getOwnPropertyDescriptor(fs.statSync(f.file), 'size').value;
        else if (kind === 'nested-reuse') f.reuse.run('inner', () => ({}), f.guard);
        else if (kind === 'missing-open') fs.openSync(path.join(f.root, 'missing'), 'r');
        else if (kind === 'missing-directory') fs.opendirSync(path.join(f.root, 'missing-directory'));
        else if (kind === 'unknown-fstat') fs.fstatSync(2147483647);
        else {
          const fd = fs.openSync(f.file, 'r');
          try { fs.readFileSync(fd); }
          finally { fs.closeSync(fd); }
        }
      } catch {}
      return { ignored: true };
    };
    assert.throws(() => f.reuse.run(kind, action, f.guard), /Historical proof footprint/); assert.deepEqual(hooks(), before);
  }
});

test('readdir encodings and Dirent type checks are covered without rewriting original values', t => {
  for (const options of [{ withFileTypes: true }, { encoding: 'buffer' }, { encoding: 'utf8' }]) {
    const f = fixture(t); const action = () => {
      const values = fs.readdirSync(f.root, options);
      return { values: values.map(value => typeof value === 'string' ? value : Buffer.isBuffer(value) ? value.toString('hex') : { name: value.name, file: value.isFile() }) };
    };
    const first = f.reuse.run('directory', action, f.guard); assert.deepEqual(f.reuse.run('directory', action, f.guard), first);
    fs.renameSync(f.file, path.join(f.root, 'renamed.txt')); assert.throws(() => f.reuse.run('directory', action, f.guard));
  }
});

test('unclosed file/directory handles and caught asynchronous filesystem APIs fail closed with hooks restored', t => {
  const f = fixture(t), before = hooks();
  assert.deepEqual(f.reuse.run('baseline', () => ({ text: fs.readFileSync(f.file, 'utf8') }), f.guard), { text: 'retained-one' });
  for (const [key, action] of [
    ['unclosed-file', () => { fs.openSync(f.file, 'r'); return {}; }],
    ['unclosed-directory', () => { fs.opendirSync(f.root); return {}; }],
    ['async-fs', () => { try { fs.readFile(f.file, () => {}); } catch {} return {}; }],
    ['promises-fs', () => { try { fs.promises.readFile(f.file); } catch {} return {}; }]
  ]) { assert.throws(() => f.reuse.run(key, action, f.guard), /Historical proof footprint/); assert.deepEqual(hooks(), before); }
});

test('loader substitution rejects reuse and explicit clear requires the real action again', t => {
  const f = fixture(t), loader = Module._load; let calls = 0;
  const action = () => { calls++; return { text: fs.readFileSync(f.file, 'utf8') }; };
  f.reuse.run('loader', action, f.guard);
  Module._load = function (...args) { return loader.apply(this, args); };
  try { assert.throws(() => f.reuse.run('loader', action, f.guard)); }
  finally { Module._load = loader; }
  assert.equal(calls, 1);
  f.reuse.run.clear(); fs.writeFileSync(f.file, 'freshly verified');
  assert.deepEqual(f.reuse.run('loader', action, f.guard), { text: 'freshly verified' }); assert.equal(calls, 2);
});

test('proof count is bounded and clear discards rather than supplies cached results', t => {
  const f = fixture(t), { limits } = require('./cs3-proof-footprint.cjs'); let calls = 0;
  const action = () => { calls++; return { text: fs.readFileSync(f.file, 'utf8') }; };
  for (let index = 0; index < limits.proofs; index++) f.reuse.run('proof-' + index, action, f.guard);
  assert.throws(() => f.reuse.run('over-bound', action, f.guard)); assert.equal(calls, limits.proofs);
  f.reuse.run.clear(); f.reuse.run('over-bound', action, f.guard); assert.equal(calls, limits.proofs + 1);
});

test('approved common-Git lookup is reobserved through an explicitly synthetic child transport', t => {
  const f = fixture(t), original = child.execFileSync; let calls = 0, output = path.join(f.root, 'common-control');
  child.execFileSync = (command, args, options) => {
    calls++; assert.equal(command, 'git'); assert.deepEqual(args, ['rev-parse', '--git-common-dir']);
    assert.equal(options.cwd, f.root); assert.equal(options.encoding, 'utf8'); return output;
  };
  try {
    let actions = 0;
    const action = () => { actions++; return { directory: child.execFileSync('git', ['rev-parse', '--git-common-dir'], { cwd: f.root, encoding: 'utf8', timeout: 1000 }) }; };
    assert.deepEqual(f.reuse.run('git', action, f.guard), { directory: output });
    const observed = calls; assert.deepEqual(f.reuse.run('git', action, f.guard), { directory: output });
    assert.equal(actions, 1); assert(calls > observed, 'cached proof must still reobserve Git ownership');
    output = path.join(f.root, 'different-control'); assert.throws(() => f.reuse.run('git', action, f.guard)); assert.equal(actions, 1);
  } finally { child.execFileSync = original; }
});

test('fixed aggregate footprint cap rejects overflow and caught overflow cannot publish a proof', t => {
  const production = require('./cs3-proof-footprint.cjs');
  assert.equal(production.limits.entries, 500000); assert(Object.isFrozen(production.limits));
  const filename = require.resolve('./cs3-proof-footprint.cjs'), source = fs.readFileSync(filename, 'utf8');
  const marker = 'entries: 500000,';
  assert.equal(source.split(marker).length, 2, 'test-only limit substitution must target exactly one fixed declaration');
  // Explicit synthetic module instance, never registered in require.cache or
  // used by acceptance. Only its entry cap is scaled down; real FS and the
  // complete production capture/reuse/poison implementation remain unchanged.
  const isolated = new Module(filename, module); isolated.filename = filename; isolated.paths = module.paths;
  isolated._compile(source.replace(marker, 'entries: 32,'), filename);
  const f = fixture(t), run = isolated.exports.createProofReuse(), before = hooks();
  assert.equal(isolated.exports.limits.entries, 32); assert.equal(require('./cs3-proof-footprint.cjs'), production);
  let safeCalls = 0, failedCalls = 0;
  const safe = () => {
    safeCalls++; const text = fs.readFileSync(f.file, 'utf8');
    for (let index = 0; index < 8; index++) assert.equal(fs.existsSync(path.join(f.root, 'missing-' + index)), false);
    return { text };
  };
  assert.deepEqual(run('within-cap', safe, f.guard), { text: 'retained-one' });
  assert.deepEqual(run('within-cap', safe, f.guard), { text: 'retained-one' }); assert.equal(safeCalls, 1);
  const overflow = () => {
    failedCalls++; fs.readFileSync(f.file, 'utf8');
    for (let index = 0; index < 32; index++) fs.existsSync(path.join(f.root, 'missing-' + index));
    return { incorrectly_accepted: true };
  };
  const check = error => {
    const match = /^Historical proof footprint: entry bound exceeded \(records=(\d+), files=(\d+), ancestors=(\d+), maximum=(\d+)\)$/.exec(error.message);
    assert(match, 'overflow diagnostic contains only the fixed label and aggregate counts');
    const [records, files, ancestors, maximum] = match.slice(1).map(Number);
    assert.equal(maximum, 32); assert.equal(records + files + ancestors, 33);
    assert(records > 0); assert.equal(files, 1); assert(ancestors > 0); return true;
  };
  assert.throws(() => run('overflow', overflow, f.guard), check); assert.deepEqual(hooks(), before);
  assert.throws(() => run('overflow', overflow, f.guard), check); assert.equal(failedCalls, 2, 'failed proof must never be reused');
  let caught = false;
  assert.throws(() => run('caught-overflow', () => {
    try { overflow(); } catch (error) { check(error); caught = true; }
    return { incorrectly_accepted: true };
  }, f.guard), /Historical proof footprint/);
  assert(caught); assert.deepEqual(hooks(), before);
  assert.deepEqual(run('overflow', safe, f.guard), { text: 'retained-one' }); assert.equal(safeCalls, 2);
});

test('tracked WEB-style fstat before and after positional reads binds file identity', t => {
  const f = fixture(t); let calls = 0;
  const action = () => {
    calls++; const before = fs.lstatSync(f.file), fd = fs.openSync(f.file, 'r');
    try {
      const opened = fs.fstatSync(fd); assert(opened.isFile());
      assert.equal(opened.size, before.size); assert.equal(opened.dev, before.dev); assert.equal(opened.ino, before.ino);
      const bytes = Buffer.alloc(opened.size); let offset = 0;
      while (offset < bytes.length) { const count = fs.readSync(fd, bytes, offset, bytes.length - offset, offset); assert(count > 0); offset += count; }
      const after = fs.fstatSync(fd); assert(after.isFile());
      for (const key of ['size', 'dev', 'ino']) assert.equal(after[key], opened[key]);
      return { text: bytes.toString('utf8'), size: after.size };
    } finally { fs.closeSync(fd); }
  };
  const first = f.reuse.run('web-fd', action, f.guard); assert.deepEqual(f.reuse.run('web-fd', action, f.guard), first); assert.equal(calls, 1);
  const replacement = path.join(f.root, 'replacement.txt'); fs.writeFileSync(replacement, 'retained-one');
  fs.unlinkSync(f.file); fs.renameSync(replacement, f.file);
  assert.throws(() => f.reuse.run('web-fd', action, f.guard)); assert.equal(calls, 1);
});

test('untracked and closed fstat descriptors reject even when the original action catches the failure', t => {
  for (const kind of ['external', 'closed']) {
    const f = fixture(t), before = hooks(), external = kind === 'external' ? fs.openSync(f.file, 'r') : null;
    try {
      assert.deepEqual(f.reuse.run('baseline', () => ({ text: fs.readFileSync(f.file, 'utf8') }), f.guard), { text: 'retained-one' });
      const action = () => {
        const fd = kind === 'external' ? external : fs.openSync(f.file, 'r');
        if (kind === 'closed') fs.closeSync(fd);
        try { fs.fstatSync(fd); } catch {}
        return { incorrectly_accepted: true };
      };
      assert.throws(() => f.reuse.run('unknown-fd', action, f.guard), /Historical proof footprint/); assert.deepEqual(hooks(), before);
    } finally { if (external !== null) fs.closeSync(external); }
  }
});

test('actual frozen WEB inventory inspection supports capture and reuse without browser execution', () => {
  const fixtures = require('./webapp-fixtures.cjs'), run = createProofReuse(); let calls = 0;
  const project = value => ({ revision: value.revision, manifest_sha256: value.manifest_sha256,
    cases: [...value.loaded.keys()], inventory: value.inventory });
  const expected = project(fixtures.inspect());
  const action = () => { calls++; return project(fixtures.inspect()); };
  assert.deepEqual(run('frozen-web', action, () => {}), expected);
  assert.deepEqual(run('frozen-web', action, () => {}), expected); assert.equal(calls, 1);
  assert.equal(expected.cases.length, 6); assert.equal(expected.manifest_sha256, fixtures.manifestSha256);
});

test('stat replay uses one fresh snapshot per record in each before and after validation', t => {
  for (const method of ['statSync', 'lstatSync', 'fstatSync']) {
    const f = fixture(t), original = fs[method], identity = fs.statSync(f.file); let reads = 0, actions = 0;
    // Explicit synthetic counting around the genuine native Stats result. The
    // action and helper still receive the original values and ordinary real IO.
    fs[method] = function (...args) {
      const value = original.apply(this, args);
      if (value.dev === identity.dev && value.ino === identity.ino) reads++;
      return value;
    };
    try {
      const checkpoints = [], guard = () => checkpoints.push(reads);
      const action = () => {
        actions++; const fd = method === 'fstatSync' ? fs.openSync(f.file, 'r') : null;
        try {
          const stat = fs[method](fd === null ? f.file : fd);
          return { dev: stat.dev, ino: stat.ino, mode: stat.mode, size: stat.size, nlink: stat.nlink,
            uid: stat.uid, gid: stat.gid, mtimeMs: stat.mtimeMs, file: stat.isFile(), directory: stat.isDirectory(), link: stat.isSymbolicLink() };
        } finally { if (fd !== null) fs.closeSync(fd); }
      };
      const first = f.reuse.run(method, action, guard); reads = 0; checkpoints.length = 0;
      assert.deepEqual(f.reuse.run(method, action, guard), first);
      assert.equal(actions, 1); assert.equal(reads, 2, 'one snapshot before and one snapshot after reuse');
      assert.deepEqual(checkpoints, [0, 1, 2], 'both validation passes and every guard remain present');
    } finally { fs[method] = original; }
  }
});

test('single-snapshot stat replay still rejects every observed base, property and method mutant', t => {
  const f = fixture(t), original = fs.statSync, identity = original(f.file); let mutation = null, calls = 0;
  const fields = ['dev', 'ino', 'mode', 'size', 'nlink', 'uid', 'gid', 'mtimeMs'];
  const methods = ['isFile', 'isDirectory', 'isSymbolicLink', 'isBlockDevice', 'isCharacterDevice', 'isFIFO', 'isSocket'];
  fs.statSync = function (...args) {
    const value = original.apply(this, args);
    if (value.dev === identity.dev && value.ino === identity.ino && mutation) {
      // Windows file IDs can exceed the exact-integer range: adding one might
      // round to the same Number, so use an unequivocally different value.
      const changed = methods.includes(mutation) ? !value[mutation]() : value[mutation] === 0 ? 1 : 0;
      Object.defineProperty(value, mutation, { value: methods.includes(mutation) ? () => changed : changed, configurable: true });
    }
    return value;
  };
  try {
    const action = () => {
      calls++; const stat = fs.statSync(f.file);
      return { fields: fields.map(key => stat[key]), methods: methods.map(key => stat[key]()) };
    };
    const first = f.reuse.run('all-stat-observations', action, f.guard);
    for (const name of [...fields, ...methods]) {
      mutation = name;
      assert.throws(() => f.reuse.run('all-stat-observations', action, f.guard), /Historical proof footprint/, name);
    }
    mutation = null;
    assert.deepEqual(f.reuse.run('all-stat-observations', action, f.guard), first); assert.equal(calls, 1);
  } finally { fs.statSync = original; }
});

test('read-only scopes validate each touched proof once before and after, with nested scopes joining', t => {
  const f = fixture(t), second = path.join(f.root, 'second.txt'), original = fs.statSync;
  fs.writeFileSync(second, 'second retained');
  const counts = new Map([[f.file, 0], [second, 0]]); let actions = 0, guards = 0;
  fs.statSync = function (file, ...args) {
    if (counts.has(file)) counts.set(file, counts.get(file) + 1);
    return original.call(this, file, ...args);
  };
  try {
    const action = file => () => { actions++; const stat = fs.statSync(file); return { size: stat.size, file: stat.isFile() }; };
    const first = action(f.file), other = action(second), guard = () => { guards++; };
    f.reuse.run('first', first, guard); f.reuse.run('second', other, guard);
    counts.set(f.file, 0); counts.set(second, 0); guards = 0;
    const result = f.reuse.run.readOnly(() => {
      const one = f.reuse.run('first', first, guard);
      f.reuse.run.readOnly(() => { f.reuse.run('first', first, guard); f.reuse.run('second', other, guard); });
      f.reuse.run('second', other, guard); f.reuse.run('first', first, guard);
      return { one, during: [...counts.values()] };
    });
    assert.deepEqual(result.during, [1, 1]); assert.deepEqual([...counts.values()], [2, 2]);
    assert.equal(actions, 2); assert(guards >= 5, 'every repeated access must invoke its guard');
    counts.set(f.file, 0); counts.set(second, 0);
    f.reuse.run.readOnly(() => f.reuse.run('first', first, guard));
    assert.deepEqual([...counts.values()], [2, 0], 'new top-level scope must start fresh validation');
    counts.set(f.file, 0); f.reuse.run('first', first, guard);
    assert.equal(counts.get(f.file), 2, 'default non-scoped validation remains before and after');
  } finally { fs.statSync = original; }
});

test('scope detects file drift before admission and between or after repeated proof reads before returning', t => {
  for (const when of ['before', 'between', 'end']) {
    const f = fixture(t), externalWrite = fs.writeFileSync, before = hooks(); let actions = 0;
    const action = () => { actions++; return { text: fs.readFileSync(f.file, 'utf8') }; };
    f.reuse.run('file', action, f.guard);
    // A retained original write function simulates an external actor changing
    // the file; ordinary writes through the scope's API are tested separately.
    if (when === 'before') externalWrite(f.file, 'outside mutation');
    assert.throws(() => f.reuse.run.readOnly(() => {
      f.reuse.run('file', action, f.guard);
      if (when === 'between') { externalWrite(f.file, 'between mutation'); f.reuse.run('file', action, f.guard); }
      if (when === 'end') externalWrite(f.file, 'exit mutation');
      return { must_not_escape_scope: true };
    }), /Historical proof footprint/);
    assert.equal(actions, 1); assert.deepEqual(hooks(), before);
  }
});

test('scope preserves directory and negative-existence observations through exit revalidation', t => {
  for (const observation of ['directory', 'missing']) {
    const f = fixture(t), absent = path.join(f.root, 'new-entry'), externalWrite = fs.writeFileSync;
    const action = () => observation === 'directory' ? { names: fs.readdirSync(f.root) } : { absent: fs.existsSync(absent) };
    f.reuse.run('observed', action, f.guard);
    assert.throws(() => f.reuse.run.readOnly(() => {
      f.reuse.run('observed', action, f.guard); externalWrite(absent, 'external change'); return {};
    }), /Historical proof footprint/);
  }
});

test('read-only scope denies caught writes, async IO, processes, clear and invalid proof calls', t => {
  for (const kind of ['write', 'async-io', 'process', 'clear', 'guard']) {
    const f = fixture(t), marker = path.join(f.root, 'never-created'), before = hooks();
    const action = () => ({ text: fs.readFileSync(f.file, 'utf8') });
    f.reuse.run('baseline', action, f.guard);
    assert.deepEqual(f.reuse.run.readOnly(() => f.reuse.run('baseline', action, f.guard)), { text: 'retained-one' });
    assert.throws(() => f.reuse.run.readOnly(() => {
      try {
        if (kind === 'write') fs.writeFileSync(marker, 'forbidden');
        else if (kind === 'async-io') fs.readFile(f.file, () => {});
        else if (kind === 'process') child.spawnSync(process.execPath, ['--version']);
        else if (kind === 'clear') f.reuse.run.clear();
        else f.reuse.run('baseline', action, () => { throw Error('synthetic module guard rejection'); });
      } catch {}
      return { must_not_escape_scope: true };
    }), /Historical proof footprint/);
    assert(!fs.existsSync(marker)); assert.deepEqual(hooks(), before);
  }
});

test('scope rejects async/thenable callbacks and restores hooks after original exceptions', t => {
  const f = fixture(t), before = hooks(), originalError = Error('synthetic scoped exception');
  assert.deepEqual(f.reuse.run.readOnly(() => ({ synchronous: true })), { synchronous: true });
  for (const action of [async () => ({}), () => Promise.resolve({}), () => ({ then() {} })]) {
    assert.throws(() => f.reuse.run.readOnly(action), /Historical proof footprint/); assert.deepEqual(hooks(), before);
  }
  assert.throws(() => f.reuse.run.readOnly(() => { throw originalError; }), error => error === originalError);
  assert.deepEqual(hooks(), before);
  const action = () => ({ text: fs.readFileSync(f.file, 'utf8') });
  assert.deepEqual(f.reuse.run.readOnly(() => f.reuse.run('after-throw', action, f.guard)), { text: 'retained-one' });
});

test('scope rejects caught write-capable read flags before touching original file bytes', t => {
  const f = fixture(t), before = hooks(), bytes = fs.readFileSync(f.file);
  for (const flag of ['w', 'a+']) {
    assert.throws(() => f.reuse.run.readOnly(() => {
      try { fs.readFileSync(f.file, { encoding: 'utf8', flag }); } catch {}
      return { must_not_escape_scope: true };
    }), /Historical proof footprint/);
    assert.deepEqual(fs.readFileSync(f.file), bytes); assert.deepEqual(hooks(), before);
  }
  assert.equal(f.reuse.run.readOnly(() => fs.readFileSync(f.file, { encoding: 'utf8', flag: 'r' })), 'retained-one');
});

test('scope rejects filesystem/module/environment substitution and restores its hooks', t => {
  for (const kind of ['filesystem', 'module', 'environment']) {
    const f = fixture(t), before = hooks(), key = 'VCP_CS3_SCOPE_TEST_CONTEXT', prior = process.env[key];
    const action = () => ({ text: fs.readFileSync(f.file, 'utf8') });
    f.reuse.run('proof', action, f.guard);
    try {
      assert.throws(() => f.reuse.run.readOnly(() => {
        f.reuse.run('proof', action, f.guard);
        if (kind === 'filesystem') fs.readFileSync = before.read;
        else if (kind === 'module') Module._load = function (...args) { return before.load.apply(this, args); };
        else process.env[key] = 'changed-inside-scope';
        return {};
      }), /Historical proof footprint/);
    } finally {
      Module._load = before.load;
      if (prior === undefined) delete process.env[key]; else process.env[key] = prior;
    }
    assert.deepEqual(hooks(), before);
  }
});
