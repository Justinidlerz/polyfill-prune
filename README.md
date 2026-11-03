# polyfill-prune

Remove JavaScript polyfill branches that are no longer needed for your target runtimes. The Rust + Oxc core exposes a JavaScript API through N-API. The Vite plugin is a separate package in this repository.

| Package                           | Purpose                                                                        |
| --------------------------------- | ------------------------------------------------------------------------------ |
| `@polyfill-prune/core`            | Asynchronous pruning API, removal records, and source maps                     |
| `@polyfill-prune/vite-plugin`     | Development transforms, dependency prebundling, and production chunk pruning   |
| `@polyfill-prune/core-<platform>` | Native binaries for five platforms, selected automatically by the core package |

## Usage

Requires Node.js >= 22.12.0. Install from the public npm registry with `pnpm add -D @polyfill-prune/core`. For a local build, see [development](docs/development.md).

```ts
import { prune } from '@polyfill-prune/core';

const result = await prune(code, {
  targets: ['chrome100', 'safari15.4'],
  filename: 'vendor.js',
});
// result.code / result.map / result.removals / result.diagnostics
```

```ts
import { defineConfig } from 'vite';
import polyfillPrune from '@polyfill-prune/vite-plugin';

export default defineConfig({
  build: { target: ['chrome100', 'safari15.4'] },
  plugins: [polyfillPrune()],
});
```

For example, `Object.assign || function fallback() { /* … */ }` retains the native implementation and removes the fallback when every target supports `Object.assign`. Likewise, `if (!Array.prototype.at) { /* install */ }` loses its installation branch when every target supports the API.

## Scope

Compatibility information comes from a pinned version of MDN Browser Compat Data. API types and prototype locations come from the TypeScript standard libraries. The core recognizes native API existence and `typeof` checks in `if` statements, conditional expressions, and `&&` / `||` / `??` expressions. Bundled or minified code is supported when these structures remain recognizable.

Only branches proven unnecessary are pruned. Generic core-js installers, arbitrary polyfill fingerprints, runtime bug checks, and whole-package removal are outside the current scope. Unrecognized structures, local shadowing, dynamic scope, and APIs with conflicting writes are preserved. See [recognition and limitations](docs/recognition.md).

## Documentation

- [Core API](docs/api.md)
- [Vite plugin](packages/vite-plugin/README.md)
- [Recognition and limitations](docs/recognition.md)
- [Development and data updates](docs/development.md)
- [Building, packaging, and releases](docs/releasing.md)
- [Third-party data licenses](licenses/README.md)
