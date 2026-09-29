// SPDX-License-Identifier: Apache-2.0
// Ported from Anthropic webapp-testing examples/element_discovery.py and
// examples/console_logging.py. VCP changes: Node API, bounded locator waits,
// exact-origin request guard, isolated context, explicit outputs and cleanup.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');

function localOrigin(value) {
  const url = new URL(value);
  if (!['http:', 'https:'].includes(url.protocol) ||
      !['127.0.0.1', '[::1]'].includes(url.hostname) || url.username || url.password) {
    throw Error('Use an authorized HTTP/HTTPS literal-loopback URL without credentials');
  }
  return url.origin;
}
function sameOrigin(value, origin) {
  try { const u = new URL(value); return !u.username && !u.password && u.origin === origin; }
  catch { return false; }
}
async function runCheck(playwright, options) {
  const origin = localOrigin(options.url);
  if (!options.ready || !options.expectSelector || typeof options.expectText !== 'string') {
    throw Error('ready, expectSelector and expectText are required');
  }
  const timeout = options.timeout ?? 10000;
  if (!Number.isSafeInteger(timeout) || timeout < 1 || timeout > 60000) throw Error('timeout must be 1..60000 ms');
  let browser;
  let context;
  const result = {origin, browser: null, buttons: [], consoleCount: 0, pageErrors: 0, console: [], blocked: 0, failedRequests: 0};
  let closing = false;
  try {
    browser = await playwright.chromium.launch({headless: true, chromiumSandbox: true, timeout});
    result.browser = browser.version();
    context = await browser.newContext({viewport: {width: 1280, height: 800},
      serviceWorkers: 'block', acceptDownloads: false});
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
    await page.locator(options.expectSelector).waitFor({state: 'visible', timeout});
    await page.waitForFunction(({selector, text}) =>
      document.querySelector(selector)?.textContent.trim() === text,
      {selector: options.expectSelector, text: options.expectText}, {timeout});
    if (!sameOrigin(page.url(), origin) || result.blocked) throw Error('Request, socket or navigation left the example allowed boundary');
    if (result.pageErrors) throw Error('Application emitted a page error');
    if (options.screenshot) fs.writeFileSync(options.screenshot, await page.screenshot(), {flag: 'wx'});
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
async function main(argv) {
  if (argv.length === 1 && argv[0] === '--help') {
    console.log('Usage: node check-page.cjs --url URL --ready CSS [--button NAME] --expect-selector CSS --expect-text TEXT [--capture-console] [--screenshot NEW_PATH]');
    return;
  }
  const options = {};
  const names = {'--url': 'url', '--ready': 'ready', '--button': 'button',
    '--expect-selector': 'expectSelector', '--expect-text': 'expectText', '--screenshot': 'screenshot'};
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--capture-console') { options.captureConsole = true; continue; }
    const key = names[argv[i]];
    if (!key || i + 1 >= argv.length || options[key] !== undefined) throw Error('Unknown, duplicate or missing argument; run --help');
    options[key] = argv[++i];
  }
  console.log(JSON.stringify(await runCheck(projectPlaywright(), options)));
}
if (require.main === module) main(process.argv.slice(2)).catch(error => {
  console.error('Browser check failed or unavailable: ' + error.message);
  process.exitCode = 1;
});
module.exports = {localOrigin, sameOrigin, runCheck};
