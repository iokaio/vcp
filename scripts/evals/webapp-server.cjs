// SPDX-License-Identifier: Apache-2.0
'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

const SHA256 = /^[a-f0-9]{64}$/;
const CONTENT_TYPES = new Set([
  'application/json; charset=utf-8',
  'application/javascript; charset=utf-8',
  'text/css; charset=utf-8',
  'text/html; charset=utf-8',
  'text/plain; charset=utf-8',
]);
const DEFAULT_INVENTORY_LIMITS = Object.freeze({ maxFiles: 32, maxFileBytes: 256 * 1024, maxTotalBytes: 1024 * 1024, maxDelayMs: 2_000 });
const DEFAULT_SERVER_LIMITS = Object.freeze({ startupMs: 2_000, requestMs: 3_000, shutdownMs: 1_000, maxHeaderBytes: 8 * 1024, maxRequests: 128, maxConnections: 8, maxConnectionAttempts: 160 });

function boundedInteger(value, fallback, minimum, maximum, label) {
  const selected = value === undefined ? fallback : value;
  if (!Number.isSafeInteger(selected) || selected < minimum || selected > maximum) throw Error(`${label} is outside its bounded integer range`);
  return selected;
}

function inventoryLimits(value = {}) {
  return Object.freeze({
    maxFiles: boundedInteger(value.maxFiles, DEFAULT_INVENTORY_LIMITS.maxFiles, 1, 128, 'maxFiles'),
    maxFileBytes: boundedInteger(value.maxFileBytes, DEFAULT_INVENTORY_LIMITS.maxFileBytes, 1, 1024 * 1024, 'maxFileBytes'),
    maxTotalBytes: boundedInteger(value.maxTotalBytes, DEFAULT_INVENTORY_LIMITS.maxTotalBytes, 1, 4 * 1024 * 1024, 'maxTotalBytes'),
    maxDelayMs: boundedInteger(value.maxDelayMs, DEFAULT_INVENTORY_LIMITS.maxDelayMs, 0, 10_000, 'maxDelayMs'),
  });
}

function serverLimits(value = {}) {
  return Object.freeze({
    startupMs: boundedInteger(value.startupMs, DEFAULT_SERVER_LIMITS.startupMs, 10, 30_000, 'startupMs'),
    requestMs: boundedInteger(value.requestMs, DEFAULT_SERVER_LIMITS.requestMs, 10, 30_000, 'requestMs'),
    shutdownMs: boundedInteger(value.shutdownMs, DEFAULT_SERVER_LIMITS.shutdownMs, 10, 10_000, 'shutdownMs'),
    maxHeaderBytes: boundedInteger(value.maxHeaderBytes, DEFAULT_SERVER_LIMITS.maxHeaderBytes, 1024, 32 * 1024, 'maxHeaderBytes'),
    maxRequests: boundedInteger(value.maxRequests, DEFAULT_SERVER_LIMITS.maxRequests, 1, 1024, 'maxRequests'),
    maxConnections: boundedInteger(value.maxConnections, DEFAULT_SERVER_LIMITS.maxConnections, 1, 32, 'maxConnections'),
    maxConnectionAttempts: boundedInteger(value.maxConnectionAttempts, DEFAULT_SERVER_LIMITS.maxConnectionAttempts, 1, 2048, 'maxConnectionAttempts'),
  });
}

function safeRoute(route) {
  if (typeof route !== 'string' || !route.startsWith('/') || route.length > 256 || route.includes('%') || route.includes('\\') || route.includes('?') || route.includes('#') || route.includes('\0')) return false;
  if (route === '/') return true;
  if (route !== '/' && route.endsWith('/')) return false;
  const segments = route.split('/').slice(1);
  return segments.every(segment => segment && segment !== '.' && segment !== '..' && /^[A-Za-z0-9._~-]+$/.test(segment));
}

function safeRelative(relative) {
  if (typeof relative !== 'string' || !relative || relative.length > 512 || relative.includes('\\') || relative.includes(':') || relative.includes('\0') || path.posix.isAbsolute(relative)) return false;
  const segments = relative.split('/');
  return segments.every(segment => segment && segment !== '.' && segment !== '..' && !/[. ]$/.test(segment) && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(segment));
}

function assertUnlinkedPath(root, relative) {
  const identities = [];
  for (let ancestor = root; ; ancestor = path.dirname(ancestor)) {
    const ancestorStat = fs.lstatSync(ancestor);
    if (ancestorStat.isSymbolicLink()) throw Error('Inventory root has a symlink or reparse ancestor');
    identities.push({ file: ancestor, dev: ancestorStat.dev, ino: ancestorStat.ino, size: ancestorStat.size, mtimeMs: ancestorStat.mtimeMs, directory: true });
    if (path.dirname(ancestor) === ancestor) break;
  }
  const rootStat = fs.lstatSync(root);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) throw Error('Inventory root must be a real directory, not a symlink or reparse alias');
  let current = root;
  let stat;
  for (const segment of relative.split('/')) {
    current = path.join(current, segment);
    stat = fs.lstatSync(current);
    if (stat.isSymbolicLink()) throw Error(`Inventory source uses a symlink or reparse alias: ${relative}`);
    if (current !== path.join(root, ...relative.split('/')) && !stat.isDirectory()) throw Error(`Inventory source ancestor is not a directory: ${relative}`);
    identities.push({ file: current, dev: stat.dev, ino: stat.ino, size: stat.size, mtimeMs: stat.mtimeMs, directory: current !== path.join(root, ...relative.split('/')) });
  }
  return { file: current, stat, identities };
}

function assertIdentities(identities) {
  for (const expected of identities) {
    const actual = fs.lstatSync(expected.file);
    const kindChanged = expected.directory ? !actual.isDirectory() : !actual.isFile();
    const fileChanged = !expected.directory && (actual.size !== expected.size || actual.mtimeMs !== expected.mtimeMs);
    if (actual.isSymbolicLink() || actual.dev !== expected.dev || actual.ino !== expected.ino || kindChanged || fileChanged) throw Error('Inventory source path identity changed during snapshot');
  }
}

function stableRead(file, expectedStat, identities, expectedHash, maximum) {
  // O_NOFOLLOW is unavailable in Windows Node. Repeated lstat/fstat identity and
  // content-hash checks bind trusted synthetic staging; they do not admit a
  // hostile concurrently mutated source tree as a stronger filesystem boundary.
  const descriptor = fs.openSync(file, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
  try {
    const before = fs.fstatSync(descriptor);
    if (!before.isFile()) throw Error('Inventory source is not a regular file');
    if (before.dev !== expectedStat.dev || before.ino !== expectedStat.ino || before.size !== expectedStat.size || before.mtimeMs !== expectedStat.mtimeMs) throw Error('Inventory source changed before snapshot');
    assertIdentities(identities);
    if (before.size > maximum) throw Error('Inventory source exceeds maxFileBytes');
    const bytes = Buffer.alloc(before.size);
    let offset = 0;
    while (offset < bytes.length) {
      const count = fs.readSync(descriptor, bytes, offset, bytes.length - offset, offset);
      if (!count) throw Error('Inventory source changed during snapshot');
      offset += count;
    }
    const after = fs.fstatSync(descriptor);
    if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeMs !== after.mtimeMs) throw Error('Inventory source changed during snapshot');
    assertIdentities(identities);
    const actual = crypto.createHash('sha256').update(bytes).digest('hex');
    if (actual !== expectedHash) throw Error(`Inventory source hash mismatch: expected ${expectedHash}, received ${actual}`);
    return bytes;
  } finally {
    fs.closeSync(descriptor);
  }
}

/**
 * Validate an explicit synthetic-file manifest and snapshot every source into memory.
 * Returned inventory contains immutable base64 strings, never live filesystem paths.
 */
function freezeInventory(specification) {
  if (!specification || typeof specification !== 'object' || Array.isArray(specification)) throw Error('Inventory specification is required');
  if (typeof specification.root !== 'string' || !path.isAbsolute(specification.root)) throw Error('Inventory root must be an existing absolute directory');
  const root = path.resolve(specification.root);
  if (!fs.existsSync(root)) throw Error('Inventory root must be an existing absolute directory');
  const limits = inventoryLimits(specification.limits);
  if (!Array.isArray(specification.entries) || !specification.entries.length || specification.entries.length > limits.maxFiles) throw Error('Inventory entry count is outside bounds');
  const routes = new Set();
  let totalBytes = 0;
  const entries = specification.entries.map((entry, index) => {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) throw Error(`Inventory entry ${index} is invalid`);
    if (!safeRoute(entry.route)) throw Error(`Inventory route is not an exact safe raw path: ${entry.route}`);
    if (routes.has(entry.route)) throw Error(`Duplicate inventory route: ${entry.route}`);
    routes.add(entry.route);
    if (!safeRelative(entry.source)) throw Error(`Inventory source is not a safe relative path: ${entry.source}`);
    if (typeof entry.sha256 !== 'string' || !SHA256.test(entry.sha256)) throw Error(`Inventory entry ${entry.route} requires a lowercase SHA-256`);
    if (!CONTENT_TYPES.has(entry.contentType)) throw Error(`Inventory entry ${entry.route} has an unsupported content type`);
    const delayMs = boundedInteger(entry.delayMs, 0, 0, limits.maxDelayMs, `delayMs for ${entry.route}`);
    const selected = assertUnlinkedPath(root, entry.source);
    const bytes = stableRead(selected.file, selected.stat, selected.identities, entry.sha256, limits.maxFileBytes);
    totalBytes += bytes.length;
    if (totalBytes > limits.maxTotalBytes) throw Error('Inventory exceeds maxTotalBytes');
    return Object.freeze({ route: entry.route, sha256: entry.sha256, contentType: entry.contentType, delayMs, bytes: bytes.toString('base64') });
  });
  return Object.freeze({ schema: 'vcp-webapp-memory-inventory/1', entries: Object.freeze(entries), totalBytes, limits });
}

function checkedEntries(inventory) {
  if (!inventory || inventory.schema !== 'vcp-webapp-memory-inventory/1' || !Object.isFrozen(inventory) || !Object.isFrozen(inventory.entries)) throw Error('A frozen VCP webapp inventory is required');
  const limits = inventoryLimits(inventory.limits);
  if (!Array.isArray(inventory.entries) || !inventory.entries.length || inventory.entries.length > limits.maxFiles) throw Error('Frozen inventory entry count failed integrity validation');
  const routes = new Map();
  let totalBytes = 0;
  for (const entry of inventory.entries) {
    if (!Object.isFrozen(entry) || !safeRoute(entry.route) || routes.has(entry.route) || !CONTENT_TYPES.has(entry.contentType) || !SHA256.test(entry.sha256)) throw Error('Frozen inventory integrity check failed');
    boundedInteger(entry.delayMs, undefined, 0, limits.maxDelayMs, `delayMs for ${entry.route}`);
    if (typeof entry.bytes !== 'string' || entry.bytes.length > Math.ceil(limits.maxFileBytes / 3) * 4 + 4) throw Error('Frozen inventory encoded body exceeds bounds');
    const bytes = Buffer.from(entry.bytes, 'base64');
    totalBytes += bytes.length;
    if (bytes.toString('base64') !== entry.bytes || bytes.length > limits.maxFileBytes || totalBytes > limits.maxTotalBytes || crypto.createHash('sha256').update(bytes).digest('hex') !== entry.sha256) throw Error('Frozen inventory bytes failed integrity validation');
    routes.set(entry.route, { ...entry, bytes });
  }
  if (totalBytes !== inventory.totalBytes) throw Error('Frozen inventory total failed integrity validation');
  return routes;
}

function responseHeaders(entry, bodyLength) {
  return {
    'Cache-Control': 'no-store',
    'Content-Length': String(bodyLength),
    'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
    'Content-Type': entry.contentType,
    'Cross-Origin-Resource-Policy': 'same-origin',
    'Referrer-Policy': 'no-referrer',
    'X-Content-Type-Options': 'nosniff',
  };
}

/** Start one owned HTTP server on an OS-assigned 127.0.0.1 port. */
async function startOwnedServer(inventory, options = {}) {
  const entries = checkedEntries(inventory);
  const limits = serverLimits(options.limits);
  const sockets = new Set();
  const timers = new Set();
  let requests = 0;
  let connectionAttempts = 0;
  let stopping = false;
  let terminalReason = null;
  let listenerClosed = false;
  let terminalTimer;
  let shutdownTimerCount = 0;

  const reply = (response, status, text) => {
    if (response.destroyed || response.writableEnded) return;
    const body = Buffer.from(text);
    response.writeHead(status, { 'Cache-Control': 'no-store', 'Connection': 'close', 'Content-Length': String(body.length), 'Content-Type': 'text/plain; charset=utf-8', 'X-Content-Type-Options': 'nosniff' });
    response.end(body);
  };
  let expectedHost;
  const terminateAdmission = reason => {
    if (terminalReason) return;
    terminalReason = reason;
    server.close(() => { listenerClosed = true; clearTimeout(terminalTimer); terminalTimer = undefined; });
    terminalTimer = setTimeout(() => {
      terminalTimer = undefined;
      for (const socket of sockets) socket.destroy();
      if (typeof server.closeAllConnections === 'function') server.closeAllConnections();
    }, limits.requestMs);
    terminalTimer.unref?.();
  };
  const chargeConnectionAttempt = () => {
    connectionAttempts += 1;
    if (connectionAttempts >= limits.maxConnectionAttempts) terminateAdmission('max_connection_attempts');
  };
  const server = http.createServer({ maxHeaderSize: limits.maxHeaderBytes, requireHostHeader: true }, (request, response) => {
    requests += 1;
    response.setHeader('Connection', 'close');
    const deadline = setTimeout(() => reply(response, 408, 'request timeout\n'), limits.requestMs);
    timers.add(deadline);
    const finish = () => { clearTimeout(deadline); timers.delete(deadline); };
    response.once('close', finish);
    response.once('finish', finish);

    const rawHostCount = request.rawHeaders.filter((_, index) => index % 2 === 0 && request.rawHeaders[index].toLowerCase() === 'host').length;
    const forbidden = ['authorization', 'proxy-authorization', 'cookie'].some(name => request.headers[name] !== undefined);
    if (requests >= limits.maxRequests) terminateAdmission('max_requests');
    if (stopping || requests > limits.maxRequests) return reply(response, 503, 'server request limit reached\n');
    if (rawHostCount !== 1 || request.headers.host !== expectedHost || forbidden) return reply(response, 400, 'request boundary rejected\n');
    if ((request.method !== 'GET' && request.method !== 'HEAD') || !safeRoute(request.url)) return reply(response, 400, 'method or raw path rejected\n');
    if ((request.headers['content-length'] !== undefined && request.headers['content-length'] !== '0') || request.headers['transfer-encoding'] !== undefined) return reply(response, 413, 'request bodies are not accepted\n');
    const entry = entries.get(request.url);
    if (!entry) return reply(response, 404, 'not found\n');
    const send = () => {
      if (response.destroyed || response.writableEnded || stopping) return;
      response.writeHead(200, responseHeaders(entry, entry.bytes.length));
      response.end(request.method === 'HEAD' ? undefined : entry.bytes);
    };
    if (!entry.delayMs) return send();
    const delayed = setTimeout(() => { timers.delete(delayed); send(); }, entry.delayMs);
    timers.add(delayed);
    response.once('close', () => { clearTimeout(delayed); timers.delete(delayed); });
  });
  server.maxConnections = limits.maxConnections;
  server.requestTimeout = limits.requestMs;
  server.headersTimeout = limits.requestMs;
  server.keepAliveTimeout = Math.min(1_000, limits.requestMs);
  server.on('connection', socket => {
    chargeConnectionAttempt();
    sockets.add(socket);
    socket.setTimeout(limits.requestMs, () => socket.destroy());
    socket.once('close', () => sockets.delete(socket));
  });
  // Once maxConnections is saturated Node rejects new sockets through `drop`
  // instead of `connection`; those attempts still consume the admission budget.
  server.on('drop', chargeConnectionAttempt);
  server.on('clientError', (error, socket) => {
    if (!socket.writable) return socket.destroy();
    const status = error?.code === 'HPE_HEADER_OVERFLOW' ? '431 Request Header Fields Too Large' : '400 Bad Request';
    socket.end(`HTTP/1.1 ${status}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n`);
  });

  await new Promise((resolve, reject) => {
    let settled = false;
    const startupError = error => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      reject(error);
    };
    const timeout = setTimeout(() => {
      if (settled) return;
      settled = true;
      server.close();
      reject(Error('Owned server startup timed out'));
    }, limits.startupMs);
    server.once('error', startupError);
    server.listen({ host: '127.0.0.1', port: 0, exclusive: true }, () => {
      if (settled) { server.close(); return; }
      settled = true;
      clearTimeout(timeout);
      server.off('error', startupError);
      resolve();
    });
  });
  server.on('error', error => terminateAdmission(`server_error:${typeof error?.code === 'string' ? error.code : 'unknown'}`));
  const address = server.address();
  if (!address || typeof address === 'string' || address.address !== '127.0.0.1') {
    server.close();
    throw Error('Owned server did not bind the exact IPv4 loopback address');
  }
  expectedHost = `127.0.0.1:${address.port}`;

  let stopPromise;
  const stop = () => {
    if (stopPromise) return stopPromise;
    stopping = true;
    if (!terminalReason) terminalReason = 'manual_stop';
    clearTimeout(terminalTimer);
    terminalTimer = undefined;
    for (const timer of timers) clearTimeout(timer);
    timers.clear();
    stopPromise = new Promise((resolve, reject) => {
      let completed = false;
      let forced;
      let failed;
      const complete = error => {
        if (completed) return;
        completed = true;
        clearTimeout(forced);
        clearTimeout(failed);
        shutdownTimerCount = 0;
        if (error && error.code !== 'ERR_SERVER_NOT_RUNNING') reject(error); else resolve();
      };
      server.close(error => setImmediate(() => { listenerClosed = true; complete(error); }));
      if (typeof server.closeIdleConnections === 'function') server.closeIdleConnections();
      for (const socket of sockets) socket.end();
      forced = setTimeout(() => {
        for (const socket of sockets) socket.destroy();
        if (typeof server.closeAllConnections === 'function') server.closeAllConnections();
      }, limits.shutdownMs);
      failed = setTimeout(() => {
        for (const socket of sockets) socket.destroy();
        complete(Error('Owned server did not confirm closure before its shutdown deadline'));
      }, limits.shutdownMs * 2);
      shutdownTimerCount = 2;
    });
    return stopPromise;
  };

  return Object.freeze({ origin: `http://${expectedHost}`, port: address.port, stop, observation: () => Object.freeze({ requests, connection_attempts: connectionAttempts, active_sockets: [...sockets].filter(socket => !socket.destroyed).length, pending_timers: timers.size + (terminalTimer ? 1 : 0) + shutdownTimerCount, stopping, terminal_reason: terminalReason, listener_closed: listenerClosed }) });
}

module.exports = { freezeInventory, startOwnedServer };
