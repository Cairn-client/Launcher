# Versioning

Cairn follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) strictly.

## The one source of truth

The repo-root `VERSION` file holds the release number and nothing else: one line, `MAJOR.MINOR.PATCH`.
Every project (launcher, setup, master server, protocol) and the installer read it. Builds only read it and never write it.
Do not hardcode a version anywhere else.

## Format

```
MAJOR.MINOR.PATCH
```

- Three non-negative integers separated by dots.
- No leading zeros (`0.1.4`, never `0.01.4`).
- No `v` prefix, no whitespace, no fourth component, no pre-release or build suffix.
- Valid: `0.1.4`, `0.2.0`, `1.0.0`. Invalid: `v0.1.4`, `0.1`, `0.1.4.0`, `0.1.4-beta`.

This is the stable subset of the SemVer regex. Pre-release (`-beta.1`) and build metadata (`+sha`) are not used,
because .NET's `System.Version`, which the projects use to read `VERSION`, cannot parse them.

## Enforcement

- Every .NET build fails with a clear message if `VERSION` is not valid (`ValidateCairnVersionFile` in `Directory.Build.props`).
- `tools/Build-Installer.ps1` applies the same rule to `VERSION` and to an explicit `-Version`.

## When to bump

While MAJOR is `0`, the API is not stable and MINOR plays the role of "breaking".

| Bump | When | Network compatibility |
| --- | --- | --- |
| MAJOR | Stable release, or an intentional full break (1.0.0 and after) | Update together |
| MINOR | A change to the master server or presence API, or a new feature other launchers must understand | Update together |
| PATCH | Bug fix or internal change | Any `x.y.*` works with any other `x.y.*` |

Rules:

1. A bump increases exactly one component and resets the ones to its right to `0`
   (`0.1.4` to `0.1.5`, `0.1.4` to `0.2.0`).
2. Versions only go up. A published version is never reused or edited.
3. Two launchers talk to the master server by `MAJOR.MINOR`. A game's mod has its own repository and its own version: two players need the same `MAJOR.MINOR` of that mod, which the launcher checks.

## Releasing

Releases are cut by CI from a tag. Do not build or upload installers by hand.

1. Edit `VERSION` (one valid bump of the latest release) and merge it to `main` with the title `chore(release): X.Y.Z`.
2. Tag that commit `X.Y.Z` (bare version, no `v`) and push the tag.
3. `.github/workflows/release.yml` checks the tag is on `main`, equals `VERSION` and is a valid bump, runs every test gate,
   builds `Cairn-Setup-X.Y.Z.exe` and publishes the release. The notes are the changelog only, generated from the
   `feat` / `fix` / `perf` / `refactor` commits since the previous release (`tools/New-ReleaseNotes.ps1`).

`.github/workflows/ci.yml` runs on every push and pull request: `tools/Test-VersionPolicy.ps1` (VERSION is strict SemVer and
at most one valid bump above the latest release), the .NET build and unit tests, the mod's synthetic suites and static
checks, and the native build and tests.

Commit titles follow Conventional Commits (`feat:`, `fix:`, `perf:`, `refactor:`, `chore:`, `docs:`, `test:`, `ci:`), because the
changelog is derived from them.
