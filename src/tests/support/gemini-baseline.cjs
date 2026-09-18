// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const SUITES = Object.freeze({
  'src/policy/policy-engine.test.ts': 155,
  'src/scheduler/scheduler.test.ts': 43,
  'src/tools/tools.test.ts': 15,
  'src/policy/stable-stringify.test.ts': 21,
  'src/skills/skillLoader.test.ts': 15,
  'src/skills/skillManager.test.ts': 10,
  'src/tools/mcp-client.test.ts': 78,
  'src/tools/mcp-tool.test.ts': 65
});
function parseArgs(argv) {
  const result = { prepare: false };
  for (let i = 0; i < argv.length; i++) {
    const key = argv[i];
    if (key === '--prepare' && !result.prepare) { result.prepare = true; continue; }
    if (!['--source', '--output-root'].includes(key) || result[key] || !argv[i + 1] || argv[i + 1].startsWith('--')) throw Error('Invalid Gemini qualification arguments');
    result[key] = argv[++i];
  }
  if (!result['--source'] || !result['--output-root']) throw Error('Required: --source <pinned checkout> --output-root <evidence directory> [--prepare]');
  return result;
}
function resolveProspective(file) {
  let current = path.resolve(file), suffix = [];
  while (!fs.existsSync(current)) {
    suffix.unshift(path.basename(current));
    const parent = path.dirname(current);
    if (parent === current) throw Error('No existing output ancestor');
    current = parent;
  }
  return path.join(fs.realpathSync(current), ...suffix);
}
function outsideSource(source, output) {
  const canonicalSource = resolveProspective(source), canonicalOutput = resolveProspective(output);
  const normalize = value => process.platform === 'win32' ? value.toLowerCase() : value;
  const root = normalize(canonicalSource), target = normalize(canonicalOutput);
  if (target === root || target.startsWith(root + path.sep)) throw Error('Gemini evidence must be outside the source checkout');
  return { source: canonicalSource, outputRoot: canonicalOutput };
}
function verifySource(source, expected) {
  if (!/^[a-f0-9]{40}$/.test(expected.commit) || !/^[a-f0-9]{40}$/.test(expected.tree)) throw Error('Invalid pinned source identity');
  const git = args => execFileSync('git', ['-c', 'core.longpaths=true', '-c', 'safe.directory=' + source.replaceAll('\\', '/'), '-C', source, ...args],
    { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 8 * 1024 * 1024 });
  if (git(['rev-parse', 'HEAD']).trim() !== expected.commit || git(['rev-parse', 'HEAD^{tree}']).trim() !== expected.tree) throw Error('Gemini checkout does not match its immutable pin');
  if (git(['status', '--porcelain=v1', '--untracked-files=all']).trim()) throw Error('Gemini baseline requires a clean checkout');
  return { commit: expected.commit, tree: expected.tree, git: git(['--version']).trim() };
}
function validateResults(report, core) {
  const total = Object.values(SUITES).reduce((a, b) => a + b, 0);
  if (report.success !== true || report.numTotalTests !== total || report.numPassedTests !== total ||
      report.numFailedTests !== 0 || report.numFailedTestSuites !== 0 || report.numPendingTests !== 0 || report.numTodoTests !== 0 ||
      !Array.isArray(report.testResults) || report.testResults.length !== Object.keys(SUITES).length || report.snapshot?.failure) throw Error('Incomplete or failing Gemini test result');
  const seen = new Set();
  for (const suite of report.testResults) {
    const relative = path.relative(core, suite.name).split(path.sep).join('/');
    if (!Object.hasOwn(SUITES, relative) || seen.has(relative) || suite.status !== 'passed' ||
        suite.assertionResults?.length !== SUITES[relative] || suite.assertionResults.some(item => item.status !== 'passed' || item.failureMessages?.length)) throw Error('Unexpected or incomplete Gemini suite: ' + relative);
    seen.add(relative);
  }
  return { suites: seen.size, tests: total, status: 'pass' };
}
module.exports = { SUITES, parseArgs, outsideSource, verifySource, validateResults };
