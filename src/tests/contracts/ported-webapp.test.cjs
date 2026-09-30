// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const http = require('node:http');
const https = require('node:https');
const crypto = require('node:crypto');
const {spawnSync} = require('node:child_process');
const assert = require('node:assert/strict');
const test = require('node:test');
const {localOrigin, sameOrigin, parseArgs, projectAxeSource, runCheck, containedOutput, failureMessage} = require('../../skills/builtin/webapp-testing/scripts/check-page.cjs');
const playwrightPath = process.env.VCP_SKILL_PLAYWRIGHT;
const playwright = playwrightPath ? require(playwrightPath) : null;
const smoke = {skip: playwright ? false : 'Set VCP_SKILL_PLAYWRIGHT to an existing Playwright module path'};
async function ownedServer(handler, tls) {
  const server = tls ? https.createServer(tls, handler) : http.createServer(handler);
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return {server, port: server.address().port, origin: 'http://127.0.0.1:' + server.address().port,
    async close() { server.closeAllConnections(); await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); }};
}
function fixture(extra = '') {
 return '<!doctype html><html><body><main id="contact"><button>Save</button><p id="status">Ready</p></main><script>console.log("fixture ready");document.querySelector("button").onclick=()=>{setTimeout(()=>{document.querySelector("#status").textContent="Saved";console.log("saved fixture")},25)};</script>' + extra + '</body></html>';
}
const options = origin => ({url: origin + '/', ready: '#contact', button: 'Save', expectSelector: '#status', expectText: 'Saved', timeout: 5000});

// Minimal self-signed ECDSA certificate built with node:crypto, so the HTTPS
// check needs no fixture key material or external tooling.
function der(tag, ...parts) {
 const body = Buffer.concat(parts); const size = [];
 for (let n = body.length; n > 0; n >>= 8) size.unshift(n & 255);
 return Buffer.concat([Buffer.from([tag]), Buffer.from(body.length < 128 ? [body.length] : [0x80 | size.length, ...size]), body]);
}
function selfSignedLoopback() {
 const {privateKey, publicKey} = crypto.generateKeyPairSync('ec', {namedCurve: 'prime256v1'});
 const seq = (...parts) => der(0x30, ...parts);
 const sigAlg = seq(der(0x06, Buffer.from('2a8648ce3d040302', 'hex')));
 const name = seq(der(0x31, seq(der(0x06, Buffer.from('550403', 'hex')), der(0x0c, Buffer.from('localhost')))));
 const time = offset => der(0x17, Buffer.from(new Date(Date.now() + offset).toISOString().replace(/[-:T]/g, '').slice(2, 14) + 'Z'));
 const tbs = seq(der(0xa0, der(0x02, Buffer.from([2]))), der(0x02, Buffer.from([1, ...crypto.randomBytes(7)])), sigAlg, name,
   seq(time(-86400000), time(86400000)), name, publicKey.export({type: 'spki', format: 'der'}));
 const body = seq(tbs, sigAlg, der(0x03, Buffer.from([0]), crypto.sign('sha256', tbs, privateKey)));
 const cert = '-----BEGIN CERTIFICATE-----\n' + body.toString('base64').match(/.{1,64}/g).join('\n') + '\n-----END CERTIFICATE-----\n';
 new crypto.X509Certificate(cert);
 return {cert, key: privateKey.export({type: 'pkcs8', format: 'pem'})};
}
const tempDir = prefix => fs.mkdtempSync(path.join(os.tmpdir(), prefix));

test('origin policy accepts loopback names and rejects credential URLs, other hosts, alternate schemes and other ports', () => {
 assert.equal(localOrigin('http://127.0.0.1:8000/a'), 'http://127.0.0.1:8000');
 assert.equal(localOrigin('http://[::1]:8000/a'), 'http://[::1]:8000');
 assert.equal(localOrigin('http://localhost:8000/a'), 'http://localhost:8000');
 assert.equal(localOrigin('https://LOCALHOST:8443/'), 'https://localhost:8443');
 for (const url of ['file:///tmp/page.html', 'https://example.com/', 'http://u:p@127.0.0.1/', 'http://u:p@localhost/',
   'http://localhost.example.com/', 'http://app.localhost/', 'http://127.0.0.2/', 'http://0.0.0.0/', 'ws://localhost/',
   'ftp://localhost/', 'javascript:alert(1)']) assert.throws(() => localOrigin(url), url);
 assert(sameOrigin('http://127.0.0.1:8000/b', 'http://127.0.0.1:8000'));
 assert(!sameOrigin('http://127.0.0.1:8001/b', 'http://127.0.0.1:8000'));
 assert(!sameOrigin('http://u:p@127.0.0.1:8000/', 'http://127.0.0.1:8000'));
});
test('real browser discovers, clicks, waits for observable state, captures logs and viewport', smoke, async () => {
 const app = await ownedServer((req,res) => { res.writeHead(200, {'Content-Type':'text/html'}); res.end(fixture()); });
 const output = tempDir('vcp-webapp-smoke-');
 const screenshot = path.join(output, 'screen.png');
 try {
   const result = await runCheck(playwright, {...options(app.origin), captureConsole:true, screenshot}, {outputRoot: output});
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
 const output=tempDir('vcp-webapp-output-');const screenshot=path.join(output,'user.png');fs.writeFileSync(screenshot,'USER DATA');
 try {
   await assert.rejects(runCheck(tracked,{...options(app.origin),expectText:'Wrong',timeout:600}));
   assert(browser);assert.equal(browser.isConnected(),false);
   await assert.rejects(runCheck(playwright,{...options(app.origin),screenshot},{outputRoot:output}),/exists/);
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
test('CLI parses and bounds the timeout and opt-in flags', () => {
 const base = ['--url', 'http://localhost:5173/', '--ready', '#contact', '--expect-selector', 'text=Saved', '--expect-text', 'Saved'];
 const parsed = parseArgs([...base, '--timeout', '2500', '--axe', '--ignore-https-errors', '--aria-snapshot', 'aria.yml']);
 assert.equal(parsed.timeout, 2500); assert.equal(parsed.axe, true); assert.equal(parsed.ignoreHttpsErrors, true); assert.equal(parsed.ariaSnapshot, 'aria.yml');
 assert.equal(parseArgs([...base, '--timeout', '1']).timeout, 1); assert.equal(parseArgs([...base, '--timeout', '60000']).timeout, 60000);
 assert.equal(parseArgs(base).timeout, undefined);
 for (const value of ['0', '60001', '1.5', '-5', 'abc', '', '1e3', ' 100', '0x10']) assert.throws(() => parseArgs([...base, '--timeout', value]), /timeout/);
 assert.throws(() => parseArgs([...base, '--timeout'])); assert.throws(() => parseArgs([...base, '--axe', '--axe']));
 assert.throws(() => parseArgs([...base, '--unknown']));
});
test('runCheck rejects invalid timeouts before launching a browser', async () => {
 let launched = false;
 const fake = {chromium: {async launch() { launched = true; throw Error('unexpected'); }}};
 for (const timeout of [0, 60001, 1.5]) await assert.rejects(runCheck(fake, {...options('http://127.0.0.1:1'), timeout}), /timeout/);
 assert.equal(launched, false);
});
test('axe-core is resolved from the project only and is absent here', () => {
 assert.equal(projectAxeSource(tempDir('vcp-webapp-noaxe-')), null);
});
test('Playwright-only selectors are asserted through the waited locator', smoke, async () => {
 const app = await ownedServer((req,res) => { res.writeHead(200, {'Content-Type':'text/html'}); res.end(fixture()); });
 try {
   for (const expectSelector of ['text=Saved', 'role=main >> p', '#contact >> text=Saved']) {
     const result = await runCheck(playwright, {...options(app.origin), expectSelector});
     assert.equal(result.assertion, 'passed', expectSelector);
   }
   await assert.rejects(runCheck(playwright, {...options(app.origin), expectSelector: 'role=main >> p', expectText: 'Other', timeout: 600}), /not observed/);
 } finally { await app.close(); }
});
test('localhost is accepted and HTTPS certificate errors are tolerated only on request', smoke, async () => {
 const app = await ownedServer((req,res) => { res.writeHead(200, {'Content-Type':'text/html'}); res.end(fixture()); }, selfSignedLoopback());
 const url = 'https://localhost:' + app.port + '/';
 try {
   await assert.rejects(runCheck(playwright, {...options(app.origin), url, timeout: 3000}));
   const result = await runCheck(playwright, {...options(app.origin), url, ignoreHttpsErrors: true});
   assert.equal(result.assertion, 'passed'); assert.equal(result.origin, 'https://localhost:' + app.port); assert.equal(result.blocked, 0);
 } finally { await app.close(); }
});
test('ARIA snapshot is written with exclusive create', smoke, async () => {
 const app = await ownedServer((req,res) => { res.writeHead(200, {'Content-Type':'text/html'}); res.end(fixture()); });
 const output = tempDir('vcp-webapp-aria-');
 const ariaSnapshot = path.join(output, 'page.aria.yml');
 try {
   const result = await runCheck(playwright, {...options(app.origin), ariaSnapshot}, {outputRoot: output});
   assert.equal(result.assertion, 'passed');
   const snapshot = fs.readFileSync(ariaSnapshot, 'utf8');
   assert.match(snapshot, /- main:/); assert.match(snapshot, /- button "Save"/); assert.match(snapshot, /- paragraph: Saved/);
   await assert.rejects(runCheck(playwright, {...options(app.origin), ariaSnapshot}, {outputRoot: output}), /exists/);
   assert.equal(fs.readFileSync(ariaSnapshot, 'utf8'), snapshot);
 } finally { await app.close(); }
});
test('axe scan reports unavailable without failing, and bounds a provided result', smoke, async () => {
 const app = await ownedServer((req,res) => { res.writeHead(200, {'Content-Type':'text/html'}); res.end(fixture()); });
 // Stand-in for the project's axe-core source: exercises injection and bounding, not axe rules.
 const fakeAxe = 'window.axe = {run: async () => ({violations: [' +
   '{id: "color-contrast", impact: "serious", nodes: [1, 2]}, {id: "x".repeat(500), impact: "bogus", nodes: []}' +
   '].concat(Array.from({length: 60}, (_, i) => ({id: "rule-" + i, impact: "minor", nodes: [1]})))})};';
 try {
   const missing = await runCheck(playwright, {...options(app.origin), axe: true}, {resolveAxe: () => null});
   assert.equal(missing.assertion, 'passed'); assert.equal(missing.axe, 'unavailable');
   const skipped = await runCheck(playwright, options(app.origin), {resolveAxe: () => { throw Error('not requested'); }});
   assert.equal(skipped.axe, undefined);
   const scanned = await runCheck(playwright, {...options(app.origin), axe: true}, {resolveAxe: () => fakeAxe});
   assert.equal(scanned.axe.total, 62); assert.equal(scanned.axe.violations.length, 50);
   assert.deepEqual(scanned.axe.violations[0], {id: 'color-contrast', impact: 'serious', count: 2});
   assert.deepEqual(scanned.axe.violations[1], {id: 'x'.repeat(100), impact: null, count: 0});
 } finally { await app.close(); }
});

const helper = path.resolve(__dirname, '../../skills/builtin/webapp-testing/scripts/check-page.cjs');
const cliArgs = ['--url', 'http://127.0.0.1:1/', '--ready', '#contact', '--expect-selector', '#status', '--expect-text', 'Saved'];
// Run the helper as a materialized copy is run: from the project root, without inherited module paths.
const runCli = (args, cwd) => spawnSync(process.execPath, [helper, ...args],
 {cwd, encoding: 'utf8', timeout: 30000, env: {...process.env, NODE_PATH: ''}});
function fakeProject(launchBody) {
 const project = tempDir('vcp-webapp-project-');
 const pkg = path.join(project, 'node_modules', 'playwright');
 fs.mkdirSync(pkg, {recursive: true});
 fs.writeFileSync(path.join(pkg, 'package.json'), '{"name":"playwright","main":"index.js"}\n');
 fs.writeFileSync(path.join(pkg, 'index.js'), 'module.exports = {chromium: {async launch() { ' + launchBody + ' }}};\n');
 return project;
}
const notLaunched = {chromium: {async launch() { throw Error('launch reached'); }}};

test('CLI --help prints usage and exits zero; bad arguments exit nonzero', () => {
 const help = runCli(['--help'], tempDir('vcp-webapp-help-'));
 assert.equal(help.status, 0); assert.match(help.stdout, /^Usage: node check-page\.cjs --url URL /); assert.equal(help.stderr, '');
 const bad = runCli(['--bogus'], tempDir('vcp-webapp-help-'));
 assert.equal(bad.status, 1); assert.equal(bad.stdout, '');
 assert.equal(bad.stderr, 'Browser check failed or unavailable: Unknown, duplicate or missing argument; run --help\n');
});
test('CLI reports a missing project Playwright installation in the current directory', () => {
 const run = runCli(cliArgs, tempDir('vcp-webapp-noplaywright-'));
 assert.equal(run.status, 1); assert.equal(run.stdout, '');
 assert.equal(run.stderr, 'Browser check failed or unavailable: No project Playwright installation found; browser check not run\n');
});
test('CLI resolves the project Playwright and prints only a bounded first error line', () => {
 const multiline = runCli(cliArgs, fakeProject('throw new Error(' + JSON.stringify('launch failed at locator\n<div class="x">PAGE SECRET</div>') + ');'));
 assert.equal(multiline.status, 1);
 assert.equal(multiline.stderr, 'Browser check failed or unavailable: launch failed at locator\n');
 const long = runCli(cliArgs, fakeProject('throw new Error("x".repeat(5000) + "TAIL");'));
 assert.equal(long.status, 1);
 assert.equal(long.stderr, 'Browser check failed or unavailable: ' + 'x'.repeat(300) + '...\n');
 assert.equal(failureMessage(Error('one\r\ntwo')), 'one'); assert.equal(failureMessage('plain'), 'plain');
 assert.equal(failureMessage(Error('y'.repeat(300))), 'y'.repeat(300));
});
test('output paths are contained under the project root before a browser launches', async () => {
 const root = tempDir('vcp-webapp-root-');
 const outside = tempDir('vcp-webapp-outside-');
 fs.mkdirSync(path.join(root, 'shots'));
 fs.writeFileSync(path.join(root, 'existing.png'), 'USER DATA');
 fs.symlinkSync(outside, path.join(root, 'linked'), 'junction');
 const reject = async (value, pattern) => {
   for (const key of ['screenshot', 'ariaSnapshot']) {
     await assert.rejects(runCheck(notLaunched, {...options('http://127.0.0.1:1'), [key]: value}, {outputRoot: root}), pattern, key + ' ' + value);
   }
 };
 await reject(path.join(outside, 'screen.png'), /inside the project root/);
 await reject('../escape.png', /inside the project root/);
 await reject('shots/../../escape.png', /inside the project root/);
 await reject('.', /inside the project root/);
 await reject('linked/screen.png', /inside the project root/);
 await reject('linked', /exists/);
 await reject('existing.png', /exists/);
 await reject('missing/screen.png', /parent directory must already exist/);
 if (process.platform === 'win32') { await reject('screen.png:stream', /ordinary file name/); await reject('NUL.png', /ordinary file name/); }
 // A file symlink needs privileges on some Windows hosts; the junction cases above always run.
 let fileLink = false;
 try { fs.symlinkSync(path.join(outside, 'target.png'), path.join(root, 'dangling.png'), 'file'); fileLink = true; }
 catch (error) { if (error.code !== 'EPERM') throw error; }
 if (fileLink) await reject('dangling.png', /exists/);
 assert.equal(fs.readFileSync(path.join(root, 'existing.png'), 'utf8'), 'USER DATA');
 assert.deepEqual(fs.readdirSync(outside), []);
 await assert.rejects(runCheck(notLaunched, {...options('http://127.0.0.1:1'), screenshot: 'same.png', ariaSnapshot: './same.png'}, {outputRoot: root}), /differ/);
 // Relative and absolute in-root paths pass validation and reach the browser launch.
 await assert.rejects(runCheck(notLaunched, {...options('http://127.0.0.1:1'), screenshot: 'shots/new.png',
   ariaSnapshot: path.join(root, 'page.aria.yml')}, {outputRoot: root}), /launch reached/);
 assert.equal(containedOutput(root, 'shots/new.png'), path.join(fs.realpathSync(root), 'shots', 'new.png'));
});
test('a page error fails the check without echoing the page message', smoke, async () => {
 const app = await ownedServer((req,res) => { res.writeHead(200, {'Content-Type':'text/html'}); res.end(fixture('<script>throw new Error("PAGE SECRET")</script>')); });
 try {
   await assert.rejects(runCheck(playwright, options(app.origin)), error => {
     assert.equal(error.message, 'Application emitted a page error'); return true;
   });
 } finally { await app.close(); }
});
test('service-worker registration is blocked and never fetches the worker script', smoke, async () => {
 let workerRequests = 0;
 const app = await ownedServer((req,res) => {
   if (req.url === '/sw.js') { workerRequests++; res.writeHead(200, {'Content-Type':'text/javascript'}); res.end('self.addEventListener("fetch", () => {});'); return; }
   res.writeHead(200, {'Content-Type':'text/html'});
   res.end(fixture('<script>navigator.serviceWorker && navigator.serviceWorker.register("/sw.js").catch(() => {})</script>'));
 });
 try {
   const result = await runCheck(playwright, options(app.origin));
   assert.equal(result.assertion, 'passed'); assert.equal(result.blocked, 0); assert.equal(workerRequests, 0);
 } finally { await app.close(); }
});
test('blocked requests fail the check and name only their origins', smoke, async () => {
 let canaryRequests = 0;
 const canary = await ownedServer((req,res) => { canaryRequests++; res.end(); });
 const app = await ownedServer((req,res) => {
   res.writeHead(200, {'Content-Type':'text/html'});
   res.end(fixture('<link rel="stylesheet" href="' + canary.origin + '/fonts/private-path.css?token=QUERYSECRET">' +
     '<img src="http://localhost:' + canary.port + '/img.png?q=1">'));
 });
 try {
   await assert.rejects(runCheck(playwright, options(app.origin)), error => {
     assert.match(error.message, /^Request, socket or navigation left the example allowed boundary; blocked origins: /);
     assert.deepEqual([...error.blockedOrigins].sort(), [canary.origin, 'http://localhost:' + canary.port].sort());
     assert.doesNotMatch(error.message, /private-path|QUERYSECRET|img\.png/);
     return true;
   });
   assert.equal(canaryRequests, 0);
 } finally { await app.close(); await canary.close(); }
});
