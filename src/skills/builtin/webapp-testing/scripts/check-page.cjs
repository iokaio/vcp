// SPDX-License-Identifier: Apache-2.0
// Ported from Anthropic webapp-testing examples/element_discovery.py and
// examples/console_logging.py. VCP changes: Node API, bounded locator waits,
// exact-origin request guard, isolated context, explicit outputs and cleanup,
// optional ARIA snapshot and project-provided axe-core scan, blocked-origin
// reporting, project-contained outputs and bounded CLI error text.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');

// Chromium resolves `localhost` only to loopback, so it is treated as literal loopback.
const LOOPBACK_HOSTS = ['127.0.0.1', '[::1]', 'localhost'];
const AXE_IMPACTS = ['minor', 'moderate', 'serious', 'critical'];
const AXE_MAX_BYTES = 8 * 1024 * 1024;
const MAX_BLOCKED_ORIGINS = 10;
const ERROR_TEXT_MAX = 300;
const WINDOWS_RESERVED = /^(con|prn|aux|nul|com[0-9]|lpt[0-9])(\..*)?$/i;

function localOrigin(value) {
  const url = new URL(value);
  if (!['http:', 'https:'].includes(url.protocol) ||
      !LOOPBACK_HOSTS.includes(url.hostname) || url.username || url.password) {
    throw Error('Use an authorized HTTP/HTTPS loopback URL (127.0.0.1, [::1] or localhost) without credentials');
  }
  return url.origin;
}
function sameOrigin(value, origin) {
  try { const u = new URL(value); return !u.username && !u.password && u.origin === origin; }
  catch { return false; }
}
function originOf(value) {
  // Origins only: paths and queries can carry page data.
  try { const origin = new URL(value).origin; return origin === 'null' ? 'opaque' : origin.slice(0, 100); }
  catch { return 'unparseable'; }
}
function inside(root, candidate) {
  const rel = path.relative(root, candidate);
  return rel !== '' && !rel.startsWith('..') && !path.isAbsolute(rel);
}
function containedOutput(root, value) {
  // Outputs stay under the project root: no absolute or '..' escapes, no
  // linked parent directory leading out of the root and no existing entry.
  if (typeof value !== 'string' || value === '') throw Error('Output path must be a non-empty string');
  const lexicalRoot = path.resolve(root);
  const target = path.resolve(lexicalRoot, value);
  if (!inside(lexicalRoot, target)) throw Error('Output path must stay inside the project root');
  const name = path.basename(target);
  if (process.platform === 'win32' && (path.relative(lexicalRoot, target).includes(':') || WINDOWS_RESERVED.test(name))) {
    throw Error('Output path must be an ordinary file name');
  }
  const realRoot = fs.realpathSync(lexicalRoot);
  let parent;
  try { parent = fs.realpathSync(path.dirname(target)); }
  catch { throw Error('Output parent directory must already exist inside the project root'); }
  if (parent !== realRoot && !inside(realRoot, parent)) throw Error('Output path must stay inside the project root');
  const output = path.join(parent, name);
  let exists = true;
  try { fs.lstatSync(output); } catch (error) { if (error.code === 'ENOENT') exists = false; else throw error; }
  if (exists) throw Error('Output file already exists; choose a new path');
  return output;
}
function failureMessage(error) {
  // Playwright call logs can include element HTML or page text; keep a bounded first line.
  const text = String(error?.message ?? error).split(/\r?\n/, 1)[0];
  return text.length > ERROR_TEXT_MAX ? text.slice(0, ERROR_TEXT_MAX) + '...' : text;
}
function parseTimeout(value) {
  const timeout = /^[0-9]{1,5}$/.test(value) ? Number(value) : NaN;
  if (!(timeout >= 1 && timeout <= 60000)) throw Error('timeout must be 1..60000 ms');
  return timeout;
}
function projectAxeSource(baseDir = process.cwd()) {
  const projectRequire = createRequire(path.join(baseDir, 'package.json'));
  let resolved;
  try { resolved = projectRequire.resolve('axe-core'); }
  catch (error) { if (error.code === 'MODULE_NOT_FOUND') return null; throw error; }
  if (fs.statSync(resolved).size > AXE_MAX_BYTES) throw Error('Project axe-core source is unexpectedly large');
  return fs.readFileSync(resolved, 'utf8');
}
function axeSummary(violations) {
  // Page-produced data is untrusted: keep only bounded ids, known impacts and counts.
  const list = Array.isArray(violations) ? violations : [];
  return {total: list.length, violations: list.slice(0, 50).map(v => ({
    id: String(v?.id ?? '').slice(0, 100),
    impact: AXE_IMPACTS.includes(v?.impact) ? v.impact : null,
    count: Number.isSafeInteger(v?.count) && v.count >= 0 ? v.count : 0}))};
}
async function waitForText(locator, text, timeout) {
  // Assert through the same Playwright locator that was waited on, so
  // Playwright-only selector syntax (text=, role=, >>) behaves consistently.
  const deadline = Date.now() + timeout;
  for (;;) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) break;
    const actual = await locator.textContent({timeout: remaining}).catch(error => {
      if (error?.name === 'TimeoutError') return null;
      throw error;
    });
    if (actual !== null && actual.trim() === text) return;
    await new Promise(resolve => setTimeout(resolve, Math.min(50, Math.max(0, deadline - Date.now()))));
  }
  throw Error('Expected text was not observed at expectSelector within ' + timeout + ' ms');
}
async function runCheck(playwright, options, {resolveAxe = projectAxeSource, outputRoot = process.cwd()} = {}) {
  const origin = localOrigin(options.url);
  if (!options.ready || !options.expectSelector || typeof options.expectText !== 'string') {
    throw Error('ready, expectSelector and expectText are required');
  }
  const timeout = options.timeout ?? 10000;
  if (!Number.isSafeInteger(timeout) || timeout < 1 || timeout > 60000) throw Error('timeout must be 1..60000 ms');
  const screenshot = options.screenshot ? containedOutput(outputRoot, options.screenshot) : null;
  const ariaSnapshot = options.ariaSnapshot ? containedOutput(outputRoot, options.ariaSnapshot) : null;
  if (screenshot && screenshot === ariaSnapshot) throw Error('Output paths must differ');
  let browser;
  let context;
  const result = {origin, browser: null, buttons: [], consoleCount: 0, pageErrors: 0, console: [], blocked: 0, failedRequests: 0};
  const blockedOrigins = [];
  const block = value => {
    result.blocked++;
    const blockedOrigin = originOf(value);
    if (!blockedOrigins.includes(blockedOrigin) && blockedOrigins.length < MAX_BLOCKED_ORIGINS) blockedOrigins.push(blockedOrigin);
  };
  let closing = false;
  try {
    browser = await playwright.chromium.launch({headless: true, chromiumSandbox: true, timeout});
    result.browser = browser.version();
    // Certificate-error tolerance is opt-in and honored only for a loopback origin.
    const ignoreHTTPSErrors = options.ignoreHttpsErrors === true &&
      LOOPBACK_HOSTS.includes(new URL(origin).hostname);
    context = await browser.newContext({viewport: {width: 1280, height: 800},
      serviceWorkers: 'block', acceptDownloads: false, ignoreHTTPSErrors});
    if (typeof context.routeWebSocket !== 'function') throw Error('This example requires Playwright with context.routeWebSocket support');
    // route.continue follows redirects without invoking this handler again.
    // Fetch one hop and reject redirects instead of following them off-origin.
    await context.route('**/*', async route => {
      const requestUrl = route.request().url();
      if (!sameOrigin(requestUrl, origin)) {
        block(requestUrl);
        await route.abort('blockedbyclient');
        return;
      }
      try {
        const response = await route.fetch({maxRedirects: 0, timeout});
        if (response.status() >= 300 && response.status() < 400) {
          let target = requestUrl;
          try { target = new URL(response.headers().location, requestUrl).href; } catch {}
          block(target);
          await route.abort('blockedbyclient');
          return;
        }
        await route.fulfill({response});
      } catch {
        if (!closing) result.failedRequests++;
        await route.abort('failed').catch(() => {});
      }
    });
    await context.routeWebSocket('**/*', async socket => { block(socket.url()); await socket.close().catch(() => {}); });
    const page = await context.newPage();
    page.setDefaultTimeout(timeout);
    page.on('console', message => {
      result.consoleCount++;
      if (options.captureConsole && result.console.length < 40) {
        result.console.push({type: message.type(), text: message.text().slice(0, 300)});
      }
    });
    page.on('pageerror', () => { result.pageErrors++; });
    await page.goto(options.url, {waitUntil: 'domcontentloaded', timeout});
    await page.locator(options.ready).waitFor({state: 'visible', timeout});
    result.buttons = await page.locator('button, [role="button"]').evaluateAll(nodes =>
      nodes.slice(0, 20).map(node => (node.getAttribute('aria-label') || node.textContent || '').trim().slice(0, 100)));
    if (options.button) await page.getByRole('button', {name: options.button, exact: true}).click();
    const expected = page.locator(options.expectSelector);
    await expected.waitFor({state: 'visible', timeout});
    await waitForText(expected, options.expectText, timeout);
    if (options.axe) {
      const source = resolveAxe();
      if (source === null) result.axe = 'unavailable';
      else {
        await page.evaluate(source);
        result.axe = axeSummary(await page.evaluate(async () => {
          const report = await window.axe.run(document, {resultTypes: ['violations']});
          return report.violations.map(v => ({id: v.id, impact: v.impact, count: v.nodes.length}));
        }));
      }
    }
    const aria = ariaSnapshot ? await page.locator('body').ariaSnapshot({timeout}) : null;
    const image = screenshot ? await page.screenshot({timeout}) : null;
    if (!sameOrigin(page.url(), origin)) block(page.url());
    if (result.blocked) {
      // Name blocked origins only (no paths or queries); any blocked request fails the check.
      const shown = blockedOrigins.slice(0, 5).join(', ') + (blockedOrigins.length > 5 ? ' and more' : '');
      const error = Error('Request, socket or navigation left the example allowed boundary; blocked origins: ' + shown);
      error.blockedOrigins = blockedOrigins.slice();
      throw error;
    }
    if (result.pageErrors) throw Error('Application emitted a page error');
    // Re-check containment just before the exclusive-create writes.
    if (image) fs.writeFileSync(containedOutput(outputRoot, options.screenshot), image, {flag: 'wx'});
    if (aria !== null) fs.writeFileSync(containedOutput(outputRoot, options.ariaSnapshot), aria + '\n', {flag: 'wx'});
    result.assertion = 'passed';
    return result;
  } finally {
    closing = true;
    try { if (context) await context.close(); }
    finally { if (browser) await browser.close(); }
  }
}
function projectPlaywright() {
  const projectRequire = createRequire(path.join(process.cwd(), 'package.json'));
  for (const name of ['playwright', '@playwright/test', 'playwright-core']) {
    let resolved;
    try { resolved = projectRequire.resolve(name); }
    catch (error) { if (error.code === 'MODULE_NOT_FOUND') continue; throw error; }
    return projectRequire(resolved);
  }
  throw Error('No project Playwright installation found; browser check not run');
}
const USAGE = 'Usage: node check-page.cjs --url URL --ready SELECTOR [--button NAME] --expect-selector SELECTOR --expect-text TEXT ' +
  '[--timeout MS] [--capture-console] [--screenshot NEW_PATH] [--aria-snapshot NEW_PATH] [--axe] [--ignore-https-errors]';
function parseArgs(argv) {
  const options = {};
  const flags = {'--capture-console': 'captureConsole', '--axe': 'axe', '--ignore-https-errors': 'ignoreHttpsErrors'};
  const names = {'--url': 'url', '--ready': 'ready', '--button': 'button', '--timeout': 'timeout',
    '--expect-selector': 'expectSelector', '--expect-text': 'expectText', '--screenshot': 'screenshot',
    '--aria-snapshot': 'ariaSnapshot'};
  for (let i = 0; i < argv.length; i++) {
    const flag = flags[argv[i]];
    if (flag) {
      if (options[flag] !== undefined) throw Error('Duplicate argument; run --help');
      options[flag] = true;
      continue;
    }
    const key = names[argv[i]];
    if (!key || i + 1 >= argv.length || options[key] !== undefined) throw Error('Unknown, duplicate or missing argument; run --help');
    options[key] = argv[++i];
  }
  if (options.timeout !== undefined) options.timeout = parseTimeout(options.timeout);
  return options;
}
async function main(argv) {
  if (argv.length === 1 && argv[0] === '--help') {
    console.log(USAGE);
    return;
  }
  const options = parseArgs(argv);
  console.log(JSON.stringify(await runCheck(projectPlaywright(), options)));
}
if (require.main === module) main(process.argv.slice(2)).catch(error => {
  console.error('Browser check failed or unavailable: ' + failureMessage(error));
  process.exitCode = 1;
});
module.exports = {localOrigin, sameOrigin, parseArgs, projectAxeSource, runCheck, containedOutput, failureMessage};
