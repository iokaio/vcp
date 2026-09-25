// Original synthetic transport. No network, filesystem or credentials.
'use strict';
exports.makeTransport = function (responses) {
  if (!Array.isArray(responses) || responses.length > 16 || Buffer.byteLength(JSON.stringify(responses)) > 65536) throw Error('fixture response bound');
  const queue = structuredClone(responses), calls = [];
  return { calls, async send(request) {
    if (calls.length >= 16 || Buffer.byteLength(JSON.stringify(request)) > 4096) throw Error('fixture request bound');
    calls.push(structuredClone(request));
    if (!queue.length) throw Error('no synthetic response');
    return queue.shift();
  } };
};
