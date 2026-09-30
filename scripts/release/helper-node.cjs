// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const http = require('node:http'), assert = require('node:assert/strict');
const { createRequire } = require('node:module');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function plain(file) {
  assert(path.isAbsolute(file), 'Absolute helper input required');
  for (let cursor = file;; cursor = path.dirname(cursor)) {
    assert(!fs.lstatSync(cursor).isSymbolicLink(), 'Redirected helper input refused');
    if (cursor === path.dirname(cursor)) break;
  }
  return file;
}
function fileHash(file) {
  const fd = fs.openSync(plain(file), 'r'), digest = crypto.createHash('sha256'), buffer = Buffer.alloc(1024 * 1024);
  try { for (;;) { const length = fs.readSync(fd, buffer); if (!length) break; digest.update(buffer.subarray(0, length)); } }
  finally { fs.closeSync(fd); }
  return digest.digest('hex');
}
function tree(root, output) {
  plain(root); const rows = [], pending = ['']; let bytes = 0;
  while (pending.length) {
    const relative = pending.pop();
    for (const entry of fs.readdirSync(path.join(root, relative), { withFileTypes: true })) {
      const name = path.join(relative, entry.name), file = plain(path.join(root, name));
      if (entry.isDirectory()) pending.push(name);
      else {
        assert(entry.isFile(), 'Ordinary dependency files required');
        const size = fs.statSync(file).size; bytes += size;
        assert(rows.length < 100000 && bytes < 2 * 1024 ** 3, 'Dependency inventory bound exceeded');
        rows.push({ path: name.split(path.sep).join('/'), bytes: size, sha256: fileHash(file) });
      }
    }
  }
  rows.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  const data = JSON.stringify(rows); fs.writeFileSync(output, data, { flag: 'wx' });
  return { root, files: rows.length, bytes, inventory: output, sha256: hash(data) };
}
function authoring(skills) {
  const catalog = JSON.parse(fs.readFileSync(plain(path.join(skills, 'catalog.json')), 'utf8'));
  assert.equal(catalog.schema_version, 1); assert(Array.isArray(catalog.skills) && catalog.skills.length > 0 && catalog.skills.length <= 128);
  const ids = new Set(), validatorPath = plain(path.join(skills, 'skill-authoring/scripts/validate.cjs'));
  const validator = require(validatorPath), rows = [];
  for (const entry of catalog.skills) {
    assert(/^[a-z0-9][a-z0-9._-]{0,127}$/.test(entry.id) && !ids.has(entry.id), 'Unique ordinary catalog skill ID required');
    ids.add(entry.id);
    assert.equal(entry.descriptor, `${entry.id}/skill.json`);
    assert.equal(fileHash(path.join(skills, entry.descriptor)), entry.descriptor_sha256);
    const result = validator.validateSkill(plain(path.join(skills, entry.id)));
    assert.equal(result.id, entry.id); assert(Array.isArray(result.warnings));
    rows.push({ ...result, status: result.warnings.length ? 'pass-with-warnings' : 'pass' });
  }
  return { status: 'pass', catalog_sha256: fileHash(path.join(skills, 'catalog.json')), validator_sha256: fileHash(validatorPath),
    skills: rows, warnings: rows.reduce((count, row) => count + row.warnings.length, 0) };
}
function dependencies(spec, label) {
  const project = plain(spec.browser_project), load = createRequire(path.join(project, 'package.json'));
  const packagePath = plain(load.resolve('playwright/package.json'));
  const corePath = plain(load.resolve('playwright-core/package.json'));
  assert.equal(path.dirname(packagePath), path.join(project, 'node_modules/playwright'));
  assert.equal(path.dirname(corePath), path.join(project, 'node_modules/playwright-core'));
  assert.equal(JSON.parse(fs.readFileSync(packagePath)).version, '1.63.0');
  assert.equal(JSON.parse(fs.readFileSync(corePath)).version, '1.63.0');
  const registry = load('playwright-core/lib/coreBundle').registry.registry;
  const browser = plain(registry.findExecutable('chromium-headless-shell').executablePath());
  const trees = [tree(path.dirname(packagePath), path.join(spec.output, `node-${label}-playwright.json`)),
    tree(path.dirname(corePath), path.join(spec.output, `node-${label}-core.json`)),
    tree(path.dirname(browser), path.join(spec.output, `node-${label}-browser.json`))];
  const lock = plain(path.join(project, 'package-lock.json'));
  let axe = null;
  try { axe = plain(load.resolve('axe-core')); } catch (error) { if (error.code !== 'MODULE_NOT_FOUND') throw error; }
  return { node_version: process.version, node_sha256: fileHash(process.execPath), playwright_version: '1.63.0',
    browser, browser_sha256: fileHash(browser), lock_sha256: fileHash(lock), trees,
    axe: axe ? { status: 'present-not-selected', sha256: fileHash(axe) } : { status: 'missing', scan: 'not-run' } };
}
async function listen(server) {
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return `http://127.0.0.1:${server.address().port}`;
}
async function close(server) { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
async function browser(spec) {
  const load = createRequire(path.join(plain(spec.browser_project), 'package.json'));
  const playwright = load('playwright');
  const helper = plain(path.join(spec.skills, 'webapp-testing/scripts/check-page.cjs'));
  const { runCheck } = require(helper);
  const launches = [];
  const observedPlaywright = { chromium: { launch(options) {
    launches.push({ ...options });
    return playwright.chromium.launch(options);
  } } };
  let otherRequests = 0;
  const other = http.createServer((request, response) => { otherRequests++; response.end('must never be reached'); });
  const otherOrigin = await listen(other);
  const server = http.createServer((request, response) => {
    response.setHeader('Content-Type', 'text/html; charset=utf-8');
    const action = request.url === '/refuse' ? `fetch('${otherOrigin}/denied').catch(()=>{}).then(()=>document.querySelector('#status').textContent='Saved')` : `document.querySelector('#status').textContent='Saved'`;
    response.end(`<button onclick="${action}">Save</button><p id="status">Ready</p>`);
  });
  try {
    const origin = await listen(server);
    const options = { url: origin, ready: '#status', button: 'Save', expectSelector: '#status', expectText: 'Saved', timeout: 10000 };
    const passed = await runCheck(observedPlaywright, { ...options, screenshot: 'saved.png' }, { outputRoot: spec.output });
    assert.equal(passed.assertion, 'passed'); assert.equal(passed.blocked, 0);
    let refusal;
    try { await runCheck(observedPlaywright, { ...options, url: origin + '/refuse' }, { outputRoot: spec.output }); }
    catch (error) { refusal = error; }
    assert(refusal, 'Exact-origin violation must refuse');
    assert.deepEqual(refusal.blockedOrigins, [otherOrigin]); assert.equal(otherRequests, 0);
    assert.equal(launches.length, 2);
    assert(launches.every(options => options.chromiumSandbox === true && options.headless === true));
    return { status: 'pass', helper_sha256: fileHash(helper), interaction: passed,
      exact_origin_refusal: { status: 'pass', blocked_origins: refusal.blockedOrigins, other_origin_requests: otherRequests },
      screenshot_sha256: fileHash(path.join(spec.output, 'saved.png')), browser_launch_options: launches,
      chromium_sandbox_requested: true,
      accessibility_scan: { status: 'not-run', reason: 'axe-core is not selected for this bounded helper smoke' } };
  } finally { await close(server); await close(other); }
}
async function main(file, mode) {
  const spec = JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
  let result;
  if (mode === 'before' || mode === 'after') result = dependencies(spec, mode);
  else if (mode === 'authoring') result = authoring(spec.skills);
  else if (mode === 'browser') result = await browser(spec);
  else throw Error('Unknown helper qualification mode');
  process.stdout.write(JSON.stringify(result) + '\n');
}
if (require.main === module) main(...process.argv.slice(2)).catch(error => { console.error(String(error.stack)); process.exitCode = 1; });
module.exports = { plain, fileHash, tree, authoring };
