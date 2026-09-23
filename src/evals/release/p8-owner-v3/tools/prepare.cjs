// SPDX-License-Identifier: Apache-2.0
'use strict';
// Offline, one-shot materialization. No provider or credential client is imported.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const sourceRoot = path.resolve(__dirname, '..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const digestFile = file => sha(fs.readFileSync(file));
const inside = (root, candidate) => candidate === root || candidate.startsWith(root + path.sep);
function safeRelative(value) {
  if (typeof value !== 'string' || !value || value.includes('\\') || value.includes(':') || value.startsWith('/') || value.split('/').some(p => !p || p === '.' || p === '..' || p === '.git')) throw Error('Unsafe fixture path');
  return value;
}
function inventory(directory, prefix = '', omitGit = true) {
  const rows = [];
  for (const name of fs.readdirSync(path.join(directory, prefix)).sort()) {
    if (name === '.git') { if (omitGit) continue; throw Error('Embedded Git metadata in fixture source'); }
    const relative = prefix ? prefix + '/' + name : name;
    const file = path.join(directory, relative), stat = fs.lstatSync(file);
    if (stat.isSymbolicLink()) throw Error('Linked fixture path');
    if (stat.isDirectory()) rows.push(...inventory(directory, relative, omitGit));
    else if (stat.isFile()) rows.push({ path: relative, bytes: stat.size, sha256: digestFile(file) });
    else throw Error('Unsupported fixture path');
  }
  return rows;
}
function prepare(destination, gitExecutable) {
  destination = path.resolve(destination); gitExecutable = path.resolve(gitExecutable);
  if (fs.existsSync(destination) || !fs.existsSync(path.dirname(destination))) throw Error('New destination with an existing parent required');
  const actualParent = fs.realpathSync(path.dirname(destination));
  destination = path.join(actualParent, path.basename(destination));
  if (inside(sourceRoot, destination) || inside(destination, sourceRoot)) throw Error('Private execution root must be disjoint from source fixture');
  for (let directory = actualParent; ; directory = path.dirname(directory)) {
    if (fs.existsSync(path.join(directory, '.git'))) throw Error('Private execution root must be outside every repository');
    if (directory === path.dirname(directory)) break;
  }
  if (!fs.lstatSync(gitExecutable).isFile()) throw Error('Pinned native Git executable required');
  const manifestFile = path.join(sourceRoot, 'manifest.json'), manifestBytes = fs.readFileSync(manifestFile), manifest = JSON.parse(manifestBytes);
  if (manifest.schema !== 'p805-portable-owner-fixtures/1' || manifest.revision !== 'p8-owner-v3-node-verification-1-spdx-1') throw Error('Unknown proposal revision');
  const sourceFiles = inventory(sourceRoot, '', false).filter(row => row.path !== 'manifest.json');
  if (JSON.stringify(sourceFiles) !== JSON.stringify(manifest.files)) throw Error('Frozen source inventory changed');
  const expected = new Set(manifest.files.map(row => row.path));
  function bytes(relative) { safeRelative(relative); if (!expected.has(relative)) throw Error('Undeclared source file'); return fs.readFileSync(path.join(sourceRoot, relative)); }
  function put(root, relative, content) { safeRelative(relative); const file = path.join(root, relative); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, content); }
  const environment = {};
  for (const key of ['SystemRoot', 'WINDIR', 'PATH', 'TEMP', 'TMP']) if (process.env[key] !== undefined) environment[key] = process.env[key];
  Object.assign(environment, { GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null', GIT_OPTIONAL_LOCKS: '0', GIT_AUTHOR_DATE: '2000-01-01T00:00:00Z', GIT_COMMITTER_DATE: '2000-01-01T00:00:00Z' });
  function git(cwd, args) {
    const result = spawnSync(gitExecutable, ['-c', 'safe.directory=' + cwd.replaceAll('\\', '/'), ...args], { cwd, env: environment, encoding: 'utf8', windowsHide: true, timeout: 10000 });
    if (result.status !== 0) throw Error('Fixture Git operation failed: ' + (result.error?.message || result.stderr || result.status));
    return result.stdout;
  }
  fs.mkdirSync(destination, { mode: 0o700 });
  const receipt = { schema: 'p805-private-materialization/1', disposition: 'proposal-awaiting-owner-approval', model_calls: 0,
    fixture_manifest_sha256: sha(manifestBytes), source_revision: manifest.revision, source_root: sourceRoot,
    git_executable: gitExecutable, git_sha256: digestFile(gitExecutable), proposed_limits: manifest.proposed_limits, runs: [] };
  for (const task of manifest.cases) for (const backend of manifest.backends) {
    const id = task.id.toLowerCase() + '-' + backend, run = path.join(destination, id), workspace = path.join(run, 'workspace');
    fs.mkdirSync(workspace, { recursive: true });
    const targetFiles = new Map(task.workspace_files.map(relative => [safeRelative(relative), bytes(task.directory + '/workspace/' + relative)]));
    for (const [relative, content] of targetFiles) if (!(task.git?.untracked || []).includes(relative)) put(workspace, relative, content);
    let gitState = null;
    if (task.git) {
      for (const [relative, overlay] of Object.entries(task.git.baseline_overrides)) {
        if (!targetFiles.has(relative) || !((typeof overlay.text === 'string') !== (typeof overlay.source === 'string'))) throw Error('Invalid declarative baseline overlay');
        put(workspace, relative, overlay.source ? bytes(task.directory + '/workspace/' + safeRelative(overlay.source)) : overlay.text);
      }
      git(workspace, ['init', '--initial-branch=main']);
      git(workspace, ['config', 'user.name', 'VCP Fixture']); git(workspace, ['config', 'user.email', 'fixture@example.invalid']);
      git(workspace, ['config', 'core.autocrlf', 'false']); git(workspace, ['config', 'core.hooksPath', '.git/hooks']);
      git(workspace, ['add', '.']); git(workspace, ['commit', '-m', 'fixture: freeze pre-task baseline']);
      for (const [relative, content] of targetFiles) put(workspace, relative, content);
      if (task.git.staged.length) git(workspace, ['add', '--', ...task.git.staged.map(safeRelative)]);
      if (task.git.diff_file) put(workspace, task.git.diff_file, git(workspace, ['diff', '--', 'src']));
      const status = git(workspace, ['status', '--porcelain=v1', '--untracked-files=all']);
      if (JSON.stringify(status.trimEnd().split('\n').sort()) !== JSON.stringify([...task.git.expected_status].sort())) throw Error('Reconstructed Git state mismatch');
      gitState = { head: git(workspace, ['rev-parse', 'HEAD']).trim(), status, index_sha256: digestFile(path.join(workspace, '.git/index')), config_sha256: digestFile(path.join(workspace, '.git/config')) };
    }
    const prompt = bytes(task.directory + '/task.txt'); fs.writeFileSync(path.join(run, 'task.txt'), prompt, { flag: 'wx' });
    let foreignWorkspace = null;
    if (task.second_workspace_files) {
      foreignWorkspace = path.join(destination, 'owner-only', id, 'separate-workspace');
      for (const relative of task.second_workspace_files) put(foreignWorkspace, relative, bytes(task.directory + '/second-workspace/' + safeRelative(relative)));
    }
    receipt.runs.push({ id, case: task.id, backend, workspace, prompt: path.join(run, 'task.txt'), prompt_sha256: sha(prompt),
      workspace_files: inventory(workspace), git: gitState, other_workspace_not_authorized_context: foreignWorkspace,
      status: 'prepared-not-run', hidden_files_copied: false });
  }
  fs.writeFileSync(path.join(destination, 'preparation.json'), JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
  return { receipt: path.join(destination, 'preparation.json'), sha256: digestFile(path.join(destination, 'preparation.json')), runs: receipt.runs.length, model_calls: 0 };
}
module.exports = { prepare, inventory };
if (require.main === module) {
  try { if (process.argv.length !== 4) throw Error('Usage: prepare.cjs <new-private-directory> <pinned-git-executable>'); console.log(JSON.stringify(prepare(process.argv[2], process.argv[3]))); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
