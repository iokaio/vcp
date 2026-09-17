// SPDX-License-Identifier: Apache-2.0
'use strict';
const { shippingFee } = require('./shipping.cjs');
exports.receiptTotal = subtotal => subtotal + shippingFee(subtotal);
