// SPDX-License-Identifier: Apache-2.0
// Harness-only truth set; never include this file in model-visible fixture roots.
'use strict';
function gradeAnalysis(report) {
  const failures = [];
  if (!report.modules?.includes('receipt.cjs') || !report.modules?.includes('shipping.cjs')) failures.push('module-coverage');
  if (!report.dependencies?.some(edge => edge.from === 'receipt.cjs' && edge.to === 'shipping.cjs')) failures.push('dependency-direction');
  if (report.domainDoesIO !== false) failures.push('domain-boundary');
  return failures;
}
function gradeReview(findings) {
  const relevant = finding => finding.path === 'shipping.cjs' && finding.trigger === 50 && finding.expectedFee === 0 && finding.actualFee === 5;
  const truePositives = findings.some(relevant) ? 1 : 0;
  const falsePositives = findings.filter(finding => !relevant(finding)).length;
  const duplicates = findings.filter(relevant).length - truePositives;
  return { truePositives, falsePositives, duplicates, recall: truePositives,
    precision: findings.length ? truePositives / findings.length : 0 };
}
function gradeLabel(normalize) {
  const failures = [];
  for (const [input, expected] of [['  Work  ', 'Work'], [' Ångström 🧪 ', 'Ångström 🧪'], ['a  b', 'a  b']]) {
    try { if (normalize(input) !== expected) failures.push('valid-label'); }
    catch { failures.push('valid-label'); }
  }
  for (const input of ['', ' \t\r\n ', null, 3, {}, []]) {
    let threw = false;
    try { normalize(input); } catch { threw = true; }
    if (!threw) failures.push('invalid-label');
  }
  return failures;
}
module.exports = { gradeAnalysis, gradeReview, gradeLabel };
