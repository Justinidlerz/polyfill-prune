---
'@polyfill-prune/core': minor
'@polyfill-prune/vite-plugin': minor
---

Recognize Web API presence checks, compound guards, stable aliases, synchronous IIFE parameters, early-return continuations, and for/while fallbacks in the Rust core. Derive browser API locations from pinned Webref Web IDL and support versions from MDN BCD. Preserve getter evaluation, retained side effects, conflicting writes, and unsupported targets.

Add the `environment` option: the core defaults to ECMAScript APIs, while the Vite client plugin enables Window Web APIs. Include the Webref source revision in version metadata and dependency cache fingerprints.
