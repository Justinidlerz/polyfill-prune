# Development and data updates

## Building

Toolchain: Rust >= 1.97.0, Node.js >= 22.12.0, and pnpm 10.20.0. The napi CLI and Vite versions used for development may require a newer Node.js patch release; follow their `engines` requirements.

```sh
pnpm install --frozen-lockfile
pnpm build
pnpm typecheck
pnpm check:rust
pnpm check:scripts
pnpm example
pnpm example:vite
```

`pnpm build` builds the native library for the current platform, followed by the core JavaScript API and the Vite package. Building from source requires a C/C++ linker toolchain. Published packages use prebuilt binaries for the matching platform.

| Directory                   | Contents                                                                |
| --------------------------- | ----------------------------------------------------------------------- |
| `native/src`                | Rust analysis, pruning, source maps, N-API bindings, and data generator |
| `native/data/web-apis.json` | Normalized Web IDL API locations, kinds, and source commit              |
| `native/data/compat.json`   | Version-controlled compatibility data embedded in the native library    |
| `src`                       | JavaScript API and TypeScript types                                     |
| `packages/vite-plugin`      | Separate Vite package                                                   |
| `npm/*`                     | Platform packages                                                       |
| `examples`                  | API, bundled dependency, and Vite examples                              |
| `scripts`                   | Build, version synchronization, and release tools                       |

## Updating data

```sh
pnpm data:sync
pnpm build
```

The generator uses the versions of `@mdn/browser-compat-data` and `typescript` pinned in the lockfile. After upgrading these development dependencies, regenerate the catalog and review the data diff. Web API entries also use the checked-in Webref snapshot. Compatibility data is not downloaded during normal runtime or package installation.

Oxc parses TypeScript's `lib.es*.d.ts` files to extract global declarations, constructor and callable interfaces, static methods, and prototype methods. These are then matched to BCD's `javascript.builtins` entries. Namespaces, inheritance, accessors, optional properties, and computed properties are excluded when a complete mapping is unavailable.

To update Web API declarations, check out the desired commit of [W3C Webref's curated branch](https://github.com/w3c/webref/tree/curated), including `ed/idlparsed`, then run:

```sh
cargo run --locked -p polyfill_prune_core --bin sync-webref -- /path/to/webref native/data/web-apis.json
pnpm data:sync
pnpm build
```

`sync-webref` requires an unmodified parsed-data directory and records the checkout's commit. To reproduce the current snapshot, use the `revision` in `native/data/web-apis.json`. Both generators run in Rust. Web IDL supplies Window exposure, methods, attributes, mixins, and partial interfaces; BCD supplies support versions. Secure-context and isolation requirements are excluded. Prototype mappings exclude `LegacyUnforgeable` members. Unmapped namespaces and inherited members remain absent. Attributes and constants are marked for presence checks only.

Update the API, recognition, and plugin documentation alongside new rules or options. Local commands do not automatically commit or push changes.

[Back to the README](../README.md)
