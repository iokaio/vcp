// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { createRequire } = require('node:module');

// Only these loaded authoring modules see the simulated host. Filesystem helpers
// retain the real platform; no global process state or production guard changes.
// All executable bytes and provider calls in these tests are synthetic.
function authoringHost(platform = 'win32', modulesForTest = {}) {
  const directory = path.resolve(__dirname, '../../../scripts/evals');
  const hostProcess = new Proxy(process, { get(target, property) {
    if (property === 'platform') return platform;
    if (property === 'env') return { ...target.env, SystemRoot: target.env.SystemRoot || os.tmpdir() };
    return Reflect.get(target, property);
  } });
  const modules = new Map();
  function load(name) {
    if (modules.has(name)) return modules.get(name).exports;
    const filename = path.join(directory, name), module = { exports: {} };
    modules.set(name, module);
    const actualRequire = createRequire(filename);
    const localRequire = requested => modulesForTest[requested] || (['./authoring-prepare.cjs', './authoring-runner.cjs', './authoring-followup.cjs'].includes(requested) ? load(requested.slice(2)) : actualRequire(requested));
    const compile = new Function('exports', 'require', 'module', '__filename', '__dirname', 'process', fs.readFileSync(filename, 'utf8'));
    compile(module.exports, localRequire, module, filename, directory, hostProcess);
    return module.exports;
  }
  return { prep: load('authoring-prepare.cjs'), runner: load('authoring-runner.cjs'), followup: load('authoring-followup.cjs') };
}
module.exports = { authoringHost };
