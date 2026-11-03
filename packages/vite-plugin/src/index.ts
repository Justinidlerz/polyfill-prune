import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import * as vite from 'vite';
import type { Plugin, ResolvedConfig } from 'vite';
import { prune, version } from 'polyfill-prune';
import type { PruneOptions, PruneResult } from 'polyfill-prune';

export interface PolyfillPruneOptions extends Omit<PruneOptions, 'filename' | 'targets'> {
  /** Defaults to the resolved Vite build.target. Syntax-only targets require an override. */
  targets?: string[];
  /** Defaults to JS/TS module files. Receives module IDs in dev and chunk filenames in build. */
  filter?: (id: string) => boolean;
  /** Process dependency optimizer inputs during development. Default: true. */
  optimizeDeps?: boolean;
  onResult?: (
    result: PruneResult,
    context: { filename: string; stage: 'dev' | 'dependency' | 'build' },
  ) => void | Promise<void>;
}

export function polyfillPrune(options: PolyfillPruneOptions = {}): Plugin {
  let config: ResolvedConfig;
  let targets: string[];
  let active = true;
  const accepts = (id: string) =>
    !id.includes('\0') &&
    !/[?&](raw|url)(?:&|$)/.test(id) &&
    (options.filter?.(id) ?? /\.[cm]?[jt]sx?(?:\?|$)/.test(id));

  async function transform(code: string, id: string, stage: 'dev' | 'dependency' | 'build') {
    if (!active || !accepts(id)) return null;
    const result = await prune(code, {
      targets,
      filename: id.split('?')[0],
      keep: options.keep,
      sourceMap: options.sourceMap,
    });
    for (const diagnostic of result.diagnostics) {
      config.logger.warn(`[polyfill-prune] ${id}: ${diagnostic}`);
    }
    await options.onResult?.(result, { filename: id, stage });
    return result.changed ? { code: result.code, map: result.map } : null;
  }

  // The optimizer hashes plugin names. Include native/data versions and options
  // after config resolution so a target change cannot reuse stale dependency code.
  const optimizer = {
    name: 'polyfill-prune:dependencies',
    transform: (code: string, id: string) => transform(code, id, 'dependency'),
  };
  const legacyOptimizer = {
    name: 'polyfill-prune:dependencies',
    setup(build: {
      onLoad(
        options: { filter: RegExp; namespace: string },
        callback: (args: { path: string }) => Promise<
          | {
              contents: string;
              loader: 'js';
              resolveDir: string;
            }
          | undefined
        >,
      ): void;
    }) {
      build.onLoad({ filter: /\.[cm]?js$/, namespace: 'file' }, async ({ path }) => {
        if (!active || !accepts(path)) return;
        const result = await transform(await readFile(path, 'utf8'), path, 'dependency');
        if (!result) return;
        const comment = '//# source' + 'MappingURL=data:application/json;base64,';
        const map = result.map
          ? `\n${comment}${Buffer.from(JSON.stringify(result.map)).toString('base64')}`
          : '';
        return { contents: result.code + map, loader: 'js', resolveDir: dirname(path) };
      });
    },
  };

  return {
    name: 'vite-plugin-polyfill-prune',
    enforce: 'post',
    applyToEnvironment(environment) {
      return environment.name === 'client';
    },
    configEnvironment(name, _, env) {
      if (name !== 'client' || env.command !== 'serve' || options.optimizeDeps === false) return;
      return 'rolldownVersion' in vite
        ? { optimizeDeps: { rolldownOptions: { plugins: [optimizer] } } }
        : { optimizeDeps: { esbuildOptions: { plugins: [legacyOptimizer] } } };
    },
    async configResolved(resolved) {
      config = resolved;
      active = !resolved.build.ssr && !resolved.isWorker;
      if (!active) return;
      const target = options.targets ?? resolved.build.target;
      targets = typeof target === 'string' ? [target] : target || [];
      // Rust owns target parsing and validates even when no module needs pruning.
      await prune('', { targets, sourceMap: false });
      const fingerprint = createHash('sha256')
        .update(
          JSON.stringify({
            targets,
            keep: options.keep,
            sourceMap: options.sourceMap,
            filter: options.filter?.toString(),
            version: version(),
          }),
        )
        .digest('hex');
      optimizer.name += `:${fingerprint}`;
      legacyOptimizer.name = optimizer.name;
    },
    transform: {
      order: 'post',
      handler(code, id, transformOptions) {
        if (
          config.command !== 'serve' ||
          transformOptions?.ssr ||
          this.environment?.config.consumer === 'server'
        )
          return null;
        return transform(code, id, 'dev');
      },
    },
    renderChunk: {
      order: 'pre',
      handler(code, chunk) {
        if (config.command !== 'build' || this.environment?.config.consumer === 'server')
          return null;
        return transform(code, chunk.fileName, 'build');
      },
    },
  };
}

export default polyfillPrune;
