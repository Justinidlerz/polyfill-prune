export interface PruneOptions {
  /** Explicit minimum runtimes, e.g. ['chrome100', 'safari15.4']. */
  targets: string[];
  /** Enable Window Web APIs with browser targets. Default: ecmascript. */
  environment?: 'ecmascript' | 'browser';
  /** Input filename, also determines JavaScript/TypeScript/JSX parsing. Default: input.js. */
  filename?: string;
  /** Preserve exact APIs and their descendants, e.g. ['Promise', 'Array.prototype.includes']. */
  keep?: string[];
  /** Generate a source map. Default: true. */
  sourceMap?: boolean;
}

export interface SourceMap {
  version: 3;
  file?: string;
  sources: string[];
  sourcesContent?: (string | null)[];
  names: string[];
  mappings: string;
}

export interface Removal {
  /** Replaced input range, in UTF-8 bytes. */
  start: number;
  end: number;
  kind: 'if' | 'conditional' | 'logical' | 'early-return' | 'for' | 'while';
  features: string[];
}

export interface PruneResult {
  code: string;
  /** Maps the returned code to this call's input. Null when unchanged or disabled. */
  map: SourceMap | null;
  changed: boolean;
  removals: Removal[];
  /** Parse/semantic diagnostics or a reason the file was preserved. */
  diagnostics: string[];
}
