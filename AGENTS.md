# Repository guidelines

- Do not commit, push, or add unit tests unless explicitly requested.
- Do not use a browser unless authorized.
- Use Rust + Oxc for parsing, scope analysis, compatibility checks, pruning, and source map generation.
- Keep the Vite wrapper limited to lifecycle hooks, option forwarding, and native API calls. Implement recognition rules in the Rust core.
- Derive browser support versions from pinned open-source datasets. Do not hardcode compatibility decisions.
- Update the README and documentation whenever features or options change.
