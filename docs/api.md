# Core API

## `prune(code, options): Promise<PruneResult>`

Parsing, scope analysis, compatibility checks, text pruning, and source map generation run on a Rust worker thread. The input code is never executed. The function returns transformed source without modifying files on disk.

| Parameter     | Type                        | Default        | Description                                                                    |
| ------------- | --------------------------- | -------------- | ------------------------------------------------------------------------------ |
| `code`        | `string`                    | Required       | JavaScript, TypeScript, or JSX source                                          |
| `targets`     | `string[]`                  | Required       | Minimum runtime versions, such as `chrome100`, `safari15.4`, or `node22`       |
| `environment` | `'ecmascript' \| 'browser'` | `'ecmascript'` | Enable Window Web APIs in browser mode; browser targets are required           |
| `filename`    | `string`                    | `input.js`     | Determines the parsing mode and source map filename                            |
| `keep`        | `string[]`                  | `[]`           | Preserve an API and its descendants, such as `Promise` or `Array.prototype.at` |
| `sourceMap`   | `boolean`                   | `true`         | Generate a source map for transformed files                                    |

Supported runtime names are `chrome`, `edge`, `firefox`, `safari`, `ios`, `opera`, `node`, and `deno`, each followed by an explicit version. When a runtime appears more than once, the lowest version is used. Pruning requires explicit support data for every target. Browser mode assumes a Window realm; Node and Deno targets preserve Web APIs. Secure-context-only APIs remain unchanged. See [recognition](recognition.md) for supported guards and preservation rules.

Syntax targets such as `es2015` and `esnext` do not establish native API support. They are ignored when mixed with runtime targets; a call with only syntax targets is rejected. Callers must expand Browserslist queries and aliases such as `baseline-widely-available` first. The Vite plugin uses targets already resolved by Vite.

| Result field  | Description                                                                                 |
| ------------- | ------------------------------------------------------------------------------------------- |
| `code`        | Transformed source, or the original string when nothing changes                             |
| `changed`     | Whether pruning was applied                                                                 |
| `map`         | A Source Map v3 object, or `null` when unchanged or source maps are disabled                |
| `removals`    | Records containing `start`, `end`, `kind`, and `features`                                   |
| `diagnostics` | Parse errors, semantic errors, or the reason a file was preserved; `[]` when there are none |

Each removal's `[start, end)` range uses UTF-8 byte offsets into the input and covers the entire replaced expression or statement. Nested records can overlap, so their lengths cannot be summed to calculate bytes removed. `kind` is `if`, `conditional`, `logical`, `early-return`, `for`, or `while`. An early-return record covers the removed continuation; loop records cover the body or update expression. Source map columns use UTF-16 and map to this call's input. The build tool composes the map with upstream maps.

Invalid options and internal transformation errors reject the Promise. Input with syntax or semantic errors is returned unchanged with diagnostics.

## `version()`

Synchronously returns `{ core, parser, bcd, typescript, webref, features }`: the core version, Oxc version, MDN BCD version, TypeScript data source version, Webref source commit, and number of cataloged APIs.

[Back to the README](../README.md)
