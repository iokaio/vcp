// SPDX-License-Identifier: Apache-2.0
'use strict';
const { subtotalCents } = require('./cents.cjs');
function quoteCart(items) {
  const subtotal = subtotalCents(items);
  return { subtotalCents: subtotal, discountCents: 0, totalCents: subtotal };
}
module.exports = { quoteCart };
