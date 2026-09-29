// SPDX-License-Identifier: Apache-2.0
'use strict';
// Private, process-local reuse of an already executed synchronous historical
// proof. No first-read memoization, persisted cache, or supplied proof is accepted.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const cp = require('node:child_process'), Module = require('node:module');
const { fileURLToPath } = require('node:url'), { isDeepStrictEqual: equal } = require('node:util');
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const limits = Object.freeze({ entries: 500000, fileBytes: 1024 * 1024 * 1024, totalBytes: 4 * 1024 * 1024 * 1024, proofs: 8 });
let capturing = false, poison = null;
function need(value, message) { if (!value) { if (poison) poison(message); const error = Error('Historical proof footprint: ' + message); error.code = 'CS3_PROOF_FOOTPRINT'; throw error; } }
function clone(value) { return structuredClone(value); }
function filename(value) {
  if (value instanceof URL) value = fileURLToPath(value);
  if (Buffer.isBuffer(value)) value = value.toString('utf8');
  need(typeof value === 'string' && value.length > 0 && !value.includes('\0'), 'unsupported path');
  return path.resolve(value);
}
function errorShape(error) {
  return { code: error.code || null, name: error.name, message: error.message, errno: error.errno ?? null, syscall: error.syscall || null,
    path: error.path ?? null, dest: error.dest ?? null, status: error.status ?? null, signal: error.signal ?? null,
    stdout: error.stdout === undefined ? null : valueShape(error.stdout), stderr: error.stderr === undefined ? null : valueShape(error.stderr) };
}
function observe(action, project = value => value) {
  try { return { ok: true, value: project(action()) }; }
  catch (error) { return { ok: false, error: errorShape(error) }; }
}
function dirent(value) {
  if (value === null) return null;
  if (typeof value === 'string') return value;
  if (Buffer.isBuffer(value)) return { buffer: value.toString('hex') };
  return { name: Buffer.isBuffer(value.name) ? { buffer: value.name.toString('hex') } : value.name,
    parentPath: value.parentPath, path: value.path,
    kinds: ['isFile', 'isDirectory', 'isSymbolicLink', 'isBlockDevice', 'isCharacterDevice', 'isFIFO', 'isSocket'].map(key => value[key]()) };
}
function valueShape(value) {
  if (Buffer.isBuffer(value)) return { bytes: value.length, sha256: hash(value) };
  if (Array.isArray(value)) return value.map(dirent);
  return value;
}
function environment(original) {
  // Only a digest is retained: environment values may include credentials.
  return hash(JSON.stringify({ cwd: process.cwd(), env: Object.entries(process.env).sort(([a], [b]) => a.localeCompare(b)),
    execPath: process.execPath, argv: process.execArgv, version: process.version, versions: process.versions,
    platform: process.platform, arch: process.arch, executable: hash(original.readFileSync(process.execPath)) }));
}
function gitArguments(command, args, options) {
  need(command === 'git' && Array.isArray(args) && options && typeof options === 'object'
    && options.encoding === 'utf8' && !options.shell && !options.input && !options.env
    && Object.keys(options).every(key => ['cwd', 'encoding', 'windowsHide', 'timeout'].includes(key)), 'unsupported child process');
  const simple = equal(args, ['rev-parse', '--git-common-dir']) && path.isAbsolute(options.cwd || '');
  const absolute = args.length === 7 && args[0] === '-c' && args[1].startsWith('safe.directory=')
    && args[2] === '-C' && path.isAbsolute(args[3]) && args[1] === 'safe.directory=' + args[3]
    && equal(args.slice(4), ['rev-parse', '--path-format=absolute', '--git-common-dir']);
  need(simple || absolute, 'only exact read-only common-Git lookup is allowed');
}
function capture(action) {
  need(!capturing, 'nested capture is unsupported');
  const original = Object.fromEntries(Object.keys(fs).filter(key => typeof fs[key] === 'function').map(key => [key, fs[key]]));
  const nativeRealpath = fs.realpathSync.native, originalCp = Object.fromEntries(Object.keys(cp).filter(key => typeof cp[key] === 'function').map(key => [key, cp[key]]));
  const records = new Map(), files = new Map(), ancestors = new Map(), handles = new Map(), directories = new Set(), restores = [], installed = [];
  const promiseMethods = Object.fromEntries(Object.keys(fs.promises).filter(key => typeof fs.promises[key] === 'function').map(key => [key, fs.promises[key]]));
  let clockFloor = Date.now();
  const originalHash = crypto.createHash, originalClock = Date.now;
  let depth = 0, bytes = 0, violated = null;
  const raw = action => { depth++; try { return action(); } finally { depth--; } };
  const deny = label => { violated ||= 'unsupported operation ' + label; throw Error('Historical proof footprint: ' + violated); };
  const bounded = () => need(records.size + files.size + ancestors.size <= limits.entries,
    'entry bound exceeded (records=' + records.size + ', files=' + files.size + ', ancestors=' + ancestors.size + ', maximum=' + limits.entries + ')');
  const add = (key, result, check) => {
    const prior = records.get(key);
    if (prior) need(equal(prior.result, result), 'observation changed during original proof');
    else { records.set(key, { result: clone(result), check }); bounded(); }
    return records.get(key);
  };
  function ancestorsOf(file) {
    for (let current = path.dirname(file); ; current = path.dirname(current)) {
      if (!ancestors.has(current)) {
        const check = () => observe(() => original.lstatSync(current), stat => ({ dev: stat.dev, ino: stat.ino, mode: stat.mode, link: stat.isSymbolicLink(), directory: stat.isDirectory() }));
        const result = raw(check);
        need(result.ok && !result.value.link && result.value.directory, 'linked or missing ancestor');
        ancestors.set(current, { result, check }); bounded();
      }
      if (path.dirname(current) === current) break;
    }
  }
  function fileSnapshot(file) {
    ancestorsOf(file);
    const stat = original.lstatSync(file);
    need(stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1 && stat.size <= limits.fileBytes, 'nonordinary or oversized file');
    const data = original.readFileSync(file), after = original.lstatSync(file);
    const identity = info => ({ dev: info.dev, ino: info.ino, mode: info.mode, nlink: info.nlink, size: info.size, mtimeMs: info.mtimeMs, ctimeMs: info.ctimeMs });
    need(equal(identity(stat), identity(after)) && data.length === stat.size, 'file changed while reading');
    return { identity: identity(stat), sha256: hash(data), bytes: data.length };
  }
  function fileObserved(file) {
    const result = raw(() => fileSnapshot(file)), prior = files.get(file);
    if (prior) need(equal(prior.result, result), 'file changed during original proof');
    else { bytes += result.bytes; need(bytes <= limits.totalBytes, 'total byte bound exceeded'); files.set(file, { result, check: () => fileSnapshot(file) }); bounded(); }
  }
  function ordinary(name, args) {
    const file = filename(args[0]), copied = [file, ...clone(args.slice(1))];
    need(args.length >= 1 && args.length <= (name === 'existsSync' ? 1 : 2), 'unsupported filesystem arguments');
    if (name === 'readFileSync') {
      const options = copied[1];
      need(options === undefined || typeof options === 'string' || options && typeof options === 'object'
        && Object.keys(options).every(key => ['encoding', 'flag'].includes(key)) && (!options.flag || options.flag === 'r'), 'unsupported file read options');
      const info = raw(() => observe(() => original.lstatSync(file), value => value.size));
      need(!info.ok || info.value <= limits.fileBytes, 'file byte bound exceeded');
    } else if (name !== 'existsSync') {
      const options = copied[1], allowed = name === 'readdirSync' ? ['encoding', 'withFileTypes'] : ['encoding'];
      need(options === undefined || typeof options === 'string' || options && typeof options === 'object'
        && Object.keys(options).every(key => allowed.includes(key)), 'unsupported directory or realpath options');
    }
    const operation = name === 'realpathNative' ? nativeRealpath : original[name];
    let output, failure;
    try { output = raw(() => operation(...copied)); } catch (error) { failure = error; }
    need(!Array.isArray(output) || output.length <= limits.entries, 'directory entry bound exceeded');
    const result = failure ? { ok: false, error: errorShape(failure) } : { ok: true, value: valueShape(output) };
    add(JSON.stringify([name, copied]), result, () => observe(() => operation(...copied), valueShape));
    if (!failure && name === 'readFileSync') fileObserved(file);
    if (failure) throw failure;
    return output;
  }
  function stat(name, args) {
    const copied = [filename(args[0]), ...clone(args.slice(1))];
    need(args.length >= 1 && args.length <= 2 && (copied[1] === undefined || copied[1] && typeof copied[1] === 'object'
      && Object.keys(copied[1]).every(key => ['bigint', 'throwIfNoEntry'].includes(key))), 'unsupported stat options');
    let output, failure;
    try { output = raw(() => original[name](...copied)); } catch (error) { failure = error; }
    if (failure || output === undefined) {
      add(JSON.stringify([name, copied]), failure ? { ok: false, error: errorShape(failure) } : { ok: true, value: undefined }, () => observe(() => original[name](...copied)));
      if (failure) throw failure;
      return output;
    }
    return trackedStat(JSON.stringify([name, copied]), output, () => original[name](...copied));
  }
  function trackedStat(key, output, currentStat) {
    const base = { dev: output.dev, ino: output.ino, mode: output.mode };
    const record = add(key, { ok: true, value: base }, () => observe(currentStat, value => ({ dev: value.dev, ino: value.ino, mode: value.mode })));
    record.currentStat ||= currentStat;
    record.properties ||= new Map();
    record.descriptors ||= new Map();
    const property = (name, call, value) => {
      const key = (call ? 'call:' : 'get:') + name;
      if (record.properties.has(key)) need(equal(record.properties.get(key).result, value), 'stat property changed during proof');
      else record.properties.set(key, { name, call, result: clone(value) });
      return value;
    };
    return new Proxy(output, { get(target, key) {
      need(typeof key === 'string', 'unsupported stat reflection');
      const value = target[key];
      if (typeof value === 'function') {
        need(['isFile', 'isDirectory', 'isSymbolicLink', 'isBlockDevice', 'isCharacterDevice', 'isFIFO', 'isSocket'].includes(key), 'unsupported stat method');
        return (...args) => { need(args.length === 0, 'stat method arguments'); return property(key, true, value.call(target)); };
      }
      return property(key, false, value);
    }, set() { return deny('stat mutation'); }, defineProperty() { return deny('stat mutation'); }, deleteProperty() { return deny('stat mutation'); },
    ownKeys() { return deny('stat reflection'); },
    getOwnPropertyDescriptor(target, key) {
      // A preserved validator may wrap this Stats proxy in its own Stats
      // proxy. JavaScript then queries target descriptors to check proxy
      // invariants even for ordinary `stat.size` / `stat.isFile()` reads.
      // Authenticate those read-only data descriptors rather than bypassing
      // either validator. Prototype/enumeration/mutation remain unsupported.
      need(typeof key === 'string' && key.length <= 128, 'unsupported stat descriptor key');
      const descriptor = Object.getOwnPropertyDescriptor(target, key);
      need(descriptor === undefined || Object.hasOwn(descriptor, 'value')
        && (descriptor.value === null || ['undefined', 'number', 'bigint', 'string', 'boolean'].includes(typeof descriptor.value)),
      'unsupported stat accessor or object descriptor');
      if (record.descriptors.has(key)) need(equal(record.descriptors.get(key), descriptor), 'stat descriptor changed during proof');
      else {
        need(record.descriptors.size < 64, 'stat descriptor bound exceeded');
        record.descriptors.set(key, clone(descriptor));
      }
      return descriptor;
    }, getPrototypeOf() { return deny('stat reflection'); } });
  }
  function open(args) {
    const [value, flags, mode] = args, file = filename(value);
    need((flags === 'r' || flags === (fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0))) && mode === undefined, 'write-capable or unsupported open');
    need(handles.size + directories.size < 256, 'open handle bound exceeded');
    fileObserved(file);
    const fd = raw(() => original.openSync(file, flags));
    const opened = raw(() => original.fstatSync(fd)), expected = files.get(file).result.identity;
    if (opened.dev !== expected.dev || opened.ino !== expected.ino || opened.nlink !== 1 || !opened.isFile()) {
      raw(() => original.closeSync(fd)); return deny('opened file identity changed');
    }
    handles.set(fd, { file, position: 0 }); return fd;
  }
  function readFd(args) {
    const [fd, buffer, offset, length, position] = args, handle = handles.get(fd);
    need(handle && Buffer.isBuffer(buffer) && Number.isSafeInteger(offset) && Number.isSafeInteger(length)
      && (position === null || Number.isSafeInteger(position) && position >= 0), 'unsupported descriptor read');
    const at = position === null ? handle.position : position, count = raw(() => original.readSync(...args));
    const check = () => {
      const fd = original.openSync(handle.file, 'r');
      try { const target = Buffer.alloc(length), count = original.readSync(fd, target, 0, length, at); return { count, sha256: hash(target.subarray(0, count)) }; }
      finally { original.closeSync(fd); }
    };
    add(JSON.stringify(['readSync', handle.file, at, length]), { count, sha256: hash(buffer.subarray(offset, offset + count)) }, check);
    if (position === null) handle.position += count;
    return count;
  }
  function statFd(args) {
    const [fd, options] = args, handle = handles.get(fd);
    need(handle && args.length >= 1 && args.length <= 2 && (options === undefined || options && typeof options === 'object'
      && Object.keys(options).every(key => key === 'bigint')), 'unknown descriptor or unsupported fstat options');
    const copied = clone(options), output = raw(() => original.fstatSync(fd, copied)), expected = files.get(handle.file).result.identity;
    const currentStat = () => {
      const current = original.openSync(handle.file, 'r');
      try {
        const value = original.fstatSync(current, copied);
        need(String(value.dev) === String(expected.dev) && String(value.ino) === String(expected.ino)
          && String(value.nlink) === '1' && value.isFile(), 'reopened descriptor identity changed');
        return value;
      } finally { original.closeSync(current); }
    };
    // The descriptor number is intentionally absent from the footprint: replay
    // reopens the authenticated path and checks the original opened identity.
    return trackedStat(JSON.stringify(['fstatSync', handle.file, copied]), output, currentStat);
  }
  function openDirectory(args) {
    need(args.length === 1, 'unsupported directory options');
    need(handles.size + directories.size < 256, 'open handle bound exceeded');
    const file = filename(args[0]), directory = raw(() => original.opendirSync(file)), sequence = [];
    directories.add(directory);
    return { path: directory.path, readSync() {
      need(directories.has(directory), 'closed directory read');
      need(sequence.length < limits.entries, 'directory entry bound exceeded');
      let entry;
      try { entry = raw(() => directory.readSync()); } catch (error) { violated ||= 'directory read failed'; throw error; }
      sequence.push(dirent(entry)); return entry;
    }, closeSync() {
      need(directories.has(directory), 'closed directory close');
      try { raw(() => directory.closeSync()); } catch (error) { violated ||= 'directory close failed'; throw error; }
      directories.delete(directory);
      const check = () => {
        const current = original.opendirSync(file);
        try { return sequence.map(() => dirent(current.readSync())); } finally { current.closeSync(); }
      };
      add(JSON.stringify(['opendirSync', file, sequence.length]), sequence, check);
    }, read() { return deny('Dir.read'); }, close() { return deny('Dir.close'); }, [Symbol.asyncIterator]() { return deny('Dir async iteration'); } };
  }
  function install(object, key, replacement) {
    const descriptor = Object.getOwnPropertyDescriptor(object, key);
    need(descriptor && (descriptor.writable || descriptor.configurable), 'unhookable API');
    Object.defineProperty(object, key, { configurable: descriptor.configurable, enumerable: descriptor.enumerable, writable: true, value: replacement });
    installed.push(() => object[key] === replacement);
    restores.push(() => Object.defineProperty(object, key, descriptor));
  }
  const supported = {
    existsSync: args => ordinary('existsSync', args), readFileSync: args => ordinary('readFileSync', args),
    readdirSync: args => ordinary('readdirSync', args), realpathSync: args => ordinary('realpathSync', args),
    lstatSync: args => stat('lstatSync', args), statSync: args => stat('statSync', args), openSync: open, readSync: readFd, fstatSync: statFd,
    closeSync(args) { need(args.length === 1 && handles.has(args[0]), 'unknown descriptor close'); const value = raw(() => original.closeSync(args[0])); handles.delete(args[0]); return value; },
    opendirSync: openDirectory,
  };
  const baseline = environment(original);
  capturing = true;
  poison = message => { violated ||= message; };
  try {
    // Preserve real clock reads and retain only their high-water mark. The
    // audited historical validation path disables live expiration checks.
    install(Date, 'now', () => { const now = originalClock(); clockFloor = Math.max(clockFloor, now); return now; });
    for (const [name, method] of Object.entries(original)) install(fs, name, function (...args) {
      if (depth) return method.apply(this, args);
      try { return supported[name] ? supported[name](args) : deny('fs.' + name); }
      catch (error) {
        if (error.code === 'CS3_PROOF_FOOTPRINT' || ['openSync', 'readSync', 'fstatSync', 'closeSync', 'opendirSync'].includes(name)) violated ||= error.message;
        throw error;
      }
    });
    fs.realpathSync.native = (...args) => depth ? nativeRealpath(...args) : ordinary('realpathNative', args);
    const wrappedNative = fs.realpathSync.native;
    installed.push(() => fs.realpathSync.native === wrappedNative);
    for (const name of Object.keys(fs.promises)) if (typeof fs.promises[name] === 'function') install(fs.promises, name, () => deny('fs.promises.' + name));
    for (const [name, method] of Object.entries(originalCp)) install(cp, name, function (...args) {
      if (depth) return method.apply(this, args);
      if (name !== 'execFileSync') return deny('child_process.' + name);
      try { gitArguments(...args); } catch (error) { violated ||= error.message; throw error; }
      const copied = clone(args);
      let output, failure;
      try { output = raw(() => method(...args)); } catch (error) { failure = error; }
      add(JSON.stringify(['git', copied]), failure ? { ok: false, error: errorShape(failure) } : { ok: true, value: valueShape(output) },
        () => observe(() => method(...copied), valueShape));
      if (failure) throw failure; return output;
    });
    need(action.constructor?.name !== 'AsyncFunction', 'asynchronous proof is unsupported');
    const value = action();
    need(!value || typeof value.then !== 'function', 'asynchronous proof is unsupported');
    need(!violated && !handles.size && !directories.size && installed.every(check => check()), 'unsupported operation, changed hook or unclosed read handle');
    const proof = clone(value);
    clockFloor = Math.max(clockFloor, originalClock());
    // Restore hooks before replay: replay is fresh real IO, never memoized IO.
    for (const restore of restores.splice(0).reverse()) restore();
    capturing = false;
    poison = null;
    function validate() {
      need(Object.entries(original).every(([key, value]) => fs[key] === value) && fs.realpathSync.native === nativeRealpath
        && Object.entries(originalCp).every(([key, value]) => cp[key] === value)
        && Object.entries(promiseMethods).every(([key, value]) => fs.promises[key] === value)
        && crypto.createHash === originalHash && Date.now === originalClock, 'filesystem, hashing, clock or process API changed');
      // Clock reversal is rejected defensively; current live qualification and
      // expiration checks are deliberately outside this historical proof reuse.
      need(Date.now() >= clockFloor, 'clock moved before original proof');
      need(environment(original) === baseline, 'environment, cwd or runtime changed');
      for (const record of ancestors.values()) need(equal(record.check(), record.result), 'ancestor identity changed');
      for (const record of files.values()) need(equal(record.check(), record.result), 'file bytes or identity changed');
      for (const record of records.values()) {
        if (record.currentStat) {
          // One fresh snapshot per stat record and replay. All properties the
          // original proof observed are compared against this same snapshot;
          // no snapshot survives this validation pass or replaces byte checks.
          const current = observe(() => {
            const snapshot = record.currentStat();
            return { snapshot, base: { dev: snapshot.dev, ino: snapshot.ino, mode: snapshot.mode } };
          });
          need(current.ok && equal({ ok: true, value: current.value.base }, record.result), 'observed filesystem or Git state changed');
          for (const property of record.properties.values()) {
            const snapshot = current.value.snapshot, value = property.call ? snapshot[property.name]() : snapshot[property.name];
            need(equal(value, property.result), 'observed stat property changed');
          }
          for (const [name, descriptor] of record.descriptors)
            need(equal(Object.getOwnPropertyDescriptor(current.value.snapshot, name), descriptor), 'observed stat descriptor changed');
        } else need(equal(record.check(), record.result), 'observed filesystem or Git state changed');
      }
      need(environment(original) === baseline, 'environment changed during verification');
    }
    validate();
    return { proof, validate };
  } finally {
    for (const restore of restores.reverse()) restore();
    for (const fd of handles.keys()) try { original.closeSync(fd); } catch {}
    for (const directory of directories) try { directory.closeSync(); } catch {}
    capturing = false;
    poison = null;
  }
}
// This is a guard for audited synchronous validator bodies, not a sandbox for
// arbitrary JavaScript (a caller may already hold a prebound native function).
// No scope may encompass claims, dispatch, or an asynchronous continuation.
let activeReadOnly = null;
function readOnlyScope() {
  const hooks = [], touched = new Map();
  const loader = Module._load, createHash = crypto.createHash, clock = Date.now;
  let violated = null, depth = 0;
  const fail = label => { violated ||= label; throw Error('Historical proof footprint: ' + violated); };
  const mark = label => { violated ||= label; };
  const readonly = new Set(['existsSync', 'readFileSync', 'readdirSync', 'realpathSync', 'lstatSync', 'statSync',
    'openSync', 'readSync', 'fstatSync', 'closeSync', 'opendirSync']);
  function hook(object, key, replacement) {
    const descriptor = Object.getOwnPropertyDescriptor(object, key);
    need(descriptor && (descriptor.writable || descriptor.configurable), 'unhookable scope API');
    hooks.push({ object, key, descriptor, replacement });
  }
  const nativeRealpath = fs.realpathSync.native;
  for (const key of Object.keys(fs)) if (typeof fs[key] === 'function') {
    const original = fs[key];
    const wrapped = function (...args) {
      if (depth) return original.apply(this, args);
      if (!readonly.has(key)) return fail('unsupported read-only scope operation fs.' + key);
      if (key === 'readFileSync') {
        let options;
        try { options = clone(args[1]); } catch { return fail('unsupported scope file read options'); }
        if (!(args.length >= 1 && args.length <= 2 && (options === undefined || typeof options === 'string'
          || options && typeof options === 'object' && Object.keys(options).every(name => ['encoding', 'flag'].includes(name))
          && (options.flag === undefined || options.flag === 'r')))) return fail('write-capable or unsupported scope file read');
        args = [args[0], options];
      }
      if (key === 'openSync' && !(args.length === 2 && (args[1] === 'r' || args[1] === fs.constants.O_RDONLY
        || args[1] === (fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0))))) return fail('write-capable scope open');
      depth++;
      try {
        const result = original.apply(this, args);
        if (key !== 'opendirSync') return result;
        return { path: result.path, readSync: () => result.readSync(), closeSync: () => result.closeSync(),
          read: () => fail('asynchronous scope directory read'), close: () => fail('asynchronous scope directory close'),
          [Symbol.asyncIterator]: () => fail('asynchronous scope directory iteration') };
      } finally { depth--; }
    };
    if (key === 'realpathSync') wrapped.native = (...args) => nativeRealpath(...args);
    hook(fs, key, wrapped);
  }
  for (const key of Object.keys(fs.promises)) if (typeof fs.promises[key] === 'function')
    hook(fs.promises, key, () => fail('unsupported read-only scope operation fs.promises.' + key));
  for (const key of Object.keys(cp)) if (typeof cp[key] === 'function') {
    const original = cp[key];
    hook(cp, key, function (...args) {
      if (depth) return original.apply(this, args);
      if (key !== 'execFileSync') return fail('unsupported read-only scope process');
      try { gitArguments(...args); } catch (error) { mark(error.message); throw error; }
      depth++; try { return original.apply(this, args); } finally { depth--; }
    });
  }
  const realpathHook = hooks.find(item => item.object === fs && item.key === 'realpathSync');
  const wrappedNative = realpathHook.replacement.native;
  const install = () => { for (const item of hooks) Object.defineProperty(item.object, item.key,
    { configurable: item.descriptor.configurable, enumerable: item.descriptor.enumerable, writable: true, value: item.replacement }); };
  const restore = () => { for (const item of [...hooks].reverse()) Object.defineProperty(item.object, item.key, item.descriptor); };
  const check = () => {
    if (violated) return fail(violated);
    if (!hooks.every(item => item.object[item.key] === item.replacement) || fs.realpathSync.native !== wrappedNative)
      return fail('read-only scope API changed');
    if (Module._load !== loader || crypto.createHash !== createHash || Date.now !== clock)
      return fail('read-only scope module, hashing or clock API changed');
  };
  const canonical = () => hooks.every(item => {
    const descriptor = Object.getOwnPropertyDescriptor(item.object, item.key);
    return equal(descriptor, item.descriptor);
  }) && fs.realpathSync.native === nativeRealpath;
  install();
  return { touched, mark, check, restore,
    suspend(action) {
      check(); restore(); const priorPoison = poison; poison = null;
      try { return action(); }
      catch (error) { mark(error.message); throw error; }
      finally {
        const unchanged = canonical(); install(); poison = priorPoison;
        if (!unchanged) fail('canonical API changed during historical proof');
      }
    } };
}
function createProofReuse() {
  const proofs = new Map();
  const invoke = function (key, action, guard) {
    need(typeof key === 'string' && key.length <= 65536 && typeof action === 'function' && typeof guard === 'function', 'invalid private proof call');
    need(!capturing, 'nested reuse is unsupported');
    const scope = activeReadOnly;
    const execute = action => scope ? scope.suspend(action) : action();
    guard();
    let entry = proofs.get(key);
    if (!entry) {
      need(proofs.size < limits.proofs, 'proof bound exceeded');
      const loader = Module._load;
      entry = { ...execute(() => capture(action)), loader }; guard();
      need(Module._load === loader, 'module loader changed'); proofs.set(key, entry);
    } else {
      need(Module._load === entry.loader, 'module loader changed');
      if (!scope || !scope.touched.has(entry)) execute(() => entry.validate());
      guard();
    }
    const result = clone(entry.proof);
    if (scope) scope.touched.set(entry, guard);
    else entry.validate();
    guard();
    need(Module._load === entry.loader, 'module loader changed');
    return result;
  };
  const run = (...args) => {
    try { return invoke(...args); }
    catch (error) { if (activeReadOnly) activeReadOnly.mark(error.message); throw error; }
  };
  // Discard only. Authorized additive retirement barriers use this before a
  // completely new genuine proof; unexpected drift never clears itself.
  run.clear = () => { need(!capturing && !activeReadOnly, 'cannot invalidate an active proof or read-only scope'); proofs.clear(); };
  run.readOnly = action => {
    need(typeof action === 'function' && action.constructor?.name !== 'AsyncFunction' && !capturing,
      'read-only scope must be synchronous and outside capture');
    if (activeReadOnly) {
      try {
        const result = action();
        need(!result || typeof result.then !== 'function', 'asynchronous read-only scope is unsupported');
        activeReadOnly.check(); return result;
      } catch (error) { activeReadOnly.mark(error.message); throw error; }
    }
    const scope = readOnlyScope(), priorPoison = poison;
    activeReadOnly = scope; poison = scope.mark;
    try {
      const result = action();
      need(!result || typeof result.then !== 'function', 'asynchronous read-only scope is unsupported');
      scope.check();
      for (const [entry, guard] of scope.touched) {
        need(Module._load === entry.loader, 'module loader changed');
        scope.suspend(() => entry.validate()); guard();
        need(Module._load === entry.loader, 'module loader changed');
      }
      scope.check(); return result;
    } finally { scope.restore(); activeReadOnly = null; poison = priorPoison; }
  };
  return run;
}
module.exports = { createProofReuse, limits };
