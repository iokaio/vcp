exports.windowFor = total => { // SPDX-License-Identifier: Apache-2.0
  if (!Number.isSafeInteger(total) || total < 0) throw new TypeError('invalid pagination');
  return { start: 0, end: total, nextOffset: null };
};
