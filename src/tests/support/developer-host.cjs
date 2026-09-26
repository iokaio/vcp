// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { createRequire } = require('node:module');

// Only these loaded developer campaign modules see the simulated host. Filesystem
// helpers keep the real platform; no global process state or production guard
// changes. Two stand-ins are test-only and scoped to this loader:
// - `nodeSha256` replaces the owner-approved grading Node digest, so a synthetic
//   file can stand in for the installed Node (tests assert the real constant);
// - `gitCommonDir` redirects the campaign claim to a temporary directory, so no
//   test ever writes the real repository's single campaign claim.
// All executable bytes and provider calls in these tests are synthetic.
const pinned = /const nodeSha256 = '([a-f0-9]{64})';/;
function developerHost({ platform = 'win32', nodeSha256, gitCommonDir } = {}) {
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
    const localRequire = requested => {
      if (requested === 'node:child_process' && gitCommonDir) {
        const actual = actualRequire(requested);
        return { ...actual, execFileSync: (command, args, options) => command === 'git' && args.includes('--git-common-dir') ? gitCommonDir + '\n' : actual.execFileSync(command, args, options) };
      }
      if (['./developer-prepare.cjs', './developer-runner.cjs', './developer-review.cjs'].includes(requested)) return load(requested.slice(2));
      return actualRequire(requested);
    };
    let source = fs.readFileSync(filename, 'utf8');
    if (name === 'developer-prepare.cjs' && nodeSha256) {
      if (!pinned.test(source)) throw Error('Pinned Node digest declaration not found');
      source = source.replace(pinned, `const nodeSha256 = '${nodeSha256}';`);
    }
    const compile = new Function('exports', 'require', 'module', '__filename', '__dirname', 'process', source);
    compile(module.exports, localRequire, module, filename, directory, hostProcess);
    return module.exports;
  }
  return { prep: load('developer-prepare.cjs'), runner: load('developer-runner.cjs'), review: load('developer-review.cjs') };
}
module.exports = { developerHost, pinned };
