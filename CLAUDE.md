# Cairn Client: the launcher

A launcher that turns single-player games into shared ones: one player hosts, a friend joins, both play in the same world.
Windows only. Free software under GPLv3; see `AUTHORS.md` for the lineage (it is a modified version of earlier work).

This repository is the **launcher** only. Each game's mod (its game files, the agent and relay that run a session, the native
plugin) lives in its own repository in the same organization and is downloaded as a pack (`docs/PACKS.md`); the launcher updates
each mod on its own.

## Layout

| Path | What it is |
| --- | --- |
| `Cairn.Launcher/` | the launcher: Blazor UI hosted in a native window (.NET 8, WinForms/WPF host) |
| `dotnet/Cairn.Setup/` | first-run setup, install checks, the pack store (`Packs.cs`), `CairnSetup.exe` helper source |
| `dotnet/Cairn.SetupHost/`, `Cairn.Setup.FakeTool/` | the NativeAOT setup helper, and a fake tool for the setup tests |
| `dotnet/Cairn.MasterServer/` | the server list and friend presence |
| `dotnet/Cairn.Protocol/` | the API shared by the launcher and the master server (`MasterApi`, `PresenceApi`) |
| `dotnet/Cairn.*.Tests/` | xunit tests (`Setup`, `MasterServer`) |
| `installer/` | Inno Setup script (`Cairn.iss`) |
| `tools/` | build and test scripts (PowerShell) |
| `docs/` | `VERSIONING.md` (release rules), `PACKS.md` (mod downloads) |

## Build and test

```bash
dotnet build Cairn.sln -c Release
dotnet test dotnet/Cairn.Setup.Tests/Cairn.Setup.Tests.csproj -c Release     # also: MasterServer.Tests
```

`tools/Build-Installer.ps1` runs the gates and builds `release/Cairn-Setup-X.Y.Z.exe`. It needs Inno Setup 6 (found in Program Files
or `%LOCALAPPDATA%\Programs`). CI (`.github/workflows/ci.yml`) runs the same suites on every push; `release.yml` builds and publishes
on a tag.

## Releases and versions

- `VERSION` is the single source of truth: strict `MAJOR.MINOR.PATCH`, no `v`. It started at 0.1.0 with this repository.
- Tag `X.Y.Z` on `main` and push the tag; CI builds the installer and publishes it. Do not upload by hand.
- Release notes are the changelog only (generated from `feat`/`fix`/`perf`/`refactor` commit titles, plus any `* ` line in such a
  commit's body): nothing else goes in them.
- Commits follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `ci:`).

## Conventions

- Names: `Cairn.*` for projects and namespaces. Name things after what they do, never after a ticket or work-order number.
- Every source file starts with the license header naming `AUTHORS.md`. Never remove or reword it, and never remove credits from `AUTHORS.md`.
- Games are data: register them in `Cairn.Launcher/Models/GameProfileRegistry.cs` (title, developer, franchise, Steam app id, artwork in
  `Cairn.Launcher/wwwroot/img/games/`) and say where their mod repository is.
- The launcher never contains game-specific session code: what a mod needs from the launcher is described by the pack's `pack.json`.

## Pitfalls

- Text files are LF in the repo (Windows converts on checkout); scripted edits must not add a BOM or change line endings.
- The installer's end-to-end tests (`tools/first_run/Test-SetupCases.ps1`) compile a real Setup; they need Inno Setup and a few minutes.
- Never commit `settings.json` or anything under `release/`.
