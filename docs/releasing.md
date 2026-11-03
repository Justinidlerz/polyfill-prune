# Building, packaging, and releases

Releases use Changesets, napi-rs builds for five platforms, and optional npm dependencies for platform binaries. The core package and platform packages share a fixed version. The Vite package is versioned independently.

All packages publish publicly to `https://registry.npmjs.org/` under `@polyfill-prune`. The repository `.npmrc` and package `publishConfig` use this registry. Package names are `@polyfill-prune/core`, `@polyfill-prune/vite-plugin`, and `@polyfill-prune/core-<platform>`.

## First publication

The npm account must own or have publishing access to the `polyfill-prune` organization. Create the organization with the free public-package plan if it does not exist. Use Node.js >= 22.14 and npm >= 11.15 with account-level two-factor authentication.

1. Log in with `npm login --registry=https://registry.npmjs.org/` and verify the account with `npm whoami`.
2. Set the GitHub repository variable `NPM_RELEASE_USER` to the GitHub username authorized to publish.
3. Download the `polyfill-prune-packages` artifact from a successful main-branch Release run into `artifacts/`. It contains all seven package archives, their integrity manifest, and the publishing script.
4. Run `node artifacts/publish-ci.mjs --dry-run` to verify the archives, then `NPM_TAG=latest node artifacts/publish-ci.mjs --bootstrap`. Complete npm authentication prompts. This local first publication disables provenance because local sessions cannot generate GitHub Actions attestations. The bootstrap option is rejected in CI.
5. For each package in `artifacts/release-manifest.json`, run `npm trust github <package> --allow-publish --repo Justinidlerz/polyfill-prune --file release.yml`. Verify each configuration with `npm trust list <package>`.

Trusted publishers require packages to exist on npm, so configure them after the first publication. Subsequent releases run through GitHub Actions with OIDC and provenance. See the [npm trusted publishing documentation](https://docs.npmjs.com/trusted-publishers/) and [npm trust requirements](https://docs.npmjs.com/cli/v11/commands/npm-trust/).

## Workflow

| Trigger                                                | Behavior                                                                                          |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| Pull request                                           | Run checks, prepare a beta version, build all five platforms, run examples, and package artifacts |
| Push to `main`                                         | Run checks, consume Changesets, prepare stable versions, and package artifacts                    |
| Manual run without a run ID                            | Run checks and package artifacts only                                                             |
| Manual run with a completed main-branch Release run ID | Resume publishing from the original archives without rebuilding                                   |

Publishing requires a pending release and a triggering user matching `NPM_RELEASE_USER`. Pull requests must also come from the same repository, be non-draft, and have a matching author. npm publishing uses Trusted Publishing with OIDC. Configure the repository and `release.yml` as the trusted publisher for every platform package, the core package, and the Vite package.

The publishing script publishes packages in dependency order and checks archive integrity for versions that already exist. After a stable release succeeds, the workflow records version files and tags. Local builds and Changesets commands do not create commits.

## Package checks

`pnpm pack:release` requires `dist/` to contain exactly the five native binaries listed in `.github/native-targets.json`. It checks native versions, declaration files, entry points, platform metadata, and exact optional dependency versions, then generates `artifacts/*.tgz` and a release manifest.

A local build produces only the current platform's binary and does not replace the five-platform CI build. Publishing requires the GitHub remote, repository variables, and npm Trusted Publisher configuration.

[Back to the README](../README.md)
