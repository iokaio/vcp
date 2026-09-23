const { windowFor } = require('../domain/window.cjs'); // SPDX-License-Identifier: Apache-2.0
exports.pageItems = items => {
  if (!Array.isArray(items)) throw new TypeError('invalid pagination');
  const window = windowFor(items.length);
  return { items: items.slice(window.start, window.end), total: items.length, offset: window.start, nextOffset: window.nextOffset };
};
