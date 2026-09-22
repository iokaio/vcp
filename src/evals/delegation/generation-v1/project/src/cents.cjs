// SPDX-License-Identifier: Apache-2.0
'use strict';
function subtotalCents(items) {
  return items.reduce((sum, item) => sum + item.unitCents * item.quantity, 0);
}
module.exports = { subtotalCents };
