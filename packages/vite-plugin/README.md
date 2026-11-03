# vite-plugin-polyfill-prune

A polyfill pruning wrapper for Vite 6–8. Compatibility checks and AST rules run in the Rust API provided by `polyfill-prune`. Requires Node.js >= 22.12.0 and compliance with the Node.js requirements of your Vite version.

## Setup

Once published, install with `pnpm add -D vite-plugin-polyfill-prune`. To work from this repository, run `pnpm install && pnpm build`, followed by `pnpm example:vite`.

```ts
import { defineConfig } from 'vite';
import polyfillPrune from 'vite-plugin-polyfill-prune';

export default defineConfig({
  build: { target: ['chrome100', 'safari15.4'] },
  plugins: [polyfillPrune()],
});
```

## Options

| Option                      | Default                 | Description                                                                                          |
| --------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------- |
| `targets`                   | Resolved `build.target` | Explicit runtime target array; can override a build target that specifies only ES syntax             |
| `keep`                      | `[]`                    | Preserve APIs and their descendants                                                                  |
| `sourceMap`                 | `true`                  | Generate transformation maps for Vite to compose                                                     |
| `filter(id)`                | JS/TS module extensions | Receives module IDs during development and chunk filenames during production builds                  |
| `optimizeDeps`              | `true`                  | Run the core API during development dependency prebundling                                           |
| `onResult(result, context)` | None                    | Receive each analysis result; context contains `filename` and `stage`; async callbacks are supported |

`stage` is `dev`, `dependency`, or `build`. Virtual modules and requests with `?raw` or `?url` are skipped. If the filter captures mutable external state, use Vite's `--force` option to refresh the dependency cache after that state changes.

## Execution stages

| Scenario                         | Hook                 | Output                                                                     |
| -------------------------------- | -------------------- | -------------------------------------------------------------------------- |
| Development source               | `transform`, post    | Pruned code for requests and HMR updates                                   |
| Vite 6/7 dependency optimization | esbuild `onLoad`     | Pruned JavaScript input passed to esbuild                                  |
| Vite 8 dependency optimization   | Rolldown `transform` | Pruned code with source map chaining                                       |
| Production build                 | `renderChunk`, pre   | Pruned bundled chunks used by subsequent minification and hash calculation |

Production pruning runs on output chunks. If another output plugin injects polyfills after this plugin, adjust its hook order. Filtering chunk filenames does not filter individual source modules within those chunks.

Only the `client` environment is processed. SSR, worker, and other environments are skipped. The development dependency cache includes targets, `keep`, the filter's source text, and core/data versions in the plugin identifier. In Vite 6/7, an earlier custom esbuild loader that handles a file prevents this plugin's `onLoad` hook from processing it again.

The plugin removes only branches the core library can prove unnecessary. It does not guarantee removal of arbitrary bundled core-js code or custom installers. Syntax-only build targets such as `esnext` or `es2015` require an explicit `targets` option.
