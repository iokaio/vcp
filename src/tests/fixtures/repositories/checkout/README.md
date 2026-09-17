# Synthetic checkout

`receipt.cjs` composes receipt totals using the pure fee function in
`shipping.cjs`. Shipping costs 5 units below a subtotal of 50 and is free at or
above 50. A future workspace-label function must trim surrounding whitespace,
preserve case and Unicode, and reject empty or non-string inputs. Keep domain
functions independent of I/O. All data is synthetic; there is no network or
credential configuration.
