const { windowFor } = require('../domain/window.cjs'); // SPDX-License-Identifier: Apache-2.0
exports.pageItems = (items, options = {}) => {
  if (!Array.isArray(items) || options === null || typeof options !== 'object' || Array.isArray(options)) throw new TypeError('invalid pagination');
  const window = windowFor(items.length, options.offset, options.limit);
  return { items: items.slice(window.start, window.end), total: items.length, offset: window.start, nextOffset: window.nextOffset };
};
