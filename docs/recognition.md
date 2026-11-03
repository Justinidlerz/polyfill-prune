# Recognition and limitations

## Supported patterns

| Input pattern                                              | Behavior                                                                                   |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `if (!Object.assign) { install(); }`                       | Remove the missing-API branch when every target supports the API                           |
| `typeof Array.from === 'function' ? Array.from : fallback` | Keep the native branch                                                                     |
| `Object.assign \|\| fallback`                              | Remove the fallback on the right                                                           |
| `!Array.prototype.at && install()`                         | Remove the unreachable installation call                                                   |
| `Object.assign ?? fallback`                                | Remove the nullish fallback                                                                |
| `Object['assign']`                                         | Recognize string-literal property access                                                   |
| `globalThis.Object.assign`                                 | Recognize the API when every target also supports `globalThis`                             |
| `window.Object.assign` / `self.Object.assign`              | Recognize browser targets only; callers must ensure the corresponding global object exists |

The analyzer recognizes global APIs without local shadowing. It supports `!`, `typeof`, equality and inequality checks, nullish checks, and logical combinations that can be evaluated statically. It does not execute function calls or runtime probes.

## Preservation rules

- Preserve APIs absent from the catalog, targets without support data, implementations requiring flags or prefixes, partial implementations, and entries with removed support.
- Check direct assignments, destructuring writes, updates, deletions, and standard property-writing calls affecting an API or its parent objects. Writes in retained code block the relevant pruning. Writes inside a branch being removed do not conflict.
- Do not expand local aliases into native APIs. Directly assigning a native object to a local variable conservatively preserves the related APIs.
- Global `eval` references, dynamic `Function` constructor calls, and `with` preserve the entire file. Hoisted `var` bindings survive branch removal. Branches involving function declarations are preserved.
- Expression replacements preserve call semantics, including indirect calls without a receiver.

## Usage boundaries

Global built-ins in the input must behave as provided by the target runtimes. Analysis does not cross file boundaries or fully track object escape, custom installers, or indirect changes to built-ins by external functions. Use `keep` for APIs replaced by application code, or exclude the relevant modules or chunks through the Vite plugin's `filter`.

The current rules do not recognize core-js calls such as `$({ target, forced }, implementation)`, generic UMD installers, or arbitrary module registries. Package names alone never trigger removal. Function-based bug checks are preserved: an existence check cannot prove that a historical implementation is free of semantic bugs.

Only code ranges proven unreachable are removed. Shared helpers, extra module wrappers, and unrelated code are left for the bundler's tree shaking and minification. Unminified builds may retain empty blocks and declarations.

[Back to the README](../README.md)
