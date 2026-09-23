const { readAll } = require('../storage/readings.cjs'); // SPDX-License-Identifier: Apache-2.0
const { summarizeReadings } = require('../domain/report.cjs');
exports.summary = () => summarizeReadings(readAll());
