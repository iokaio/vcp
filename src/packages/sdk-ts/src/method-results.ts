// SPDX-License-Identifier: Apache-2.0
import type { Call, ResultValue } from '@vcp/protocol';

export type Method = Call['method'];
export type Params<M extends Method> = Extract<Call, { method: M }>['params'];

/** Audited against engine, lifecycle and configured execution dispatchers. */
export const RESULT_KINDS = {
  'controller/read': ['controller'],
  'controller/acquire': ['acceptance'],
  'controller/release': ['acceptance'],
  'controller/recover': ['acceptance'],
  'workspace/open': ['workspace'],
  'workspace/setTrust': ['acceptance'],
  'session/create': ['acceptance'],
  'session/read': ['session'],
  'session/snapshot': ['snapshot', 'gap'],
  'session/list': ['sessions'],
  'session/resume': ['acceptance'],
  'session/fork': ['acceptance'],
  'task/read': ['task'],
  'task/presentation': ['presentation'],
  'task/cancel': ['acceptance'],
  'turn/start': ['acceptance'],
  'turn/steer': ['acceptance'],
  'turn/pause': ['acceptance'],
  'turn/cancel': ['acceptance'],
  'approval/respond': ['acceptance'],
  'events/subscribe': ['events', 'gap'],
  'events/next': ['events', 'gap'],
  'events/unsubscribe': ['unsubscribed'],
  'artifact/read': ['artifact'],
  'diff/read': ['artifact'],
  'context/inspect': ['evidence'],
  'routing/explain': ['evidence'],
  'usage/read': ['usage'],
  'history/query': ['history'],
  'memory/history': ['memory_history'],
  'memory/query': ['memory_query'],
  'memory/inspect': ['memory'],
  'memory/propose': ['memory_reviewed'],
  'memory/resolve': ['memory_reviewed'],
  'memory/review': ['memory_review'],
  'memory/forget': ['forgotten'],
  'memory/forgetPreview': ['retention_preview'],
  'memory/forgetPreviewRead': ['retention_preview'],
  'memory/forgetRead': ['retention'],
  'editor/context': ['editor_context'],
  'editor/prepare': ['editor_change'],
  'editor/changeRead': ['editor_change'],
  'editor/dispatch': ['editor_dispatch'],
  'editor/changeResult': ['editor_change'],
  'session/export': ['export'],
  'command/read': ['acceptance'],
} as const satisfies Record<Method, readonly ResultValue['kind'][]>;

export type Reply<M extends Method> = Extract<ResultValue, { kind: typeof RESULT_KINDS[M][number] }>;

/** Extra mandatory profiles, in addition to the advertised method itself. */
export const REQUIRED_PROFILES: Readonly<Partial<Record<Method, readonly string[]>>> = Object.freeze({
  'history/query': Object.freeze(['history/query/1']),
  'memory/history': Object.freeze(['memory/history/1']),
  'editor/context': Object.freeze(['editor/prepared-edits/1']),
  'editor/prepare': Object.freeze(['editor/prepared-edits/1']),
  'editor/changeRead': Object.freeze(['editor/prepared-edits/1']),
  'editor/dispatch': Object.freeze(['editor/prepared-edits/1']),
  'editor/changeResult': Object.freeze(['editor/prepared-edits/1']),
  'memory/inspect': Object.freeze(['memory/inspection-state/1']),
  'memory/query': Object.freeze(['memory/query-sources/1']),
  'memory/propose': Object.freeze(['memory/governance/1']),
  'memory/resolve': Object.freeze(['memory/governance/1']),
  'memory/review': Object.freeze(['memory/governance/1']),
  'memory/forget': Object.freeze(['memory/retention/1']),
  'memory/forgetPreview': Object.freeze(['memory/retention/1']),
  'memory/forgetPreviewRead': Object.freeze(['memory/retention/1']),
  'memory/forgetRead': Object.freeze(['memory/retention/1']),
  'session/export': Object.freeze(['session/export-local/1']),
});

for (const kinds of Object.values(RESULT_KINDS)) Object.freeze(kinds);
Object.freeze(RESULT_KINDS);
