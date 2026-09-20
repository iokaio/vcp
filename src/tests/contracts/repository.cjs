// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
function visible(text) {
  return text.replace(/^([ \t]*)(`{3,}|~{3,})[^\n]*\n[\s\S]*?^\1\2[ \t]*\r?$/gm, '');
}
function anchors(text) {
  const counts = new Map(), result = new Set();
  for (const match of visible(text).matchAll(/^#{1,6}\s+(.+?)\s*#*\s*$/gm)) {
    const slug = match[1].replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/<[^>]+>/g, '').toLowerCase()
      .replace(/[^\p{L}\p{N}\p{M}_\-\s]/gu, '').replace(/\s/g, '-');
    const count = counts.get(slug) || 0;
    counts.set(slug, count + 1); result.add(slug + (count ? '-' + count : ''));
  }
  for (const match of text.matchAll(/<a\s+(?:name|id)=["']([^"']+)["']/g)) result.add(match[1]);
  return result;
}
function dependencies(text) {
  if (text === 'None') return [];
  const result = [];
  for (const group of text.split(',')) {
    const match = group.trim().match(/^(P\d+)-(\d{2})(?:(…\d{2})|((?:\/\d{2})+))?$/);
    if (!match) throw Error('Invalid dependencies: ' + text);
    const [, phase, first, range, suffix] = match;
    result.push(phase + '-' + first);
    if (range) {
      if (+range.slice(1) <= +first) throw Error('Invalid range: ' + text);
      for (let n = +first + 1; n <= +range.slice(1); n++) result.push(phase + '-' + String(n).padStart(2, '0'));
    }
    if (suffix) for (const n of suffix.slice(1).split('/')) result.push(phase + '-' + n);
  }
  return result.sort();
}
function graphErrors(nodes) {
  const errors = [], seen = new Set(), active = new Set();
  function visit(id) {
    if (active.has(id)) { errors.push('Dependency cycle: ' + id); return; }
    if (seen.has(id)) return;
    if (!nodes.has(id)) { errors.push('Unknown dependency: ' + id); return; }
    active.add(id);
    for (const dep of nodes.get(id)) visit(dep);
    active.delete(id); seen.add(id);
  }
  for (const id of nodes.keys()) visit(id);
  return errors;
}
function checkRepository(root) {
  const git = args => execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  const files = [...new Set(git(['ls-files', '--cached', '--others', '--exclude-standard', '-z']).split('\0').filter(Boolean))]
    .filter(file => fs.existsSync(path.join(root, file)));
  const read = file => fs.readFileSync(path.join(root, file), 'utf8');
  // Imported upstream documentation is byte-verified by its provenance case;
  // its original links refer to an upstream tree/site, not VCP's documentation.
  const errors = [], markdown = files.filter(file => file.endsWith('.md') && !/^src\/third_party\/(codex|munarium)\//.test(file));
  let links = 0;
  for (const file of markdown) {
    for (const match of visible(read(file)).matchAll(/!?\[[^\]]*\]\(([^)\n]+)\)/g)) {
      const target = match[1].replace(/\s+["'][^"']*["']$/, '').replace(/^<|>$/g, '');
      if (/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(target)) continue;
      links++;
      const [relative, fragment] = target.split('#').map(decodeURIComponent);
      const destination = relative ? path.resolve(root, path.dirname(file), relative) : path.join(root, file);
      if (!fs.existsSync(destination)) errors.push(file + ': missing target ' + target);
      else if (fragment && destination.endsWith('.md') && !anchors(fs.readFileSync(destination, 'utf8')).has(fragment)) errors.push(file + ': missing anchor ' + target);
    }
  }
  if (!fs.readFileSync(path.join(root, 'AGENTS.md')).equals(fs.readFileSync(path.join(root, 'CLAUDE.md')))) errors.push('Agent guidance differs');
  for (const file of files.filter(f => /^(scripts|src\/tests)\//.test(f) && /\.(cjs|ps1)$/.test(f))) {
    if (!read(file).slice(0, 200).includes('SPDX-License-Identifier: Apache-2.0')) errors.push('Missing SPDX: ' + file);
  }
  const ledger = [...read('docs/plan/20-traceability.md').matchAll(/^\| (P\d+-\d{2}) [^|]+\| ([^|]+)\| ([^|]+)\| ([^|]+)\| ([^|]+)\|$/gm)]
    .map(m => ({ id: m[1], deps: dependencies(m[2].trim()), owner: m[3].trim(), state: m[5].trim().toLowerCase() }));
  const architecture = new Map([...read('docs/architecture/vcp-what.md').matchAll(/^\| (P\d+-\d{2}) [^|]+\| ([^|]+)\|/gm)].map(m => [m[1], dependencies(m[2].trim())]));
  const owners = new Map();
  for (const file of markdown.filter(f => /^docs\/plan\/[^/]+$/.test(f))) {
    for (const match of read(file).matchAll(/^## (P\d+-\d{2}) /gm)) {
      owners.set(match[1], [...(owners.get(match[1]) || []), path.basename(file)]);
    }
  }
  const nodes = new Map(), states = new Map(ledger.map(row => [row.id, row.state]));
  for (const row of ledger) {
    if (nodes.has(row.id)) errors.push('Duplicate task: ' + row.id);
    nodes.set(row.id, row.deps);
    if (owners.get(row.id)?.length !== 1 || !row.owner.includes('](' + owners.get(row.id)?.[0] + '#')) errors.push('Invalid owner: ' + row.id);
    if (JSON.stringify(architecture.get(row.id)) !== JSON.stringify(row.deps)) errors.push('Architecture dependency mismatch: ' + row.id);
    if (!['planned', 'in_progress', 'blocked', 'implemented_unverified', 'complete'].includes(row.state)) errors.push('Invalid state: ' + row.id);
    if (row.state === 'complete' && row.deps.some(dep => states.get(dep) !== 'complete')) errors.push('Incomplete prerequisite: ' + row.id);
  }
  if (ledger.length !== 68 || owners.size !== 68 || architecture.size !== 68) errors.push('Task inventory must contain 68 items');
  const ledgerText = read('docs/plan/20-traceability.md');
  for (const [prefix, count] of [['A', 17], ['FR-', 17], ['I-', 19]]) {
    const ids = [...ledgerText.matchAll(new RegExp('^\\| (' + prefix + '\\d{2}) ', 'gm'))].map(m => m[1]);
    for (let n = 1; n <= count; n++) {
      const id = prefix + String(n).padStart(2, '0');
      if (ids.filter(found => found === id).length !== 1) errors.push('Requirement coverage missing or duplicated: ' + id);
    }
    if (ids.length !== count) errors.push('Unexpected requirement inventory: ' + prefix);
  }
  if (files.filter(file => /^docs\/adr\/\d{3}-[^/]+\.md$/.test(file)).length !== 29) errors.push('ADR inventory must contain 29 records');
  errors.push(...graphErrors(nodes));
  const closure = new Set();
  function include(id) { if (closure.has(id)) return; closure.add(id); for (const dep of nodes.get(id) || []) include(dep); }
  include('P8-05');
  const release = [...nodes.keys()].filter(id => !/^P(?:4|9|10)-/.test(id));
  if (closure.size !== 56 || release.some(id => !closure.has(id)) || [...closure].some(id => /^P(?:4|9|10)-/.test(id))) errors.push('First-release closure must contain all 56 release tasks and no deferred tasks');
  try { git(['diff', '--check', 'HEAD', '--', '.', ':!src/third_party/codex', ':!src/third_party/munarium']); } catch { errors.push('git diff --check HEAD failed'); }
  return { markdown_files: markdown.length, relative_links: links, tasks: ledger.length, release_tasks: closure.size, errors };
}
if (require.main === module) {
  const result = checkRepository(path.resolve(__dirname, '../../..'));
  console.log(JSON.stringify(result, null, 2));
  process.exitCode = result.errors.length ? 1 : 0;
}
module.exports = { anchors, dependencies, graphErrors, checkRepository };
