// SPDX-License-Identifier: Apache-2.0
'use strict';
// Finite fixture contract/server only. No browser, subprocess or workspace code execution.
const fs = require('node:fs'), path = require('node:path'), http = require('node:http');
const { createHash } = require('node:crypto');
const MIME = Object.freeze({ '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.json': 'application/json', '.txt': 'text/plain' });
const READY = '/__vcp_ready';
function keys(value, expected) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).sort().join(',') !== [...expected].sort().join(',')) throw Error('Unknown or missing contract fields');
}
function text(value, max) { return typeof value === 'string' && value.length > 0 && Buffer.byteLength(value) <= max && !/[\x00-\x1f\x7f]/.test(value); }
function portable(value) {
  return text(value, 512) && !/[\\:%?#*"<>|]/.test(value) && !value.startsWith('/') && value.split('/').every(part => part && part !== '.' && part !== '..' && !/[. ]$/.test(part) && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part));
}
function route(value) { return typeof value === 'string' && value.startsWith('/') && portable(value.slice(1)); }
function validate(spec) {
  keys(spec, ['schema_version', 'origin', 'application_id', 'root', 'files', 'actions', 'timeout_ms']);
  if (spec.schema_version !== 1 || !text(spec.application_id, 128) || !/^[a-zA-Z0-9_-]+$/.test(spec.application_id) || typeof spec.root !== 'string' || !path.isAbsolute(spec.root)) throw Error('Invalid application identity or root');
  const match = typeof spec.origin === 'string' && /^http:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})$/.exec(spec.origin);
  if (!match || Number(match[1]) > 65535) throw Error('Exact literal loopback origin required');
  if (!Number.isSafeInteger(spec.timeout_ms) || spec.timeout_ms < 1 || spec.timeout_ms > 60000) throw Error('Invalid lifetime bound');
  if (!Array.isArray(spec.files) || spec.files.length < 1 || spec.files.length > 32 || !Array.isArray(spec.actions) || spec.actions.length < 1 || spec.actions.length > 32) throw Error('Finite files/actions required');
  const urls = new Set(), paths = new Set(); let total = 0;
  for (const file of spec.files) {
    keys(file, ['url', 'path', 'mime', 'bytes', 'sha256']);
    if (!route(file.url) || file.url === READY || !portable(file.path) || urls.has(file.url) || paths.has(file.path.toLowerCase()) || !MIME[path.extname(file.path)] || file.mime !== MIME[path.extname(file.path)] || !Number.isSafeInteger(file.bytes) || file.bytes < 0 || file.bytes > 65536 || typeof file.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(file.sha256)) throw Error('Invalid mapped source');
    urls.add(file.url); paths.add(file.path.toLowerCase()); total += file.bytes;
  }
  if (total > 262144) throw Error('Total source byte bound');
  for (const action of spec.actions) {
    if (!action || typeof action !== 'object') throw Error('Invalid action');
    const shape = { navigate: ['kind', 'path'], click: ['kind', 'selector'], fill: ['kind', 'selector', 'value'], key: ['kind', 'key'], assert_text: ['kind', 'selector', 'text'] }[action.kind];
    if (!shape) throw Error('Unsupported action');
    keys(action, shape);
    if (action.kind === 'navigate' && (!route(action.path) || !urls.has(action.path))) throw Error('Navigation must name mapped origin path');
    if ('selector' in action && !text(action.selector, 256)) throw Error('Invalid selector');
    if ('value' in action && (typeof action.value !== 'string' || Buffer.byteLength(action.value) > 4096 || /[\x00-\x1f\x7f]/.test(action.value))) throw Error('Invalid fill value');
    if ('text' in action && !text(action.text, 4096)) throw Error('Invalid expected text');
    if (action.kind === 'key' && !['Tab', 'Enter', 'Escape', 'Space', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(action.key)) throw Error('Unsupported key');
  }
  return structuredClone(spec);
}
function noLinks(absolute) {
  const parts = path.resolve(absolute).slice(path.parse(absolute).root.length).split(path.sep).filter(Boolean);
  let current = path.parse(absolute).root;
  for (const part of parts) { current = path.join(current, part); if (fs.lstatSync(current).isSymbolicLink()) throw Error('Linked source path rejected'); }
}
function capture(spec) {
  noLinks(spec.root);
  if (!fs.lstatSync(spec.root).isDirectory()) throw Error('Source root is not a directory');
  const contents = new Map();
  for (const file of spec.files) {
    const absolute = path.join(spec.root, file.path); noLinks(absolute);
    const fd = fs.openSync(absolute, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
    try {
      const before = fs.fstatSync(fd);
      if (!before.isFile() || before.size !== file.bytes) throw Error('Source size or type changed');
      const bytes = Buffer.alloc(file.bytes + 1); let length = 0;
      while (length < bytes.length) { const got = fs.readSync(fd, bytes, length, bytes.length - length, length); if (!got) break; length += got; }
      const after = fs.fstatSync(fd); noLinks(absolute);
      const named = fs.statSync(absolute);
      if (length !== file.bytes || before.size !== after.size || before.mtimeMs !== after.mtimeMs || named.dev !== after.dev || named.ino !== after.ino || createHash('sha256').update(bytes.subarray(0, length)).digest('hex') !== file.sha256) throw Error('Source identity changed');
      contents.set(file.url, { bytes: bytes.subarray(0, length), mime: file.mime });
    } finally { fs.closeSync(fd); }
  }
  return contents;
}
async function start(input) {
  const spec = validate(input), contents = capture(spec), sockets = new Set();
  const authority = spec.origin.slice('http://'.length); let requests = 0, timer, closed, closing;
  const server = http.createServer({ maxHeaderSize: 8192, requestTimeout: 5000, headersTimeout: 5000 }, (req, res) => {
    const send = (status, body, mime = 'text/plain') => { res.writeHead(status, { 'Content-Type': mime + '; charset=utf-8', 'Content-Length': Buffer.byteLength(body), 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'Connection': 'close' }); res.end(req.method === 'HEAD' ? undefined : body); };
    if (++requests > 128) { send(429, 'request limit'); void close(); return; }
    if (req.headers.host !== authority || (req.headers.origin !== undefined && req.headers.origin !== spec.origin) || !['GET', 'HEAD'].includes(req.method) || req.headers['transfer-encoding'] !== undefined || (req.headers['content-length'] !== undefined && req.headers['content-length'] !== '0')) { send(403, 'request outside fixture contract'); return; }
    if (req.url === READY) { send(200, JSON.stringify({ application_id: spec.application_id, origin: spec.origin }), 'application/json'); return; }
    if (!route(req.url)) { send(400, 'invalid route'); return; }
    const file = contents.get(req.url); if (!file) { send(404, 'unmapped source'); return; }
    send(200, file.bytes, file.mime);
  });
  server.maxHeadersCount = 16; server.maxConnections = 8; server.setTimeout(5000, socket => socket.destroy());
  server.on('connection', socket => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); });
  function close() {
    if (closing) return closing;
    clearTimeout(timer);
    closing = new Promise(resolve => { server.close(() => { closed = true; resolve(); }); for (const socket of sockets) socket.destroy(); });
    return closing;
  }
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(Number(authority.split(':')[1]), '127.0.0.1', () => { server.removeListener('error', reject); resolve(); }); });
  timer = setTimeout(() => { void close(); }, spec.timeout_ms); timer.unref();
  return Object.freeze({ origin: spec.origin, application_id: spec.application_id, close, get closed() { return closed === true; } });
}
module.exports = { validate, start };
