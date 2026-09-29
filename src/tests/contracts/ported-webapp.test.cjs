// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const assert = require('node:assert/strict');
const test = require('node:test');
const {localOrigin, sameOrigin, runCheck} = require('../../skills/builtin/webapp-testing/scripts/check-page.cjs');
const playwrightPath = process.env.VCP_SKILL_PLAYWRIGHT;
const playwright = playwrightPath ? require(playwrightPath) : null;
const smoke = {skip: playwright ? false : 'Set VCP_SKILL_PLAYWRIGHT to an existing Playwright module path'};
async function ownedServer(handler) {
  const server = http.createServer(handler);
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return {server, origin: 'http://127.0.0.1:' + server.address().port,
    async close() { server.closeAllConnections(); await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); }};
}
function fixture(extra = '') {
 return '<!doctype html><html><body><main id="contact"><button>Save</button><p id="status">Ready</p></main><script>console.log("fixture ready");document.querySelector("button").onclick=()=>{setTimeout(()=>{document.querySelector("#status").textContent="Saved";console.log("saved fixture")},25)};</script>' + extra + '</body></html>';
}
const options = origin => ({url: origin + '/', ready: '#contact', button: 'Save', expectSelector: '#status', expectText: 'Saved', timeout: 5000});

test('origin policy rejects credential URLs, aliases, alternate schemes and other ports', () => {
 assert.equal(localOrigin('http://127.0.0.1:8000/a'), 'http://127.0.0.1:8000');
 assert.equal(localOrigin('http://[::1]:8000/a'), 'http://[::1]:8000');
 for (const url of ['file:///tmp/page.html', 'https://example.com/', 'http://u:p@127.0.0.1/', 'http://localhost/']) assert.throws(() => localOrigin(url));
 assert(sameOrigin('http://127.0.0.1:8000/b', 'http://127.0.0.1:8000'));
 assert(!sameOrigin('http://127.0.0.1:8001/b', 'http://127.0.0.1:8000'));
 assert(!sameOrigin('http://u:p@127.0.0.1:8000/', 'http://127.0.0.1:8000'));
});
test('real browser discovers, clicks, waits for observable state, captures logs and viewport', smoke, async () => {
 const app = await ownedServer((req,res) => { res.writeHead(200, {'Content-Type':'text/html'}); res.end(fixture()); });
 const output = fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'vcp-webapp-smoke-'));
 const screenshot = path.join(output, 'screen.png');
 try {
   const result = await runCheck(playwright, {...options(app.origin), captureConsole:true, screenshot});
   assert.equal(result.assertion, 'passed'); assert.deepEqual(result.buttons,['Save']); assert.equal(result.blocked,0);
   assert(result.console.some(entry=>entry.text==='saved fixture')); assert(fs.statSync(screenshot).size>100);
   assert(fs.readFileSync(screenshot).subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])));
   console.log(JSON.stringify({browser:result.browser,observed:result.assertion,consoleCount:result.consoleCount,screenshot}));
 } finally { await app.close(); }
});
test('long polling does not prevent readiness or action', smoke, async () => {
 const app = await ownedServer((req,res) => { if(req.url==='/pending') return; res.writeHead(200,{'Content-Type':'text/html'});res.end(fixture('<script>fetch("/pending").catch(()=>{})</script>')); });
 try { const result = await runCheck(playwright,options(app.origin)); assert.equal(result.assertion,'passed'); }
 finally { await app.close(); }
});
test('cross-origin subresource and redirect do not reach a canary listener', smoke, async () => {
 let canaryRequests=0;
 const canary=await ownedServer((req,res)=>{canaryRequests++;res.end('canary');});
 const app=await ownedServer((req,res)=>{
   if(req.url==='/redirect'){res.writeHead(302,{Location:canary.origin+'/should-not-run'});res.end();return;}
   res.writeHead(200,{'Content-Type':'text/html'});res.end(fixture('<script src="'+canary.origin+'/should-not-run.js"></script>'));
 });
 try {
   await assert.rejects(runCheck(playwright,options(app.origin)),/boundary/);
   await assert.rejects(runCheck(playwright,{...options(app.origin),url:app.origin+'/redirect'}));
   assert.equal(canaryRequests,0);
 } finally { await app.close(); await canary.close(); }
});
test('assertion failure disconnects the owned browser and preserves an existing output', smoke, async () => {
 const app=await ownedServer((req,res)=>{res.writeHead(200,{'Content-Type':'text/html'});res.end(fixture());});
 let browser;
 const tracked={chromium:{async launch(args){browser=await playwright.chromium.launch(args);return browser;}}};
 const output=fs.mkdtempSync(path.join(require('node:os').tmpdir(),'vcp-webapp-output-'));const screenshot=path.join(output,'user.png');fs.writeFileSync(screenshot,'USER DATA');
 try {
   await assert.rejects(runCheck(tracked,{...options(app.origin),expectText:'Wrong',timeout:600}));
   assert(browser);assert.equal(browser.isConnected(),false);
   await assert.rejects(runCheck(playwright,{...options(app.origin),screenshot}));
   assert.equal(fs.readFileSync(screenshot,'utf8'),'USER DATA');
 } finally { await app.close(); }
});
test('WebSockets are blocked before contacting an undeclared service', smoke, async () => {
 let attempts=0;
 const canary=await ownedServer((req,res)=>{attempts++;res.end();});
 canary.server.on('upgrade',(req,socket)=>{attempts++;socket.destroy();});
 const socketUrl=canary.origin.replace('http:','ws:')+'/socket';
 const app=await ownedServer((req,res)=>{res.writeHead(200,{'Content-Type':'text/html'});res.end(fixture('<script>new WebSocket('+JSON.stringify(socketUrl)+')</script>'));});
 try { await assert.rejects(runCheck(playwright,options(app.origin)),/boundary/);assert.equal(attempts,0); }
 finally { await app.close();await canary.close(); }
});
