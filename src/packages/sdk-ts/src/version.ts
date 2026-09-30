// SPDX-License-Identifier: Apache-2.0
import { createRequire } from 'node:module';

// Resolve beside the emitted module in both the source package and VSIX bundle.
export const SDK_VERSION = (createRequire(import.meta.url)('../package.json') as { version: string }).version;
