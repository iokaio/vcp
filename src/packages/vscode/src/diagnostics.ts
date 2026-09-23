// SPDX-License-Identifier: Apache-2.0
/** Only SDK-authored categories are shown; raw errors may contain local secrets. */
export function safeFailure(error: unknown): string {
  const code = error && typeof error === 'object' && Object.hasOwn(error, 'code')
    ? Object.getOwnPropertyDescriptor(error, 'code')?.value : undefined;
  switch (code) {
    case 'UNSUPPORTED_VERSION': return 'The engine protocol is incompatible with this extension.';
    case 'CAPABILITY_UNAVAILABLE': return 'The engine does not provide the required read-only API.';
    case 'TIMEOUT': return 'The connection deadline expired. Inspect the engine state before reconnecting.';
    case 'INVALID_ARGUMENT': return 'Check the selected folder and absolute paths in User settings.';
    case 'DISPOSED': return 'The local connection is closed.';
    default: return 'The local connection is unavailable. Check the trusted engine path and initialized workspace.';
  }
}
