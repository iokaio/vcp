// SPDX-License-Identifier: Apache-2.0
// Test-only access to the owned editor's real renderer. Never installs handlers
// or substitutes engine replies; interactions invoke actual DOM button clicks.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
class Renderer {
  constructor(socket) {
    this.socket = socket; this.next = 0; this.pending = new Map(); this.contexts = new Map(); this.sessions = new Set();
    socket.addEventListener('message', event => {
      const message = JSON.parse(String(event.data));
      if (message.id) {
        const pending = this.pending.get(message.id); if (!pending) return;
        this.pending.delete(message.id); clearTimeout(pending.timer);
        if (message.error) pending.reject(Error(`CDP ${message.error.code}`)); else pending.resolve(message.result);
      } else if (message.method === 'Runtime.executionContextCreated') {
        const context = message.params.context;
        this.contexts.set(`${message.sessionId}:${context.id}`, {session: message.sessionId, id: context.id});
      } else if (message.method === 'Runtime.executionContextDestroyed') {
        this.contexts.delete(`${message.sessionId}:${message.params.executionContextId}`);
      } else if (message.method === 'Runtime.executionContextsCleared') {
        for (const [key, context] of this.contexts) if (context.session === message.sessionId) this.contexts.delete(key);
      }
    });
  }
  call(method, params = {}, sessionId) {
    const id = ++this.next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(Error(`CDP deadline ${method}`)); }, 5000);
      this.pending.set(id, {resolve, reject, timer});
      this.socket.send(JSON.stringify({id, method, params, ...(sessionId ? {sessionId} : {})}));
    });
  }
  async attachTargets() {
    const {targetInfos} = await this.call('Target.getTargets');
    for (const target of targetInfos.filter(target => ['page', 'iframe'].includes(target.type))) {
      if (this.sessions.has(target.targetId)) continue;
      try {
        const {sessionId} = await this.call('Target.attachToTarget', {targetId: target.targetId, flatten: true});
        await this.call('Runtime.enable', {}, sessionId); this.sessions.add(target.targetId);
      } catch { /* A disposed test-owned webview may disappear during enumeration. */ }
    }
  }
  async evaluate(context, expression) {
    const reply = await this.call('Runtime.evaluate', {expression, contextId: context.id, returnByValue: true}, context.session);
    if (reply.exceptionDetails) throw Error('Renderer evaluation failed');
    return reply.result.value;
  }
  async inspector(expression) {
    await this.attachTargets();
    for (const context of this.contexts.values()) {
      try {
        if (await this.evaluate(context, "!!document.getElementById('inspector-tabs')")) return await this.evaluate(context, expression);
      } catch { /* Navigation retires old execution contexts. */ }
    }
    return undefined;
  }
  async wait(expression, label) {
    for (let index = 0; index < 100; index++) {
      const result = await this.inspector(expression); if (result) return result;
      await pause(50);
    }
    throw Error(`Renderer deadline: ${label}`);
  }
  async click(selector) {
    return this.wait(`(()=>{const button=document.querySelector(${JSON.stringify(selector)});if(!button||button.disabled)return false;button.click();return true;})()`, `click ${selector}`);
  }
  async text() { return this.wait('document.body.innerText', 'visible inspector text'); }
  async inert() {
    assert.equal(await this.inspector("document.querySelectorAll('a[href],iframe,img,object,embed,[onclick]').length"), 0);
  }
  close() {
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(Error('Renderer closed')); }
    this.pending.clear(); this.socket.close();
  }
}
exports.connect = async userData => {
  const discovery = path.join(userData, 'DevToolsActivePort'); let address;
  for (let index = 0; index < 200; index++) {
    if (fs.existsSync(discovery)) {
      const bytes = fs.readFileSync(discovery); assert(bytes.length < 4096);
      const [port, endpoint] = bytes.toString('utf8').trim().split(/\r?\n/);
      assert(/^\d{1,5}$/.test(port) && Number(port) > 0 && Number(port) < 65536);
      assert(/^\/devtools\/browser\/[a-zA-Z0-9-]+$/.test(endpoint));
      address = `ws://127.0.0.1:${port}${endpoint}`; break;
    }
    await pause(50);
  }
  assert(address, 'owned private CDP discovery');
  const socket = new WebSocket(address);
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, {once: true}); socket.addEventListener('error', reject, {once: true}); });
  return new Renderer(socket);
};
