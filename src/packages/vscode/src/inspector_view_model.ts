// SPDX-License-Identifier: Apache-2.0
export const INSPECTOR_TABS = ['history', 'memory', 'evidence', 'cost', 'policy', 'routing', 'optimizer', 'pruning', 'publisher'] as const;
export type InspectorTab = typeof INSPECTOR_TABS[number];
export type InspectorPhase = 'disconnected' | 'loading' | 'current' | 'stale' | 'unavailable' | 'unsupported' | 'partial';
export interface InspectorAction { readonly id: string; readonly label: string; readonly disabledReason?: string }
export interface InspectorField { readonly label: string; readonly value: string }
export interface InspectorRow { readonly title: string; readonly fields: readonly InspectorField[]; readonly actions?: readonly InspectorAction[] }
export interface InspectorSection { readonly title: string; readonly fields: readonly InspectorField[]; readonly text?: string; readonly rows?: readonly InspectorRow[] }
export interface InspectorState {
  readonly phase: InspectorPhase; readonly tab: InspectorTab; readonly message: string;
  readonly scopeLabel: string; readonly taskLabel: string;
  readonly sections: readonly InspectorSection[]; readonly actions: readonly InspectorAction[];
  readonly commands: readonly { readonly commandId: string; readonly method: string; readonly phase: string }[];
}
export type InspectorMessage = { action: 'ready' | 'refresh' } | { action: 'tab'; tab: InspectorTab } | { action: 'invoke'; id: string };
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
export function parseInspectorMessage(value: unknown): InspectorMessage | undefined {
  try {
    if (!value || typeof value !== 'object' || Array.isArray(value) || Object.getPrototypeOf(value) !== Object.prototype || Object.getOwnPropertySymbols(value).length) return undefined;
    const names = Object.getOwnPropertyNames(value).sort().join(',');
    const field = (name: string): unknown => { const descriptor = Object.getOwnPropertyDescriptor(value, name); return descriptor?.enumerable && 'value' in descriptor ? descriptor.value : undefined; };
    const action = field('action');
    if (names === 'action' && (action === 'ready' || action === 'refresh')) return { action };
    const tab = field('tab');
    if (names === 'action,tab' && action === 'tab' && typeof tab === 'string' && INSPECTOR_TABS.includes(tab as InspectorTab)) return { action, tab: tab as InspectorTab };
    const id = field('id');
    if (names === 'action,id' && action === 'invoke' && typeof id === 'string' && UUID.test(id)) return { action, id };
    return undefined;
  } catch { return undefined; }
}
export function emptyInspectorState(tab: InspectorTab = 'history', phase: InspectorPhase = 'disconnected', message = 'Connect to inspect retained workspace evidence.'): InspectorState {
  return { tab, phase, message, scopeLabel: '', taskLabel: '', sections: [], actions: [], commands: [] };
}
/** Reject the whole presentation: an abbreviated review must never retain approval controls. */
export function boundInspectorState(state: InspectorState): InspectorState {
  try {
    let rows = 0, fields = 0, actions = state.actions.length;
    for (const section of state.sections) {
      fields += section.fields.length; rows += section.rows?.length ?? 0;
      for (const row of section.rows ?? []) { fields += row.fields.length; actions += row.actions?.length ?? 0; }
    }
    if (state.sections.length <= 64 && rows <= 512 && fields <= 4096 && actions <= 512 && state.commands.length <= 128 && Buffer.byteLength(JSON.stringify({ type: 'inspectors', state }), 'utf8') <= 256 * 1024) return state;
  } catch { /* Fail closed for unserializable presentation. */ }
  return emptyInspectorState(INSPECTOR_TABS.includes(state?.tab) ? state.tab : 'history', 'partial', 'Inspector presentation exceeded its display bound. Narrow the selection and refresh; no review actions are available.');
}
function escape(value: string): string { return value.replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!); }
export function inspectorPanelHtml(source: string, style: string, script: string, nonce: string): string {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${escape(source)}; script-src 'nonce-${escape(nonce)}';"><link rel="stylesheet" href="${escape(style)}"><title>VCP Inspectors</title></head><body>
<header><h1>Inspectors</h1><span id="inspector-phase" class="badge">disconnected</span></header>
<nav id="inspector-tabs" class="actions" aria-label="Inspector categories"></nav>
<p id="inspector-message" role="status" aria-live="polite"></p><p id="inspector-scope"></p><p id="inspector-task"></p>
<button id="inspector-refresh" type="button">Refresh</button><div id="inspector-actions" class="actions"></div><div id="inspector-sections"></div>
<h2>Submitted commands</h2><ul id="inspector-commands"></ul><p class="note">Command acceptance is not completion. Retained evidence and current authorization are separate facts.</p>
<script nonce="${escape(nonce)}" src="${escape(script)}"></script></body></html>`;
}
