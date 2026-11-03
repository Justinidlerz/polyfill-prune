# Building, packaging, and releases

Releases use Changesets, napi-rs builds for five platforms, and optional npm dependencies for platform binaries. The core package and platform packages share a fixed version. The Vite package is versioned independently.

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
