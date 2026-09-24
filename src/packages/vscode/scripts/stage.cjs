// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');

const packageRoot = path.resolve(__dirname, '..');
const repository = path.resolve(packageRoot, '../../..');
const artifacts = path.join(repository, 'artifacts');
const output = require('./output.cjs').prepareOutput(process.argv[2] || path.join(artifacts, 'p4-vscode-extension'));
const copy = (source, destination) => {
  if (!fs.lstatSync(source).isFile() || fs.realpathSync(source) !== path.resolve(source)) throw new Error('Stage input must be an ordinary unredirected file');
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
};
// Derive compiled runtime files from the source module inventory, never copy an
// arbitrary build directory (which may contain stale code, maps or fixtures).
const runtime = (source, destination) => {
  for (const entry of fs.readdirSync(path.join(source, 'src'), { withFileTypes: true })) {
    if (!entry.isFile() || !/^[a-z][a-z0-9_-]*\.ts$/.test(entry.name)) throw new Error('Unexpected runtime source inventory');
    const name = entry.name.replace(/\.ts$/, '.js');
    copy(path.join(source, 'dist', name), path.join(destination, 'dist', name));
  }
};
runtime(packageRoot, output);
for (const name of ['connection.css', 'connection.js', 'inspectors.js', 'tasks.js', 'vcp.svg']) copy(path.join(packageRoot, 'media', name), path.join(output, 'media', name));
for (const name of ['README.md', 'COMPATIBILITY.md']) copy(path.join(packageRoot, name), path.join(output, name));
const manifest = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
delete manifest.scripts; delete manifest.devDependencies;
manifest.dependencies = { '@vcp/sdk': '0.1.0', '@vcp/protocol': '0.1.0' };
fs.writeFileSync(path.join(output, 'package.json'), JSON.stringify(manifest, null, 2) + '\n');
for (const [name, files] of [['sdk-ts', ['dist', 'README.md']], ['protocol-ts', ['schema.json', 'index.ts', 'README.md']]]) {
  const source = path.resolve(packageRoot, '..', name);
  const dependency = name === 'sdk-ts' ? 'sdk' : 'protocol';
  const destination = path.join(output, 'node_modules', '@vcp', dependency);
  fs.mkdirSync(destination, { recursive: true });
  if (name === 'sdk-ts') runtime(source, destination);
  for (const file of files.filter(file => file !== 'dist')) if (fs.existsSync(path.join(source, file))) copy(path.join(source, file), path.join(destination, file));
  const meta = JSON.parse(fs.readFileSync(path.join(source, 'package.json'), 'utf8'));
  delete meta.devDependencies; delete meta.scripts;
  if (name === 'sdk-ts') meta.dependencies = { '@vcp/protocol': '0.1.0' };
  fs.writeFileSync(path.join(destination, 'package.json'), JSON.stringify(meta, null, 2) + '\n');
  for (const notice of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) copy(path.join(repository, notice), path.join(destination, notice));
}
for (const notice of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) copy(path.join(repository, notice), path.join(output, notice));
console.log(output);
