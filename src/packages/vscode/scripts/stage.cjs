// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');

const packageRoot = path.resolve(__dirname, '..');
const repository = path.resolve(packageRoot, '../../..');
const artifacts = path.join(repository, 'artifacts');
const output = path.resolve(process.argv[2] || path.join(artifacts, 'p4-vscode-extension'));
const relative = path.relative(artifacts, output);
if (!relative || relative.startsWith('..') || path.isAbsolute(relative)) throw new Error('Stage output must be a named directory within repository artifacts');
const comparable = value => process.platform === 'win32' ? path.normalize(value).toLowerCase() : path.normalize(value);
if (comparable(fs.realpathSync(repository)) !== comparable(repository)) throw new Error('Repository directory is redirected');
if (!fs.existsSync(artifacts)) {
  try { fs.lstatSync(artifacts); throw new Error('Artifacts directory is redirected'); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  // Create one child of the verified repository, never recursive parents.
  fs.mkdirSync(artifacts);
}
if (comparable(fs.realpathSync(artifacts)) !== comparable(artifacts)) throw new Error('Artifacts directory is redirected');
const canonicalArtifacts = fs.realpathSync(artifacts);
// Verify every existing ancestor, including the destination itself, before any
// replacement. A marker reached through a junction must never authorize deletion.
let checked = artifacts;
for (const component of relative.split(path.sep)) {
  checked = path.join(checked, component);
  if (!fs.existsSync(checked)) {
    try { fs.lstatSync(checked); throw new Error('Stage destination is redirected'); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    continue;
  }
  if (fs.lstatSync(checked).isSymbolicLink()) throw new Error('Stage destination is redirected');
  const actual = fs.realpathSync(checked);
  const contained = path.relative(canonicalArtifacts, actual);
  if (!contained || contained.startsWith('..') || path.isAbsolute(contained) || comparable(actual) !== comparable(checked)) throw new Error('Stage destination escapes canonical artifacts');
}
const marker = path.join(output, '.vcp-stage.json');
if (fs.existsSync(output)) {
  if (!fs.existsSync(marker) || JSON.parse(fs.readFileSync(marker, 'utf8')).format !== 'vcp-extension-stage/1') throw new Error('Refusing to replace an unrecognized directory');
  fs.rmSync(output, { recursive: true });
}
fs.mkdirSync(output, { recursive: true });
fs.writeFileSync(marker, JSON.stringify({ format: 'vcp-extension-stage/1' }) + '\n');
const copy = (source, destination) => fs.cpSync(source, destination, { recursive: true, dereference: true });
for (const name of ['dist', 'media', 'README.md']) copy(path.join(packageRoot, name), path.join(output, name));
const manifest = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
delete manifest.scripts; delete manifest.devDependencies;
manifest.dependencies = { '@vcp/sdk': '0.1.0', '@vcp/protocol': '0.1.0' };
fs.writeFileSync(path.join(output, 'package.json'), JSON.stringify(manifest, null, 2) + '\n');
for (const [name, files] of [['sdk-ts', ['dist', 'README.md']], ['protocol-ts', ['schema.json', 'index.ts', 'README.md']]]) {
  const source = path.resolve(packageRoot, '..', name);
  const dependency = name === 'sdk-ts' ? 'sdk' : 'protocol';
  const destination = path.join(output, 'node_modules', '@vcp', dependency);
  fs.mkdirSync(destination, { recursive: true });
  for (const file of files) if (fs.existsSync(path.join(source, file))) copy(path.join(source, file), path.join(destination, file));
  const meta = JSON.parse(fs.readFileSync(path.join(source, 'package.json'), 'utf8'));
  delete meta.devDependencies; delete meta.scripts;
  if (name === 'sdk-ts') meta.dependencies = { '@vcp/protocol': '0.1.0' };
  fs.writeFileSync(path.join(destination, 'package.json'), JSON.stringify(meta, null, 2) + '\n');
  for (const notice of ['LICENSE', 'NOTICE']) copy(path.join(repository, notice), path.join(destination, notice));
}
for (const notice of ['LICENSE', 'NOTICE']) copy(path.join(repository, notice), path.join(output, notice));
console.log(output);
