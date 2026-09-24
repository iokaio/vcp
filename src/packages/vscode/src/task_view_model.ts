// SPDX-License-Identifier: Apache-2.0
import type { TaskPresentation, UsageView } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import type { TaskRow } from './task_projection.js';

export type TaskPanelMessage = { action: 'ready' | 'previous' | 'next' | 'refresh' } | { action: 'select' | 'task' | 'evidence' | 'history'; id: string };
export interface TaskPanelState {
  readonly phase: string; readonly message: string; readonly owner: string;
  readonly total: number; readonly offset: number; readonly hasMore: boolean;
  readonly stateCounts?: Readonly<Record<string, number>>;
  readonly rows: readonly (TaskRow & { readonly actionId: string })[];
  readonly detail?: TaskPresentation;
  readonly usage?: UsageView;
  readonly actions: readonly { id: string; label: string; input?: string; disabledReason?: string }[];
  readonly evidence?: readonly { id: string; label: string }[];
  readonly historyId?: string;
  readonly artifact?: string;
  readonly commands: readonly { commandId: string; operation: string; phase: string }[];
}
/** Web content selects host-registered opaque IDs, never engine IDs or payloads. */
export function parseTaskPanelMessage(value: unknown): TaskPanelMessage | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.getPrototypeOf(value) !== Object.prototype || Object.getOwnPropertySymbols(value).length) return undefined;
  const names = Object.getOwnPropertyNames(value).sort();
  if (names.join(',') !== 'action' && names.join(',') !== 'action,id') return undefined;
  const action = Object.getOwnPropertyDescriptor(value, 'action');
  if (!action?.enumerable || !('value' in action)) return undefined;
  if (['ready', 'previous', 'next', 'refresh'].includes(action.value) && names.length === 1) return { action: action.value };
  const id = Object.getOwnPropertyDescriptor(value, 'id');
  if (names.length === 2 && ['select', 'task', 'evidence', 'history'].includes(action.value) && id?.enumerable && 'value' in id && typeof id.value === 'string' && /^[a-f0-9-]{36}$/.test(id.value)) return { action: action.value, id: id.value };
  return undefined;
}
function escape(value: string): string { return value.replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!); }
export function taskPanelHtml(source: string, style: string, script: string, nonce: string): string {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${escape(source)}; script-src 'nonce-${escape(nonce)}';"><link rel="stylesheet" href="${escape(style)}"><title>VCP Tasks</title></head><body>
  <header><h1>Tasks and children</h1><span id="task-phase" class="badge">disconnected</span></header>
  <p id="task-message" role="status" aria-live="polite"></p><p id="task-owner"></p>
  <p id="task-count"></p><div id="task-rows"></div>
  <div class="actions"><button id="task-previous" type="button">Previous tasks</button><button id="task-next" type="button">Next tasks</button><button id="task-refresh" type="button">Refresh</button></div>
  <h2>Selected task</h2><dl id="task-detail"></dl><h2>Questions</h2><div id="task-questions"></div>
  <div id="task-actions" class="actions"></div><h2>Tool and evidence history</h2><div id="task-history"></div><div id="task-evidence" class="actions"></div>
  <pre id="task-artifact"></pre><h2>Submitted commands</h2><ul id="task-commands"></ul><p class="note">Acceptance is not completion. Unknown outcomes remain unknown until the engine supplies evidence. Answering a question does not resume paused work.</p>
  <script nonce="${escape(nonce)}" src="${escape(script)}"></script></body></html>`;
}
