const { summary } = require('../app/summary.cjs'); // SPDX-License-Identifier: Apache-2.0
exports.getSummary = () => ({ status: 200, body: summary() });
