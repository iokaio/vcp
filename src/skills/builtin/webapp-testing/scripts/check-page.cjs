// SPDX-License-Identifier: Apache-2.0
// Ported from Anthropic webapp-testing examples/element_discovery.py and
// examples/console_logging.py. VCP changes: Node API, bounded locator waits,
// exact-origin request guard, isolated context, explicit outputs and cleanup,
// optional ARIA snapshot and project-provided axe-core scan.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');

// Chromium resolves `localhost` only to loopback, so it is treated as literal loopback.
const LOOPBACK_HOSTS = ['127.0.0.1', '[::1]', 'localhost'];
const AXE_IMPACTS = ['minor', 'moderate', 'serious', 'critical'];
const AXE_MAX_BYTES = 8 * 1024 * 1024;

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
async function runCheck(playwright, options, {resolveAxe = projectAxeSource} = {}) {
  const origin = localOrigin(options.url);
  if (!options.ready || !options.expectSelector || typeof options.expectText !== 'string') {
    throw Error('ready, expectSelector and expectText are required');
  }
  const timeout = options.timeout ?? 10000;
  if (!Number.isSafeInteger(timeout) || timeout < 1 || timeout > 60000) throw Error('timeout must be 1..60000 ms');
  const outputs = [options.screenshot, options.ariaSnapshot].filter(Boolean);
  if (outputs.length === 2 && path.resolve(outputs[0]) === path.resolve(outputs[1])) throw Error('Output paths must differ');
  for (const output of outputs) if (fs.existsSync(output)) throw Error('Output file already exists; choose a new path');
  let browser;
  let context;
  const result = {origin, browser: null, buttons: [], consoleCount: 0, pageErrors: 0, console: [], blocked: 0, failedRequests: 0};
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
      if (!sameOrigin(route.request().url(), origin)) {
        result.blocked++;
        await route.abort('blockedbyclient');
        return;
      }
      try {
        const response = await route.fetch({maxRedirects: 0, timeout});
        if (response.status() >= 300 && response.status() < 400) {
          result.blocked++;
          await route.abort('blockedbyclient');
          return;
        }
        await route.fulfill({response});
      } catch {
        if (!closing) result.failedRequests++;
        await route.abort('failed').catch(() => {});
      }
    });
    await context.routeWebSocket('**/*', async socket => { result.blocked++; await socket.close().catch(() => {}); });
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
    const aria = options.ariaSnapshot ? await page.locator('body').ariaSnapshot({timeout}) : null;
    const image = options.screenshot ? await page.screenshot({timeout}) : null;
    if (!sameOrigin(page.url(), origin) || result.blocked) throw Error('Request, socket or navigation left the example allowed boundary');
    if (result.pageErrors) throw Error('Application emitted a page error');
    if (image) fs.writeFileSync(options.screenshot, image, {flag: 'wx'});
    if (aria !== null) fs.writeFileSync(options.ariaSnapshot, aria + '\n', {flag: 'wx'});
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
  console.error('Browser check failed or unavailable: ' + error.message);
  process.exitCode = 1;
});
module.exports = {localOrigin, sameOrigin, parseArgs, projectAxeSource, runCheck};
