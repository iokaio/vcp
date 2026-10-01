// SPDX-License-Identifier: Apache-2.0
'use strict';
const path = require('node:path');
const { enumerate, portable, digest } = require('../package-inventory.cjs');

// Pinned Inno 6.7.3 uses ordinary MoveFileW paths. GenerateUniqueName stages
// is-<10 base36 digits>.tmp in each destination directory before renaming.
const FILE_LIMIT = 259, DIRECTORY_LIMIT = 248, TEMP_NAME_LENGTH = 17;
const GENERATED_FILES = ['unins000.exe', 'unins000.dat', 'unins000.msg', '.vcp-setup-owned.ini'];
function derive(files) {
  if (!Array.isArray(files) || !files.length || files.length > 4096) throw Error('Bounded setup file inventory required');
  const names = new Set(), directories = new Set(['']);
  let maxFile = 0;
  for (const file of [...files, ...GENERATED_FILES]) {
    portable(file);
    const folded = file.toLowerCase();
    if (names.has(folded)) throw Error('Duplicate setup destination');
    names.add(folded);
    maxFile = Math.max(maxFile, file.length); // JavaScript and Pascal count UTF-16 units.
    let parent = path.posix.dirname(file);
    while (parent !== '.') { directories.add(parent); parent = path.posix.dirname(parent); }
  }
  const maxDirectory = Math.max(...[...directories].map(name => name.length));
  const maxTemporary = Math.max(...[...directories].map(name => (name ? name.length + 1 : 0) + TEMP_NAME_LENGTH));
  const maximum = Math.min(240, FILE_LIMIT - 1 - Math.max(maxFile, maxTemporary), DIRECTORY_LIMIT - 1 - maxDirectory);
  if (maximum < 4) throw Error('Setup destinations leave no supported application root');
  return { schema: 'vcp-setup-path-limit/1', inno_version: '6.7.3', max_app_root_utf16: maximum,
    file_path_limit_utf16: FILE_LIMIT, directory_path_limit_utf16: DIRECTORY_LIMIT,
    temporary_name_utf16: TEMP_NAME_LENGTH, max_file_relative_utf16: maxFile,
    max_temporary_relative_utf16: maxTemporary, max_directory_relative_utf16: maxDirectory };
}
function inspect(root) {
  const files = enumerate(root);
  return { ...derive(files.map(file => file.path)), files_sha256: digest(JSON.stringify(files)), files };
}
if (require.main === module) {
  if (process.argv.length !== 3) throw Error('Expected exact staged setup-files directory');
  process.stdout.write(JSON.stringify(inspect(process.argv[2])) + '\n');
}
module.exports = { derive, inspect };
