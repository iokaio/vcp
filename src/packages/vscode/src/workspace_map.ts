// SPDX-License-Identifier: Apache-2.0
import { realpath } from 'node:fs';
import { win32 } from 'node:path';
import type { WorkspaceView } from '@vcp/sdk' with { 'resolution-mode': 'import' };
import { localDrivePath } from './trust.js';

export interface WorkspaceBinding {
  readonly folderUri: string;
  readonly executionHost: string;
  readonly workspace: string;
  readonly root: string;
  readonly rootId: string;
  readonly bindingRevision: string;
  readonly workspaceRevision: string;
  readonly authorityRevision: string;
}
/** Canonicalize only the user-selected local folder, never a path supplied by a model. */
export async function canonicalWorkspaceRoot(path: string): Promise<string> {
  if (!localDrivePath(path)) throw new Error('local workspace required');
  const resolved = await new Promise<string>((resolve, reject) => realpath.native(path, (error, value) => error ? reject(error) : resolve(value)));
  if (!localDrivePath(resolved)) throw new Error('local workspace required');
  // Rust's Windows canonical spelling includes the extended path prefix.
  return win32.toNamespacedPath(resolved);
}
export class WorkspaceMap {
  #generation = 0;
  #bindings = new Map<string, WorkspaceBinding>();
  get generation(): number { return this.#generation; }
  invalidate(): number { this.#bindings.clear(); return ++this.#generation; }
  bind(generation: number, folderUri: string, view: WorkspaceView): boolean {
    if (generation !== this.#generation) return false;
    if (view.root_id == null || view.binding_revision == null) throw new Error('workspace binding projection missing');
    const value: WorkspaceBinding = Object.freeze({ folderUri, executionHost: view.host, workspace: view.workspace, root: view.root, rootId: view.root_id, bindingRevision: view.binding_revision, workspaceRevision: view.revision, authorityRevision: view.authority_revision });
    this.#bindings.set(JSON.stringify([folderUri, view.host]), value);
    return true;
  }
  get(folderUri: string, executionHost: string): WorkspaceBinding | undefined {
    return this.#bindings.get(JSON.stringify([folderUri, executionHost]));
  }
}
