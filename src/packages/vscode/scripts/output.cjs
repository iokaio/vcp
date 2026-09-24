// SPDX-License-Identifier: Apache-2.0
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const repository = path.resolve(__dirname, '../../../..');
const artifacts = path.join(repository, 'artifacts');
exports.prepareOutput = function(requested, format = 'vcp-extension-stage/1') {
  const output = path.resolve(requested);
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
    if (!fs.existsSync(marker) || JSON.parse(fs.readFileSync(marker, 'utf8')).format !== format) throw new Error('Refusing to replace an unrecognized directory');
    fs.rmSync(output, { recursive: true });
  }
  fs.mkdirSync(output, { recursive: true });
  fs.writeFileSync(marker, JSON.stringify({ format }) + '\n');
  return output;
};
