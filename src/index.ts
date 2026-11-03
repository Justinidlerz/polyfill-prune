import { pruneRaw, version as nativeVersion } from './native.js';
import type { PruneOptions, PruneResult } from './types.js';

export type * from './types.js';

/** Parse, analyze and prune on the native worker pool. */
export async function prune(code: string, options: PruneOptions): Promise<PruneResult> {
  return JSON.parse(await pruneRaw(code, options)) as PruneResult;
}

export const version = nativeVersion;
