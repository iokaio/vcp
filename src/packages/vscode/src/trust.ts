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
/** Early lexical check only; native loading checks canonical roots and file identity. */
export function publisherProfileRestriction(selection: ConnectionSelection, profile: string, platform: string): string | undefined {
  const restriction = connectionRestriction(selection, platform);
  if (restriction) return restriction;
  if (!selection.workspaceTrusted) return 'A trusted editor workspace is required for publisher control.';
  if (!localDrivePath(profile)) return 'Choose a local absolute publisher profile file.';
  const normalized = win32.normalize(profile.replace(/^\\\\\?\\/, '')).toLowerCase();
  // dataPath is the registry/config root, not the resolved canonical store.
  // Only the native loader can resolve and exclude that store's actual path.
  for (const root of [selection.workspacePath]) {
    const relative = win32.relative(win32.normalize(root.replace(/^\\\\\?\\/, '')).toLowerCase(), normalized);
    if (relative === '' || (!relative.startsWith(`..${win32.sep}`) && relative !== '..' && !win32.isAbsolute(relative))) return 'Choose a publisher profile outside the workspace; native loading also excludes canonical storage.';
  }
  return undefined;
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
