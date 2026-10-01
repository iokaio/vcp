// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs'), path = require('node:path'), os = require('node:os');
const { execFileSync } = require('node:child_process');
const TOML = require('../../src/tests/node_modules/@iarna/toml');
const { hash, json, fileHash } = require('./provenance.cjs');
const { crateFiles } = require('./crates.cjs');
const { enumerate } = require('../package-inventory.cjs');
const TARGET = 'x86_64-pc-windows-msvc';
const legalName = name => /^(licen[cs]e|copying|copyright|notice|authors)([._-]|$)/i.test(path.posix.basename(name));
function parseTree(text) {
  const found = new Map();
  for (const line of text.replace(/^\uFEFF/, '').split(/\r?\n/).filter(Boolean)) {
    const match = /^(\S+) v(\S+?)(?: \(.*\))*$/.exec(line);
    if (!match) throw Error('Unrecognized locked dependency graph row');
    const key = match[1] + '@' + match[2];
    found.set(key, { name: match[1], version: match[2] });
  }
  if (!found.size) throw Error('Empty native dependency graph');
  return [...found.values()].sort((a, b) => (a.name + '@' + a.version).localeCompare(b.name + '@' + b.version));
}
function selectedLicense(expression) {
  if (typeof expression !== 'string' || !expression.trim()) throw Error('Missing package copying permission');
  // Retain compound AND expressions. A simple dual grant may select Apache;
  // this also avoids accidentally selecting GPL from r-efi's alternative grant.
  if (!expression.includes(' AND ') && expression.split(/\s+OR\s+|\s*\/\s*/).includes('Apache-2.0')) return 'Apache-2.0';
  if (/\b(?:A?GPL|LGPL|SSPL|BUSL)-/.test(expression)) throw Error('Unreviewed restrictive release license: ' + expression);
  return expression;
}
function collect(root, { tree, runtimeTree, workspace, cargoHome = process.env.CARGO_HOME || path.join(os.homedir(), '.cargo') }) {
  const lockPath = 'src/third_party/codex/codex-rs/Cargo.lock';
  const locked = TOML.parse(fs.readFileSync(path.join(root, lockPath), 'utf8')).package;
  const overrides = json(path.join(root, 'release/license-overrides.json'));
  if (overrides.schema !== 'vcp-release-license-overrides/1') throw Error('Unsupported license overrides');
  const runtime = new Set(parseTree(runtimeTree).map(row => row.name + '@' + row.version));
  const registryRoots = fs.readdirSync(path.join(cargoHome, 'registry/cache')).map(name => path.join(cargoHome, 'registry/cache', name));
  const texts = new Map(), sources = new Map(), missing = [], records = [];
  function retain(bytes, origin) {
    if (!bytes.length || bytes.length > 2 * 1024 * 1024) throw Error('Invalid license text size');
    const sha256 = hash(bytes), output = 'licenses/texts/' + sha256 + '.txt';
    texts.set(output, Buffer.from(bytes)); return { path: output, sha256, origin };
  }
  const standardApache = fs.readFileSync(path.join(root, 'LICENSE'));
  for (const row of parseTree(tree)) {
    const key = row.name + '@' + row.version;
    const matches = locked.filter(pkg => pkg.name === row.name && pkg.version === row.version);
    if (matches.length !== 1) throw Error('Ambiguous locked package identity: ' + key);
    const pin = matches[0]; let manifest, licenseFiles = [], vcs = null, sourceArchive = null;
    if (pin.source?.startsWith('registry+')) {
      const archives = registryRoots.map(directory => path.join(directory, row.name + '-' + row.version + '.crate')).filter(file => fs.existsSync(file));
      if (archives.length !== 1 || !/^[a-f0-9]{64}$/.test(pin.checksum)) throw Error('Missing/ambiguous cached archive: ' + key);
      const compressed = fs.readFileSync(archives[0]), base = row.name + '-' + row.version + '/';
      const extracted = path.join(cargoHome, 'registry/src', path.basename(path.dirname(archives[0])), row.name + '-' + row.version);
      const sourceFiles = new Set();
      crateFiles(compressed, pin.checksum, (name, bytes) => {
        if (!name.startsWith(base)) throw Error('Cargo archive package root mismatch');
        const relative = name.slice(base.length);
        const file = path.join(extracted, relative);
        if (fileHash(file) !== hash(bytes)) throw Error('Extracted Cargo source differs from locked archive: ' + key + '/' + relative);
        sourceFiles.add(relative);
        if (relative === 'Cargo.toml') manifest = TOML.parse(bytes.toString()).package;
        if (relative === '.cargo_vcs_info.json') vcs = JSON.parse(bytes);
        if (legalName(relative)) licenseFiles.push(retain(bytes, { kind: 'cargo-archive', package_sha256: pin.checksum, path: relative }));
        // bech32's upstream distribution puts the full MIT notice in lib.rs.
        if (row.name === 'bech32' && row.version === '0.9.1' && relative === 'src/lib.rs') {
          const header = bytes.toString().split('\n\n')[0];
          if (header.includes('Permission is hereby granted') && header.includes('THE SOFTWARE.'))
            licenseFiles.push(retain(Buffer.from(header + '\n'), { kind: 'cargo-source-header', package_sha256: pin.checksum, path: relative }));
        }
      });
      function checkExtracted(directory, prefix = '') {
        for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
          const name = prefix + entry.name;
          if (entry.isSymbolicLink()) throw Error('Linked extracted Cargo source: ' + key);
          if (entry.isDirectory()) checkExtracted(path.join(directory, entry.name), name + '/');
          else if (!sourceFiles.has(name) && !['.cargo-ok', '.cargo-checksum.json'].includes(name)) throw Error('Unexpected extracted Cargo source: ' + key + '/' + name);
        }
      }
      checkExtracted(extracted);
      if (!manifest || manifest.name !== row.name || manifest.version !== row.version) throw Error('Cargo archive manifest identity mismatch');
      if (manifest.license === 'MPL-2.0' || overrides.records.some(record => record.packages.includes(key) && record.kind === 'published-declaration')) {
        sourceArchive = 'licenses/sources/' + row.name + '-' + row.version + '.crate'; sources.set(sourceArchive, compressed);
      }
    } else if (pin.source?.startsWith('git+')) {
      const revision = pin.source.split('#')[1];
      if (!/^[a-f0-9]{40}$/.test(revision || '')) throw Error('Unpinned Git dependency');
      const checkoutBase = path.join(cargoHome, 'git/checkouts'); let checkout;
      for (const group of fs.readdirSync(checkoutBase)) for (const directory of fs.readdirSync(path.join(checkoutBase, group))) {
        if (revision.startsWith(directory)) { if (checkout) throw Error('Ambiguous Git cache'); checkout = path.join(checkoutBase, group, directory); }
      }
      if (!checkout) throw Error('Missing pinned Git cache: ' + key);
      const git = args => execFileSync('git', ['-c', 'safe.directory=' + checkout.replaceAll('\\', '/'), '-C', checkout, ...args],
        { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 16 * 1024 * 1024 });
      const changed = git(['status', '--porcelain=v1', '--untracked-files=all']).toString().split(/\r?\n/).filter(line => line && line !== '?? .cargo-ok');
      if (git(['rev-parse', 'HEAD']).toString().trim() !== revision || changed.length ||
          !git(['ls-files', '-v', '-z']).toString().split('\0').filter(Boolean).every(row => row.startsWith('H '))) throw Error('Modified or unverified pinned Git cache: ' + key);
      const paths = git(['ls-tree', '-r', '--name-only', revision]).toString().trim().split('\n');
      let rootManifest = {}, selectedManifest;
      for (const name of paths.filter(name => path.posix.basename(name) === 'Cargo.toml')) {
        const parsed = TOML.parse(git(['show', revision + ':' + name]).toString());
        if (parsed.package?.name === row.name) { manifest = parsed.package; selectedManifest = name; break; }
      }
      if (!manifest) throw Error('Missing Git package manifest: ' + key);
      for (let directory = path.posix.dirname(selectedManifest);; directory = path.posix.dirname(directory)) {
        const name = directory === '.' ? 'Cargo.toml' : directory + '/Cargo.toml';
        if (paths.includes(name)) {
          const parsed = TOML.parse(git(['show', revision + ':' + name]).toString());
          if (parsed.workspace) { rootManifest = parsed; break; }
        }
        if (directory === '.') break;
      }
      if (manifest.license?.workspace) manifest.license = rootManifest.workspace.package.license;
      if (manifest.authors?.workspace) manifest.authors = rootManifest.workspace.package.authors;
      // Root plus package-local legal files; Git trees are read at the pinned
      // commit, never from mutable cached working files.
      for (const name of paths.filter(name => legalName(name) && !/(^|\/)(test|tests|example|examples)\//.test(name))) {
        licenseFiles.push(retain(git(['show', revision + ':' + name]), { kind: 'git', source: pin.source, path: name }));
      }
    } else {
      const local = workspace.packages.filter(pkg => pkg.name === row.name && pkg.version === row.version);
      if (local.length !== 1) throw Error('Missing local package metadata: ' + key);
      manifest = local[0];
      const relative = path.relative(root, manifest.manifest_path).replaceAll('\\', '/');
      if (relative.startsWith('../')) throw Error('Local package is outside reviewed source');
      const base = relative.startsWith('src/third_party/codex/') ? 'src/third_party/codex/' :
        relative.startsWith('src/third_party/munarium/') ? 'src/third_party/munarium/' : '';
      for (const name of ['LICENSE', 'NOTICE']) if (fs.existsSync(path.join(root, base + name)))
        licenseFiles.push(retain(fs.readFileSync(path.join(root, base + name)), { kind: 'source', path: base + name }));
      row.manifest = relative;
    }
    const selected = selectedLicense(manifest.license);
    const override = overrides.records.filter(record => record.packages.includes(key));
    if (override.length > 1) throw Error('Duplicate license override: ' + key);
    if (override[0]?.path) {
      const value = override[0];
      if (fileHash(path.join(root, value.path)) !== value.sha256 || !/^[a-f0-9]{40}$/.test(value.commit)) throw Error('Unpinned or changed upstream license: ' + key);
      if (vcs?.git?.sha1 && value.commit !== vcs.git.sha1) throw Error('License commit differs from crate VCS identity: ' + key);
      if (value.kind === 'published-declaration') {
        if (value.package_sha256 !== pin.checksum || value.license !== manifest.license || !value.limitation || !sourceArchive)
          throw Error('Unbound published license declaration');
        licenseFiles.push(retain(fs.readFileSync(path.join(root, value.path)), { kind: value.kind, source_path: value.path, limitation: value.limitation }));
      } else {
        if (!value.url?.includes('/' + value.commit + '/')) throw Error('Unpinned upstream license URL');
        licenseFiles.push(retain(fs.readFileSync(path.join(root, value.path)), { kind: 'pinned-upstream', url: value.url, source_path: value.path }));
      }
    }
    if (!licenseFiles.length && selected === 'Apache-2.0') {
      licenseFiles.push(retain(standardApache, { kind: 'standard-license', license: 'Apache-2.0',
        reason: 'Published crate grants Apache-2.0 but omits a standalone text; authors/provenance are retained in this inventory.' }));
    }
    if (!licenseFiles.length) missing.push(key + ': original license text unavailable');
    records.push({ ...row, source: pin.source || 'reviewed-local-source', checksum: pin.checksum || null,
      source_verified: true,
      role: runtime.has(key) ? 'normal-dependency' : 'build-dependency',
      license: manifest.license, selected_license: selected, authors: manifest.authors || [],
      repository: manifest.repository || null, vcs, licenses: licenseFiles,
      source_archive: sourceArchive });
  }
  return { inventory: { schema: 'vcp-native-components/1', target: TARGET, qualification_features: false,
    scope: 'Locked native normal/build graph, conservatively retained even if the linker removes code; build dependencies are not claimed as runtime payload.',
    workspace_lock_sha256: fileHash(path.join(root, lockPath)), graph_sha256: hash(tree), runtime_graph_sha256: hash(runtimeTree),
    status: missing.length ? 'incomplete' : 'complete', missing, components: records }, texts, sources };
}
function inspect(root) {
  const workspacePath = path.join(root, 'src/third_party/codex/codex-rs');
  const cargo = args => execFileSync('cargo', ['+1.95.0', ...args], { cwd: workspacePath, windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'], encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  const treeArgs = ['tree', '--locked', '--offline', '-p', 'vcp-cli', '--no-default-features', '--target', TARGET, '--prefix', 'none', '--format', '{p}'];
  const tree = cargo([...treeArgs, '--edges', 'normal,build']), runtimeTree = cargo([...treeArgs, '--edges', 'normal']);
  const workspace = JSON.parse(cargo(['metadata', '--locked', '--offline', '--no-deps', '--format-version', '1']));
  return { result: supplemental(root, collect(root, { tree, runtimeTree, workspace })), workspace };
}
function sourceReceipt(root) {
  const { result } = inspect(root);
  if (result.inventory.missing.length) throw Error('Incomplete release notices: ' + result.inventory.missing.join('; '));
  return { schema: 'vcp-release-dependencies/1', target: TARGET, status: 'verified',
    workspace_lock_sha256: result.inventory.workspace_lock_sha256,
    components: result.inventory.components.length,
    inventory_sha256: hash(JSON.stringify(result.inventory)) };
}
function supplemental(root, result) {
  const assets = [];
  const add = (relative, role) => {
    const bytes = fs.readFileSync(path.join(root, relative)), sha256 = hash(bytes);
    assets.push({ path: relative, bytes: bytes.length, sha256, role });
    if (role === 'license' || role === 'notice') result.texts.set('licenses/texts/' + sha256 + '.txt', bytes);
  };
  for (const [relative, role] of [
    ['src/third_party/codex/LICENSE', 'license'], ['src/third_party/codex/NOTICE', 'notice'],
    ['src/third_party/codex/third_party/wezterm/LICENSE', 'license'],
    ['src/third_party/munarium/LICENSE', 'license'], ['src/third_party/munarium/NOTICE', 'notice'],
    ['src/third_party/licenses/postgresql-16.15-COPYRIGHT', 'notice'],
    ['src/third_party/licenses/snowball-2.2.0-COPYING', 'license'],
  ]) add(relative, role);
  // codex-skills::SYSTEM_SKILLS_DIR includes these bytes in the native library.
  // Their embedding does not enable them as VCP runtime skills or tools.
  const embedded = 'src/third_party/codex/codex-rs/skills/src/assets/samples';
  for (const file of enumerate(path.join(root, embedded))) add(embedded + '/' + file.path, legalName(file.path) ? 'license' : 'embedded-system-skill');
  const builtin = 'src/skills/builtin';
  for (const file of enumerate(path.join(root, builtin))) add(builtin + '/' + file.path, 'shipped-builtin-skill');
  add('src/third_party/components/minilm-assets.json', 'unbundled-model-specification');
  result.inventory.assets = assets;
  result.inventory.exclusions = [
    'Cargo development dependencies and qualification binaries are excluded by the production graph.',
    'Codex Linux bubblewrap and native voice binaries are not included in the Windows payload.',
    'MiniLM weights, Python/Node runtimes, optional helper dependencies and browsers are user-provisioned, not bundled.',
    'Build dependencies appear for provenance; their external executables are not automatically distributed.'
  ];
  result.inventory.source_delivery = 'Original MPL-2.0 crates are supplied under licenses/sources, unchanged and under their original terms. The debugserver-types crate is also preserved because its upstream publishes a declaration without a separate license text.';
  return result;
}
function verifyCompiledGraph(log, inventory, workspace) {
  const rows = log.replace(/^\uFEFF/, '').split(/\r?\n/).filter(line => line.startsWith('{')).map(line => JSON.parse(line))
    .filter(row => row.reason === 'compiler-artifact');
  if (!rows.length) throw Error('Build log has no compiler package inventory');
  function matches(row, id) {
    const named = row.name + '@' + row.version;
    if (row.source?.startsWith('registry+')) return id === row.source + '#' + named;
    if (row.source?.startsWith('git+')) {
      const pin = /^(git\+[^#]+)#([a-f0-9]{40})$/.exec(row.source);
      if (!pin) return false;
      // Cargo's package ID retains the exact Git URL/query, but replaces the
      // lock's resolved commit fragment with the package version. It omits the
      // name only when it equals the URL's final path segment (not e.g. .git).
      // collect() already verifies that checkout against the full locked pin.
      if (id === pin[1] + '#' + named) return true;
      return new URL(pin[1].slice(4)).pathname.split('/').at(-1) === row.name && id === pin[1] + '#' + row.version;
    }
    return row.source === 'reviewed-local-source' && workspace.packages.some(local =>
      local.name === row.name && local.version === row.version && local.id === id);
  }
  const selected = new Set();
  for (const artifact of rows) {
    const candidates = inventory.components.filter(row => matches(row, artifact.package_id));
    if (candidates.length !== 1) throw Error('Compiled package absent or ambiguous in production dependency graph: ' + artifact.package_id);
    selected.add(candidates[0].name + '@' + candidates[0].version);
  }
  for (const row of inventory.components) row.compiler_observed = selected.has(row.name + '@' + row.version);
  inventory.compiler_packages = selected.size;
  // Cargo tree may include package nodes for build tools bundled for another
  // host. Keep their role explicit; never infer that their binary was shipped.
}
function stage(root, packageRoot, receiptFile) {
  const { result, workspace } = inspect(root);
  const receipt = json(receiptFile);
  if (receipt.dependency_source?.inventory_sha256 !== hash(JSON.stringify(result.inventory))) throw Error('Dependency source/license inventory differs from verified build inputs');
  const lock = receipt.inputs?.find(row => row.path === 'src/third_party/codex/codex-rs/Cargo.lock');
  if (lock?.sha256 !== result.inventory.workspace_lock_sha256) throw Error('Notices dependency lock differs from build source');
  verifyCompiledGraph(fs.readFileSync(path.join(path.dirname(receiptFile), 'build.log'), 'utf8'), result.inventory, workspace);
  if (result.inventory.missing.length) throw Error('Incomplete release notices: ' + result.inventory.missing.join('; '));
  result.inventory.native_executable_sha256 = receipt.executable_sha256;
  result.inventory.reviewed_commit = receipt.source_commit;
  result.inventory.source_content_sha256 = receipt.source_content_sha256;
  result.inventory.build_receipt_sha256 = fileHash(receiptFile);
  const output = new Map([...result.texts, ...result.sources]);
  output.set('licenses/README.txt', Buffer.from('VCP third-party license and source bundle\n\ncomponent-inventory.json maps each package and source asset to its license texts.\nText filenames are SHA-256 identities; matching texts are shared without dropping attribution.\nOriginal MPL-2.0 source crates are in licenses/sources and remain under their original terms.\nThe inventory preserves published license declarations and their exact provenance limitations.\nOptional helper/model dependencies are not bundled; see PREREQUISITES.md.\n'));
  output.set('PREREQUISITES.md', fs.readFileSync(path.join(root, 'release/package-prerequisites.md')));
  result.inventory.files = [...output].map(([name, bytes]) => ({ path: name, bytes: bytes.length, sha256: hash(bytes) }))
    .sort((a, b) => a.path.localeCompare(b.path));
  output.set('component-inventory.json', Buffer.from(JSON.stringify(result.inventory, null, 2) + '\n'));
  for (const [relative, bytes] of output) {
    const file = path.join(packageRoot, relative); fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, bytes, { flag: 'wx' });
  }
  const identity = hash(output.get('component-inventory.json'));
  verifyStaged(packageRoot, identity, lock.sha256, receipt.executable_sha256);
  return { schema: 'vcp-notice-bundle/1', inventory: 'component-inventory.json', inventory_sha256: identity,
    components: result.inventory.components.length, license_texts: result.texts.size, source_archives: result.sources.size,
    status: 'complete-with-recorded-provenance-limitations' };
}
function verifyStaged(packageRoot, expected, expectedLock, executableHash) {
  const file = path.join(packageRoot, 'component-inventory.json');
  if (fileHash(file) !== expected) throw Error('Component inventory hash mismatch');
  const inventory = json(file);
  if (inventory.schema !== 'vcp-native-components/1' || inventory.status !== 'complete' || inventory.missing?.length ||
      inventory.workspace_lock_sha256 !== expectedLock || inventory.native_executable_sha256 !== executableHash) throw Error('Unbound or incomplete notice inventory');
  const actual = enumerate(packageRoot).filter(row => row.path.startsWith('licenses/') || row.path === 'PREREQUISITES.md');
  if (JSON.stringify(actual) !== JSON.stringify(inventory.files)) throw Error('License/source payload differs from inventory');
  return inventory;
}
if (require.main === module) {
  try {
    const [command, root, packageRoot, receiptFile, ...extra] = process.argv.slice(2);
    if (command === 'verify-source' && root && !packageRoot) console.log(JSON.stringify(sourceReceipt(path.resolve(root))));
    else if (command === 'stage' && root && packageRoot && receiptFile && !extra.length) console.log(JSON.stringify(stage(path.resolve(root), path.resolve(packageRoot), path.resolve(receiptFile))));
    else throw Error('Use notices.cjs verify-source <repository> or stage <repository> <new-package-directory> <build-receipt>');
  } catch (error) { console.error('Release notices failed: ' + error.message); process.exitCode = 1; }
}
module.exports = { parseTree, selectedLicense, collect, legalName, supplemental, verifyCompiledGraph, stage, verifyStaged, sourceReceipt };
