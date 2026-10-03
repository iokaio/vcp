// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const LIMIT = 32 * 1024 * 1024;
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const check = (condition, message) => { if (!condition) throw Error('Cargo timings: ' + message); };
const seconds = value => typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= 86400;
function plain(file) {
  for (let current = path.resolve(file);; current = path.dirname(current)) {
    check(!fs.lstatSync(current).isSymbolicLink(), 'redirected report or ancestor');
    if (current === path.dirname(current)) break;
  }
}
function read(file) {
  plain(file);
  const fd = fs.openSync(file, 'r');
  try {
    const stat = fs.fstatSync(fd);
    check(stat.isFile() && stat.size <= LIMIT, 'ordinary report within 32 MiB required');
    const bytes = Buffer.alloc(stat.size + 1);
    let offset = 0, count;
    while (offset < bytes.length && (count = fs.readSync(fd, bytes, offset, bytes.length - offset, offset)) > 0) offset += count;
    check(offset === stat.size, 'report changed while reading');
    return bytes.subarray(0, offset);
  } finally { fs.closeSync(fd); }
}
function data(html, name) {
  const matches = [...html.matchAll(new RegExp('^const ' + name + ' = (\\[[\\s\\S]*?\\]);\\r?$', 'gm'))];
  check(matches.length === 1, 'one ' + name + ' JSON array required');
  // Parse JSON only. The report is untrusted HTML; never evaluate its script.
  return JSON.parse(matches[0][1]);
}
function summarize(bytes) {
  check(bytes.length <= LIMIT, 'report exceeds 32 MiB');
  const html = bytes.toString('utf8'), units = data(html, 'UNIT_DATA'), concurrency = data(html, 'CONCURRENCY_DATA');
  check(Array.isArray(units) && units.length <= 10000 && Array.isArray(concurrency) && concurrency.length <= 40000, 'too many observations');
  const result = { schema: 'vcp-cargo-timings/1', report_sha256: hash(bytes),
    measurement: 'Cargo unit wall times and scheduler concurrency; excludes internal rustc concurrency. Missing compiler sections are null.',
    units: units.map(row => {
      check(row && typeof row.name === 'string' && /^[A-Za-z0-9_-]{1,200}$/.test(row.name) && typeof row.version === 'string' && /^[A-Za-z0-9.+_-]{1,100}$/.test(row.version) &&
        typeof row.target === 'string' && /^[A-Za-z0-9 _"()-]{0,300}$/.test(row.target) && seconds(row.start) && seconds(row.duration), 'invalid unit observation');
      let frontend = null, codegen = null;
      if (row.sections != null) {
        check(Array.isArray(row.sections) && row.sections.length <= 10, 'invalid compiler sections');
        const seen = new Set();
        for (const section of row.sections) {
          check(Array.isArray(section) && section.length === 2 && ['frontend', 'codegen', 'link'].includes(section[0]) && !seen.has(section[0]) &&
            seconds(section[1]?.start) && seconds(section[1]?.end) && section[1].end >= section[1].start && section[1].end <= row.duration + 0.02, 'invalid compiler section');
          seen.add(section[0]);
          if (section[0] === 'frontend') frontend = section[1].end - section[1].start;
          if (section[0] === 'codegen') codegen = section[1].end - section[1].start;
        }
      }
      return { name: row.name, version: row.version, target: row.target, start_seconds: row.start, duration_seconds: row.duration,
        frontend_seconds: frontend, codegen_seconds: codegen };
    }), concurrency: [], maximum_active_units: 0 };
  let previous = -1;
  for (const row of concurrency) {
    check(seconds(row.t) && row.t >= previous && ['active', 'waiting', 'inactive'].every(key => Number.isSafeInteger(row[key]) && row[key] >= 0 && row[key] <= 10000), 'invalid concurrency observation');
    previous = row.t;
    result.concurrency.push({ elapsed_seconds: row.t, active: row.active, waiting: row.waiting, inactive: row.inactive });
    result.maximum_active_units = Math.max(result.maximum_active_units, row.active);
  }
  result.observed_seconds = Math.max(0, previous, ...result.units.map(row => row.start_seconds + row.duration_seconds));
  return result;
}
function capture(input, directory) {
  plain(directory);
  check(fs.statSync(directory).isDirectory(), 'ordinary evidence directory required');
  const bytes = read(input), summary = Buffer.from(JSON.stringify(summarize(bytes)) + '\n');
  fs.writeFileSync(path.join(directory, 'cargo-timing.html'), bytes, { flag: 'wx' });
  fs.writeFileSync(path.join(directory, 'cargo-timings.json'), summary, { flag: 'wx' });
  return { schema: 'vcp-cargo-timings-binding/1', report_file: 'cargo-timing.html', report_sha256: hash(bytes),
    summary_file: 'cargo-timings.json', summary_sha256: hash(summary) };
}
function validateBinding(binding) {
  check(binding?.schema === 'vcp-cargo-timings-binding/1' && binding.report_file === 'cargo-timing.html' && binding.summary_file === 'cargo-timings.json' &&
    [binding.report_sha256, binding.summary_sha256].every(value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value)), 'invalid report binding');
}
function verify(directory, binding) {
  validateBinding(binding);
  const report = read(path.join(directory, binding.report_file)), summary = read(path.join(directory, binding.summary_file));
  check(hash(report) === binding.report_sha256 && hash(summary) === binding.summary_sha256, 'report or summary digest differs');
  check(JSON.stringify(JSON.parse(summary.toString('utf8'))) === JSON.stringify(summarize(report)), 'summary differs from report observations');
  return summary;
}
if (require.main === module) {
  try {
    check(process.argv.length === 4, 'use <Cargo HTML report> <build evidence directory>');
    process.stdout.write(JSON.stringify(capture(process.argv[2], process.argv[3])) + '\n');
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { summarize, capture, validateBinding, verify };
