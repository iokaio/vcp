// SPDX-License-Identifier: Apache-2.0
import { win32 } from 'node:path';
import { fileURLToPath } from 'node:url';

export interface ConnectionSelection {
  workspaceUri: string;
  workspacePath: string;
  executable: string;
  executableSource: 'global';
  dataPath?: string;
  workspaceTrusted: boolean;
  remoteName?: string;
}
export function localDrivePath(value: string): boolean {
  return typeof value === 'string' && !value.includes('\0') && win32.isAbsolute(value)
    && /^(?:\\\\\?\\)?[a-zA-Z]:[\\/]/.test(value);
}
/** Restricted mode permits this observer-only profile; it never grants execution trust. */
export function connectionRestriction(selection: ConnectionSelection, platform: string): string | undefined {
  if (platform !== 'win32' || selection.remoteName) return 'Only a local Windows extension host is supported.';
  let uri: URL;
  try { uri = new URL(selection.workspaceUri); } catch { return 'Select an existing local workspace folder.'; }
  if (uri.protocol !== 'file:' || uri.hostname !== '' || !localDrivePath(selection.workspacePath)) return 'Select an existing local Windows drive folder; remote and network roots are unavailable.';
  try {
    const uriPath = win32.normalize(fileURLToPath(uri, { windows: true })).toLowerCase();
    const selected = win32.normalize(selection.workspacePath.replace(/^\\\\\?\\/, '')).toLowerCase();
    if (uriPath !== selected) return 'The selected folder URI and local path do not match.';
  } catch { return 'Select an existing local workspace folder.'; }
  if (selection.executableSource !== 'global' || !localDrivePath(selection.executable)) return 'Choose an absolute trusted engine executable in User settings.';
  if (selection.dataPath !== undefined && !localDrivePath(selection.dataPath)) return 'The configured engine data directory must be an absolute local Windows path.';
  return undefined;
}
