// SPDX-License-Identifier: Apache-2.0
'use strict';
// Minimum original acceptance identities. Additional required gates must also
// pass. contracts.test.ps1 checks these against the trusted source definitions.
const final = {
  A: ['npm-ci','typecheck','node-test','vitest','build','testids','protected-files',
    'api.start','api.health','api.create','api.validate-title','api.validate-status','api.validate-date','api.invalid-json','api.not-found','api.list','api.patch','api.filter','api.restart','api.persistence','api.delete',
    'labels.start','labels.legacy-default','labels.validation','labels.sort-priority','labels.sort-due','labels.sort-default','labels.filter','labels.sort-invalid',
    'prod.start','prod.index','prod.spa-fallback','prod.asset','prod.api-404','prod.stats'],
  B: ['build','dotnet-test','migrations','protected-files','publish','migration-script','app.start',
    'api.suppliers','api.paging','api.page-size-limit','api.search','api.create','api.duplicate-sku','api.validation','api.unknown-supplier','api.movements','api.low-stock','api.delete',
    'ui.index','ui.create','ui.validation','ui.details','ui.form-zero-values','ui.form-invalid-sku','ui.form-long-name','ui.form-unknown-supplier','ui.low-stock','ui.low-stock-parity','ui.antiforgery','etag.flow'],
};
const stages={A:['T1-api','T2-ui','T3-labels','T4-regressions','T5-production','T5-resume','T6-review'],B:['T1-data','T2-api','T3-razor','T4-regressions','T5-concurrency','T5-resume','T6-review']};
function required(kind) {
  return [...final[kind].map(id=>['FINAL',id]),...stages[kind].flatMap(stage=>[[''+stage,'vcp-exit'],[stage,'jsonl']]),
    [stages[kind][4],'explicit-pause'],['T5-resume','resume-same-task'],['T6-review','immutable']];
}
module.exports={final,stages,required};
