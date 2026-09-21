// SPDX-License-Identifier: Apache-2.0
'use strict';
function quoteCart(items) {
  const subtotalCents = items.reduce((sum, item) => sum + item.unitCents * item.quantity, 0);
  return { subtotalCents, discountCents: 0, totalCents: subtotalCents };
}
module.exports = { quoteCart };
