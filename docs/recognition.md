# Recognition and limitations

## Supported patterns

All removals require support in every configured target and no conflicting writes in retained code.

| Input pattern                                                | Behavior                                                           |
| ------------------------------------------------------------ | ------------------------------------------------------------------ |
| `if (!Object.assign) { install(); }`                         | Remove the missing-API branch                                      |
| `typeof Array.from === 'function' ? Array.from : fallback`   | Keep the native branch                                             |
| `Object.assign \|\| fallback`, `Object.assign ?? fallback`   | Remove the fallback                                                |
| `!Array.prototype.at && install()`                           | Remove the unreachable installation                                |
| `Object['assign']`                                           | Recognize string-literal property access                           |
| `!('fetch' in window)`                                       | Recognize property existence, including constant template keys     |
| `!('isIntersecting' in IntersectionObserverEntry.prototype)` | Check attribute presence without reading a getter                  |
| `probe() && !fetch` in an `if` or loop test                  | Remove the unreachable body while retaining evaluation of the test |
| `if (fetch) return;` followed by installation                | Remove the unreachable remainder of the same statement list        |
| `for (init(); !fetch; update()) { install(); }`              | Remove the body and update; retain the initializer and test        |
| `while (!fetch) { install(); }`                              | Remove the body; retain the test                                   |
| `const g = window; if (!g.fetch) { install(); }`             | Resolve stable local aliases and simple object destructuring       |
| `(function(g) { if (!g.fetch) install(); })(window)`         | Resolve simple parameters of direct synchronous IIFEs              |

Guards support `!`, `typeof`, equality and inequality, nullish checks, and logical combinations. Constant template strings can supply a `typeof` comparison value or an `in` key. No input function or runtime probe is executed.

When a branch is unreachable, its entire installation code can be removed: function assignments, native or vendor API references, synchronous factories, companion APIs such as `Request` and `Response`, and property descriptors containing getters or setters. Installation shape alone never authorizes deletion.

## Runtime environment

The core defaults to `environment: 'ecmascript'`. Use `environment: 'browser'` for code running in a Window realm. The Vite plugin selects this mode for its client transforms and skips SSR and worker builds.

Web API locations and types come from pinned W3C Webref Web IDL, with support versions from MDN BCD. Methods can be evaluated as functions. Attributes and constants support presence checks only: their values, getters, and truthiness are not evaluated. Secure-context-only and isolation-dependent APIs, members that cannot be placed on a prototype, and APIs missing reliable support data remain unchanged. A Node or Deno target disables Web API pruning even in browser mode.

`globalThis` access also requires target support for `globalThis`. `window` and `self` access is recognized for browser targets; callers must ensure that the corresponding global exists. Browser mode is not suitable for Worker or non-Window realms.

## Preservation rules

- Preserve missing support data, prefixed or flagged implementations, partial implementations, and removed support. `keep` preserves an API and its descendants.
- Retained assignments, destructuring writes, updates, deletions, property-writing calls, and recognized alias escapes block related removals. Writes inside removable branches do not conflict.
- Aliases require a stable binding and a known initializer on the current control path. Reads before initialization, reassignment, deferred function captures, recursive IIFE parameters, defaults/rest parameters, and IIFEs using `arguments` remain unresolved.
- A force flag such as `!fetch || force` leaves the installation reachable. Function-based bug checks remain opaque.
- Guards do not carry past loops or across deferred function boundaries. The first `do...while` iteration is retained. Loops, `throw`, and `try/finally` do not prove a continuation return.
- Global `eval` references, dynamic `Function` constructor calls, and `with` preserve the file. Hoisted `var` bindings survive removal. Function declarations and potentially observable lexical declarations prevent removal of the affected range.
- Expression replacements preserve indirect-call semantics. Source maps retain positions for copied code; generated scaffolding is unmapped.

## Usage boundaries

Native APIs must behave as provided by the configured runtimes. Analysis does not cross file boundaries or fully track object escape, custom installers, arbitrary alias-returning factories, or indirect changes by external functions. Use `keep` for APIs replaced by application code, or exclude modules/chunks through the Vite plugin's `filter`.

Generic core-js calls such as `$({ target, forced }, implementation)`, UMD installers, arbitrary module registries, and package fingerprints are not removal rules. Recognizable guards inside bundled or minified code are handled normally. Shared helpers and wrappers outside unreachable ranges remain for bundler tree shaking and minification.

[Back to the README](../README.md)
