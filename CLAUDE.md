# Cairn Client: the launcher

A launcher that turns single-player games into shared ones: one player hosts, a friend joins, both play in the same world.
Windows only. Free software under GPLv3; see `AUTHORS.md` for the lineage (it is a modified version of earlier work).

This repository is the **launcher** only. Each game's mod (its game files, the agent and relay that run a session, the native
plugin) lives in its own repository in the same organization and is downloaded as a pack (`docs/PACKS.md`); the launcher updates
each mod on its own.

## Layout

| Path | What it is |
| --- | --- |
| `rust/crates/cairn-launcher/` | the launcher: a native egui window (pages in `src/ui/`, the seed in `src/seed/`) |
| `rust/crates/cairn-setup/` | where Steam and the games are, the pack store (`packs/`), the catalog, the clients of the pack program (`pack_setup`, `pack_session`, `game_options`), the bridge server |
| `rust/crates/cairn-protocol/` | the API shared by the launcher and the master server (`master_api`, `presence_api`), in Rust |
| `dotnet/Cairn.MasterServer/` | the server list and friend presence (the one part still in C#) |
| `dotnet/Cairn.Protocol/` | the same API for the master server (`MasterApi`, `PresenceApi`) |
| `dotnet/Cairn.MasterServer.Tests/` | xunit tests |
| `installer/` | Inno Setup script (`Cairn.iss`) |
| `tools/` | build and test scripts (PowerShell) |
| `docs/` | `VERSIONING.md` (release rules), `PACKS.md` (mod downloads), `CATALOG.md` (the game catalog) |

## Build and test

```bash
cd rust && cargo build --release && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
dotnet build Cairn.sln -c Release                                             # the master server
dotnet test dotnet/Cairn.MasterServer.Tests/Cairn.MasterServer.Tests.csproj -c Release
```

`tools/Build-Installer.ps1` runs the gates and builds `release/Cairn-Setup-X.Y.Z.exe`. It needs Inno Setup 6 (found in Program Files
or `%LOCALAPPDATA%\Programs`). CI (`.github/workflows/ci.yml`) runs the same suites on every push; `release.yml` builds and publishes
on a tag.

## Releases and versions

- `VERSION` is the single source of truth: strict `MAJOR.MINOR.PATCH`, no `v`. It started at 0.1.0 with this repository. The Rust launcher reads it at build time (`build.rs`); the crates' own `version` is not the release number.
- Tag `X.Y.Z` on `main` and push the tag; CI builds the installer and publishes it. Do not upload by hand.
- Release notes are the changelog only (generated from `feat`/`fix`/`perf`/`refactor` commit titles, plus any `* ` line in such a
  commit's body): nothing else goes in them.
- Commits follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `ci:`).

## Conventions

- Names: `cairn-*` for crates and `Cairn.*` for .NET projects and namespaces. Name things after what they do, never after a ticket or work-order number.
- Every source file starts with the license header naming `AUTHORS.md`. Never remove or reword it, and never remove credits from `AUTHORS.md`.
- Games are data: the list, their requirements, artwork, feature cards, blocked mod versions and notices are the catalog (`docs/CATALOG.md`),
  in the `Cairn-client/games` repository and each game's own repository; the launcher ships only the KCD2 seed (`rust/crates/cairn-launcher/src/seed/`).
  Do not put a game's data back into the launcher.
- The launcher never contains game-specific session code: what a mod needs from the launcher is described by the pack's `pack.json`.

## Pitfalls

- Text files are LF in the repo (Windows converts on checkout); scripted edits must not add a BOM or change line endings.
- The installer (`installer/Cairn.iss`) knows nothing of any game; compile it with `tools/Build-Installer.ps1` (needs Inno Setup). Do not run the compiled Setup on a machine with a real Cairn install: it shares the real one's AppId.
- Never commit `settings.json` or anything under `release/`.
