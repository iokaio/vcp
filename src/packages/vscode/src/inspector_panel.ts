// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { randomBytes } from 'node:crypto';
import { boundInspectorState, emptyInspectorState, inspectorPanelHtml, parseInspectorMessage, type InspectorMessage, type InspectorState, type InspectorTab } from './inspector_view_model.js';

export class InspectorPanel implements vscode.WebviewViewProvider, vscode.Disposable {
  #view: vscode.WebviewView | undefined;
  #listeners: vscode.Disposable[] = [];
  #tab: InspectorTab = 'history';
  constructor(private readonly uri: vscode.Uri, private readonly onMessage: (message: InspectorMessage) => Promise<void>, private readonly onVisibility: (visible: boolean) => void) {}
  resolveWebviewView(view: vscode.WebviewView): void {
    this.dispose(); this.#view = view;
    const media = vscode.Uri.joinPath(this.uri, 'media');
    view.webview.options = { enableScripts: true, localResourceRoots: [media] };
    view.webview.html = inspectorPanelHtml(view.webview.cspSource, view.webview.asWebviewUri(vscode.Uri.joinPath(media, 'connection.css')).toString(), view.webview.asWebviewUri(vscode.Uri.joinPath(media, 'inspectors.js')).toString(), randomBytes(24).toString('hex'));
    this.#listeners.push(view.webview.onDidReceiveMessage((raw: unknown) => {
      if (this.#view !== view || !view.visible) return;
      const message = parseInspectorMessage(raw);
      if (!message) return;
      if (message.action === 'ready') this.#clear();
      void this.onMessage(message).catch(() => {});
    }), view.onDidChangeVisibility(() => {
      if (this.#view !== view) return;
      this.#clear(); this.onVisibility(view.visible);
    }), view.onDidDispose(() => { if (this.#view === view) this.dispose(); }));
    this.#clear(); this.onVisibility(view.visible);
  }
  #clear(): void {
    if (this.#view) void this.#view.webview.postMessage({ type: 'inspectors', state: emptyInspectorState(this.#tab, 'stale', 'Refresh to authorize the current inspector selection.') });
  }
  publish(state: InspectorState): void {
    if (!this.#view?.visible) return;
    const bounded = boundInspectorState(state); this.#tab = bounded.tab;
    void this.#view.webview.postMessage({ type: 'inspectors', state: bounded });
  }
  dispose(): void {
    const hadView = !!this.#view;
    this.#clear(); this.#view = undefined;
    for (const listener of this.#listeners) listener.dispose();
    this.#listeners = [];
    if (hadView) this.onVisibility(false);
  }
}
