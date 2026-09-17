// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');

class FakeClock {
  #now = 0;
  #next = 0;
  #timers = new Map();
  now() { return this.#now; }
  schedule(delay, callback) {
    if (!Number.isSafeInteger(delay) || delay < 0 || !Number.isSafeInteger(this.#now + delay)) throw Error('Invalid delay');
    const id = ++this.#next;
    this.#timers.set(id, { at: this.#now + delay, callback });
    return () => this.#timers.delete(id);
  }
  advance(milliseconds) {
    if (!Number.isSafeInteger(milliseconds) || milliseconds < 0 || !Number.isSafeInteger(this.#now + milliseconds)) throw Error('Invalid advance');
    const target = this.#now + milliseconds;
    let callbacks = 0;
    while (true) {
      const next = [...this.#timers.entries()].filter(([, timer]) => timer.at <= target)
        .sort((a, b) => a[1].at - b[1].at || a[0] - b[0])[0];
      if (!next) break;
      if (++callbacks > 10000) throw Error('Non-quiescent fake clock');
      this.#timers.delete(next[0]); this.#now = next[1].at; next[1].callback();
    }
    this.#now = target;
  }
}

class ScriptedProvider {
  #script;
  #index = 0;
  constructor(script) { this.#script = structuredClone(script); }
  request(input) {
    const step = this.#script[this.#index];
    if (!step) throw Error('Unexpected model request after script exhaustion');
    // Do not include mismatched request contents in diagnostics.
    try { assert.deepEqual(input, step.expect); }
    catch { throw Error('Unexpected scripted model request at step ' + this.#index); }
    this.#index++;
    if (step.error) throw Error(step.error);
    return structuredClone(step.response);
  }
  assertConsumed() { assert.equal(this.#index, this.#script.length, 'Unconsumed scripted model responses'); }
  get requests() { return this.#index; }
}

function ownedRoot(parent) {
  const base = fs.realpathSync(parent);
  const container = fs.mkdtempSync(path.join(base, 'vcp-fixture-'));
  const root = path.join(container, 'workspace');
  fs.mkdirSync(root);
  const token = crypto.randomUUID();
  const marker = path.join(container, 'owner.json');
  fs.writeFileSync(marker, JSON.stringify({ token, root }), { flag: 'wx', mode: 0o600 });
  return {
    root, container,
    cleanup() {
      // Refuse renamed/replaced roots and ownership changes before recursive deletion.
      if (fs.realpathSync(container) !== container || path.dirname(container) !== base ||
          fs.lstatSync(root).isSymbolicLink() || fs.realpathSync(root) !== root) throw Error('Fixture root ownership changed');
      const identity = JSON.parse(fs.readFileSync(marker, 'utf8'));
      if (identity.token !== token || identity.root !== root) throw Error('Fixture marker mismatch');
      fs.rmSync(container, { recursive: true });
    }
  };
}

class EffectLog {
  #fd;
  #sequence = 0;
  constructor(file) { this.#fd = fs.openSync(file, 'wx', 0o600); }
  record(kind, identity) {
    if (!['barrier', 'acknowledgement', 'effect'].includes(kind) || !/^[a-z0-9-]+$/.test(identity)) throw Error('Invalid observation');
    const observation = { sequence: ++this.#sequence, kind, identity };
    fs.writeSync(this.#fd, JSON.stringify(observation) + '\n');
    fs.fsyncSync(this.#fd);
    return observation;
  }
  close() { if (this.#fd !== undefined) { fs.closeSync(this.#fd); this.#fd = undefined; } }
}
module.exports = { FakeClock, ScriptedProvider, ownedRoot, EffectLog };
