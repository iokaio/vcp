exports.windowFor = (total, offset = 0, limit = 25) => { // SPDX-License-Identifier: Apache-2.0
  if (!Number.isSafeInteger(total) || total < 0 || !Number.isSafeInteger(offset) || offset < 0 || offset > total || !Number.isInteger(limit) || limit < 1 || limit > 100) throw new TypeError('invalid pagination');
  const end = offset + Math.min(limit, total - offset);
  return { start: offset, end, nextOffset: end < total ? end : null };
};
