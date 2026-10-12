# Mod packs

The installer carries the launcher and nothing game specific. Each game's mod is a
**pack**: a small zip published as an asset of the same GitHub release as the installer. The launcher downloads the pack for
its own version when the player clicks **Download mod** on the game's page, checks it, and places it in the game's mod folder.

## Why

- Someone who plays one game downloads one mod, not twelve.
- A game that is not on Steam works the same way: point the launcher at its executable (**Locate game...**) and download the mod.
- One repository, one release: the packs are built and versioned together with the client they belong to.

## What a release publishes

| Asset | What it is |
| --- | --- |
| `Cairn-Setup-X.Y.Z.exe` | the installer (launcher, agent, relay, native plugin) |
| `packs.json` | the catalog: which packs this release has, with each zip's size and sha256 |
| `Cairn-Pack-<id>-X.Y.Z.zip` | one mod pack per game that has a mod |

A pack zip holds `manifest.txt` (one `MOD|<path>|<size>|<sha256>` line per file) and the files at those paths. Paths are relative to
the game's mod folder; for Kingdom Come: Deliverance II that is `<Modding Tools>\Mods\cairn`.

## How the launcher treats a pack

1. It fetches `packs.json` from the release that carries **its own version**
   (`https://github.com/Cairn-client/launcher/releases/download/X.Y.Z/packs.json`), so a pack never meets a client it was not built for.
2. On **Download mod** it downloads the zip, hashing as it goes. The size and sha256 must be what the catalog says.
3. Every file in the zip must be listed in `manifest.txt` with the listed size and hash, nothing may be unlisted, and nothing may
   land outside the pack's own folder. Any failure keeps the previous copy untouched.
4. The verified pack is kept in `%LOCALAPPDATA%\Cairn\packs\<id>\`. The setup checklist then places it into the game's mod folder
   (the same placement as before: only the listed files, written under a temporary name and moved into place).
5. **Remove** deletes the downloaded copy and takes the listed files back out of the game's mod folder; anything else in that
   folder is left alone.

## The game list

The sidebar is a set of folders. Search matches title, id, developer and franchise (and opens every folder with a match);
**All / On this PC / Mod installed** filters it. The folders can be grouped **by title** (one per first letter), **by developer**
(one per studio) or **by franchise** (one per series or license, so Kerbal Space Program and its sequel sit together although
different studios made them). Each folder opens and closes; the two buttons beside the grouping open or close them all, and
the A-Z button flips the order. What you chose is remembered in `%LOCALAPPDATA%\Cairn\list-prefs.json`.

A game is "on this PC" when Steam has it installed or the player located it; a game's mod is "installed" once its pack is downloaded.

## Adding a game's mod

1. Register the game in `Cairn.Launcher/Models/GameProfileRegistry.cs` (title, developer, franchise, Steam app id, artwork).
2. Add an entry to the `$packs` list in `tools/Build-Packs.ps1` with the files that make up the mod.
3. Release as usual: CI builds the pack and publishes it with the installer.
