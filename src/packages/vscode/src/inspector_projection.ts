// SPDX-License-Identifier: Apache-2.0
import type { ArtifactRange, ArtifactRead, ResultValue } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import type { InspectorSection } from './inspector_view_model.js';
/** Only public validated DTOs enter this projection. Cursor tokens never enter DOM. */
function display(value: unknown): string {
  return JSON.stringify(value, (key, item: unknown) => ['cursor', 'next_cursor', 'preview_id', 'preview', 'reference'].includes(key) ? undefined : item, 2);
}
const section = (title: string, value: unknown): InspectorSection => ({title,fields:[],text:display(value)});
export function projectInspector(page: ResultValue): InspectorSection[] {
  const value = page.value;
  if (page.kind === 'usage') { const v=page.value; return [{title:'Root ledger — exact micro-units; liabilities are not settled charges',fields:[{label:'Root task',value:v.root},{label:'Currency',value:v.currency},{label:'Cap (micros)',value:v.cap_micros},{label:'Settled (micros)',value:v.settled_micros},{label:'Reserved (micros)',value:v.reserved_micros},{label:'Unresolved (micros)',value:v.unresolved_micros},{label:'Overrun',value:String(v.overrun)}]}]; }
  if (page.kind === 'policy') { const {persisted,effective,rows,...facts}=page.value; return [section('Current task policy observations — not dispatch permission',facts),section('Persisted workspace policy',persisted),section('Task-effective policy / unavailable host facts',effective),section('Scoped historical denials or grants; provenance and current matches',rows)]; }
  if (page.kind === 'routing_preview') { const {prior,persisted,effective,selected,clamped,...pins}=page.value; return [section('Exact optimizer review pins and expiry',pins),section('Exact selected edits',selected),section('Prior policy',prior),section('Resulting persisted policy',persisted),section('Resulting effective policy',effective),section('Host constraints that clamp the proposal',clamped)]; }
  if (page.kind === 'retention_preview') { const {targets,...facts}=page.value; return [section('Exact pruning preview — counts, digest, revisions and expiry',facts),section('Selected and protected targets on this page',targets)]; }
  if (page.kind === 'backup_status') return [section('Native encrypted publisher availability — no key material',page.value)];
  if (page.kind === 'backup_job') { const v=page.value; return [{title:'Publication facts — acceptance, local publication and cloud transfer differ',fields:[{label:'Operation',value:v.operation},{label:'Phase',value:v.phase},{label:'Cancellation requested',value:String(v.cancel_requested)},{label:'Local publication',value:v.local_publication},{label:'Independent checkpoint',value:v.checkpoint},{label:'Cleanup',value:v.cleanup},{label:'Source pins',value:v.source_pins},{label:'Cloud transfer',value:v.cloud_transfer},{label:'Restore verification',value:v.restore_verification}]},section('Historical source revisions and current job revision',v)]; }
  if (page.kind === 'artifact') throw new Error('artifact requires range verification');
  const { rows, versions, findings, targets, ...metadata } = value as unknown as Record<string, unknown>;
  const items = rows ?? versions ?? findings ?? targets;
  return [{ title: page.kind.replaceAll('_', ' '), fields: [], text: display(metadata), ...(Array.isArray(items) ? { rows: items.map((item, i) => ({ title: `Item ${i + 1}`, fields: [{ label: 'Details', value: display(item) }] })) } : {}) }];
}
export function decodeInspectorArtifact(value: ArtifactRange, request: ArtifactRead, hash?: string): { text: string; next: string | undefined; hash: string } {
  if (value.artifact !== request.artifact || value.offset !== request.offset || !/^[0-9a-f]{64}$/.test(value.sha256) || (hash !== undefined && value.sha256 !== hash)) throw new Error('artifact identity');
  const start = BigInt(value.offset), total = BigInt(value.total_bytes);
  let bytes: Buffer;
  if (value.encoding === 'base64') {
    if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value.content)) throw new Error('artifact encoding');
    bytes = Buffer.from(value.content, 'base64');
    if (bytes.toString('base64') !== value.content) throw new Error('artifact encoding');
  } else bytes = Buffer.from(value.content, 'utf8');
  const end = start + BigInt(bytes.length);
  if (bytes.length > request.length || end > total || start > total || value.complete !== (end === total) || (!value.complete && bytes.length === 0)) throw new Error('artifact range');
  let text: string;
  try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
  catch { text = `This byte range contains binary data or a split UTF-8 character. Exact range bytes (base64):\n${bytes.toString('base64')}`; }
  return { text: `Artifact ${value.artifact}\nBytes ${start}–${end} of ${total}\nSHA-256 ${value.sha256}\n\n${text}`, next: value.complete ? undefined : end.toString(), hash: value.sha256 };
}
