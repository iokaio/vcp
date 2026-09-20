// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { digest } = require('../../src/tests/support/harness.cjs');

// Scope the existing harness's content/diff identity convention to the native
// comparison's local sources, avoiding unrelated vendored Codex history.
const inputs = [
  'src/crates', 'src/evals/memory', 'scripts/evals',
  'src/tests/support/harness.cjs',
  'src/third_party/munarium/server/src',
  'src/third_party/munarium/server/Cargo.toml',
  'src/third_party/munarium/server/Cargo.lock',
  'src/third_party/codex/codex-rs/Cargo.toml',
  'src/third_party/codex/codex-rs/Cargo.lock',
  'src/third_party/codex/codex-rs/rust-toolchain.toml',
];
const optional = ['.cargo', 'src/.cargo', 'src/third_party/codex/codex-rs/.cargo'];
function sourceIdentity(root) {
  const selected = [...inputs, ...optional.filter(relative => fs.existsSync(path.join(root, relative)))];
  const files = [];
  function visit(relative) {
    const filename = path.join(root, relative), info = fs.lstatSync(filename);
    if (info.isSymbolicLink()) throw Error('Source identity refuses linked input: ' + relative);
    if (info.isDirectory()) {
      for (const entry of fs.readdirSync(filename).sort()) visit(relative + '/' + entry);
    } else if (info.isFile()) {
      files.push({ path: relative, bytes: info.size, sha256: digest(fs.readFileSync(filename)) });
    } else throw Error('Unsupported source identity input: ' + relative);
  }
  selected.forEach(visit);
  files.sort((a, b) => a.path.localeCompare(b.path, 'en'));
  const git = args => execFileSync('git', ['-c', 'safe.directory=' + root.replaceAll('\\', '/'), ...args], {
    cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 64 * 1024 * 1024,
  });
  const status = git(['status', '--porcelain=v1', '-z', '--untracked-files=all', '--', ...selected]);
  const untracked = git(['ls-files', '--others', '--exclude-standard', '-z', '--', ...selected])
    .toString().split('\0').filter(Boolean).sort();
  return {
    schema: 'p5-08-bounded-source-identity/1',
    commit: git(['rev-parse', 'HEAD']).toString().trim(),
    git_version: git(['--version']).toString().trim(),
    dirty: status.length > 0, scope: selected, files,
    content_sha256: digest(JSON.stringify(files)),
    status_sha256: digest(status),
    tracked_diff_sha256: digest(git(['diff', '--no-ext-diff', '--no-textconv', '--binary', 'HEAD', '--', ...selected])),
    untracked_sha256: digest(JSON.stringify(untracked.map(relative => [relative, digest(fs.readFileSync(path.join(root, relative)))]))),
  };
}
if (require.main === module) {
  process.stdout.write(JSON.stringify(sourceIdentity(path.resolve(__dirname, '../..'))) + '\n');
}
module.exports = { sourceIdentity };
