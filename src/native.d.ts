import type { PruneOptions } from './types.js';
export declare function pruneRaw(code: string, options: PruneOptions): Promise<string>;
export declare function version(): {
  core: string;
  parser: string;
  bcd: string;
  typescript: string;
  webref: string;
  features: number;
};
