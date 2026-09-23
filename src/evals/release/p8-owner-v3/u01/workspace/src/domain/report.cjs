const { sum } = require('../shared/numbers.cjs'); // SPDX-License-Identifier: Apache-2.0
const { readyLabel } = require('../transport/status.cjs');
exports.summarizeReadings = readings => ({ total: sum(readings), label: readyLabel });
