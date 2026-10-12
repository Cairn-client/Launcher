# Mod packs

The installer carries the launcher and nothing game specific. Each game's mod lives in **its own repository** (for Kingdom Come:
Deliverance II, `Cairn-client/mod-kcd2`) and is published as a **pack**: one zip, attached to the repository's releases. The launcher
downloads the pack when the player clicks **Download mod** on the game's page, checks it, places the mod files in the game's mod
folder and keeps the programs that run a session in its own pack store. A mod is updated on its own, without a new launcher, and
the launcher shows **Update to X** when the repository has a newer release.

## Why

- Someone who plays one game downloads one mod, not twelve.
- A mod updates by itself the way a Steam game does; the launcher does not have to be released again.
- A mod can follow the project it was forked from (the mod repository records which version in `UPSTREAM.md`).
- A game that is not on Steam works the same way: point the launcher at its executable (**Locate game...**) and download the mod.

## What a mod release publishes

| Asset | What it is |
| --- | --- |
| `pack-index.json` | the catalog: the pack this release carries, with the zip's size and sha256 and the launcher it needs |
| `Cairn-Pack-<id>-X.Y.Z.zip` | the pack |
| `game.json` and its pictures | what the game is, for the launcher (`docs/CATALOG.md`) |

The launcher reads `https://github.com/<owner>/<repo>/releases/latest/download/pack-index.json` (the repository comes from the games catalog, `docs/CATALOG.md`) and downloads `https://github.com/<owner>/<repo>/releases/download/X.Y.Z/<file>`.

## What a pack holds

```
pack.json       the contract: what the launcher needs to drive the mod's programs
manifest.txt    MOD|<path>|<size>|<sha256> and BIN|<path>|<size>|<sha256>, one line per file
mod\...         MOD files: put into <Modding Tools>\Mods\<modFolder>\ (and nothing else: loose sources there stop the game)
bin\...         BIN files: the agent, the relay, the native plugin; kept in the pack store and started from there
```

`pack.json` (schema 2): `id`, `game`, `version`, `upstream` (the version of the project the mod is based on), `modFolder`,
`luaPrefix` (the prefix of the mod's script functions the launcher calls), `agent`, `relay`, `native`, `brandPak` (paths inside the
pack), `agentConfig` (the agent's settings file name), `launcherMin` (the oldest launcher that can use this pack) and
`replacesFolders` (earlier mod folders of ours in the game's Mods folder; each is removed only when it is provably ours).

## How the launcher treats a pack

1. It fetches the catalog of every game that has a mod repository.
2. On **Download mod** (or **Update**) it downloads the zip, hashing as it goes. The size and sha256 must be what the catalog says,
   and the launcher must be at least `launcherMin`.
3. The contract must be one this launcher understands, every file in the zip must be listed in `manifest.txt` with the listed size and
   hash, nothing may be unlisted, nothing may land outside the pack's own folder, and the contract may only name programs the pack
   carries. Any failure keeps the previous copy untouched.
4. The verified pack is kept in `%LOCALAPPDATA%\Cairn\packs\<id>\`. What the programs wrote beside themselves (their settings and
   logs, in `files\bin`) is carried over to the next pack. The setup checklist then places the MOD files into the game's mod folder
   (only the listed files, written under a temporary name and moved into place).
5. The programs are started from the pack store. A path in `settings.json` (`AgentPath`, `RelayPath`, `DllPath`) still wins, for
   development.
6. **Remove** deletes the downloaded copy and takes the listed files back out of the game's mod folder; anything else in that folder
   is left alone.

## The game list (the sidebar)

The sidebar is a set of folders. Search matches title, id, developer and franchise (and opens every folder with a match);
**All / On this PC / Mod installed** filters it. The folders can be grouped **by title** (one per first letter), **by developer**
(one per studio) or **by franchise** (one per series or license, so a game and its sequel sit together although different studios
made them). Each folder opens and closes; the two buttons beside the grouping open or close them all, and the A-Z button flips the
order. What you chose is remembered in `%LOCALAPPDATA%\Cairn\list-prefs.json`.

A game is "on this PC" when Steam has it installed or the player located it; a game's mod is "installed" once its pack is downloaded.

## Adding a game's mod

1. Make the mod's repository (the pack build script is `tools/Build-Pack.ps1` in `Cairn-client/mod-kcd2`, a good model), with its `game/game.json`.
2. List the game in `games.json` of the games catalog repository, with the mod's repository (`docs/CATALOG.md`). No launcher change.
3. Release the mod; the launcher finds it.
