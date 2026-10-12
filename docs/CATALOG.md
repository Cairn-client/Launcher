# The game catalog

The launcher does not contain a list of games. Which games there are, what each one is, what it needs, what its mod does, which mod
versions are blocked and what the players should be told are **data**, read from the network and kept on disk. Adding a game, or
fixing a description, a requirement or a picture, announcing a known issue or pulling a bad mod version is a commit in a repository,
**not a new launcher**. The launcher is released only when its own code changes.

| Where | What it is |
| --- | --- |
| [`Cairn-client/games`](https://github.com/Cairn-client/games) | `games.json` (the list, shared requirements, blocked versions, notices) and `games/<id>/` (`game.json` and pictures) for games that have no mod repository yet |
| a game's own repository (`Cairn-client/mod-kcd2`) | its releases carry `game.json` and its pictures beside the mod pack (`docs/PACKS.md`) |
| `Cairn.Launcher/Seed/` | embedded in the launcher: the one game it can show before it has ever reached the network (KCD2), so a first start offline is not empty |

## How the launcher reads it

1. `games.json` from `https://raw.githubusercontent.com/Cairn-client/games/main/` (`CAIRN_GAMES_URL` points it at a local folder served
   over http, for working on the catalog; every `game.json` is then read from that folder too).
2. For each game in the list: `game.json` from the latest release of the game's repository (`releases/latest/download/game.json`),
   or if there is none, from `games/<id>/game.json` in the games repository. Pictures are looked up in the same folder.
3. Every success is kept in `%LOCALAPPDATA%\Cairn\cache\catalog\`. Offline, that copy is used; with no copy, the seed.
   A listed game whose `game.json` can be had from nowhere keeps what it was (or what the seed knows); it is not dropped.
4. At start, and from **Manage updates > Check for updates**; then the mods' own catalogs (`pack-index.json`) are read.

Nothing in these files is run, and every file is checked before use (`Cairn.Setup/Catalog.cs`): a file that is too big (256 KB), of
another schema or malformed is ignored and the last good copy stays; ids are plain; a **picture is a plain file name looked up
beside the file that names it, never a URL**; texts are short and have no markup. `tools/Validate-Catalog.ps1` in the games
repository applies the same rules in CI.

## `games.json` (schema 1)

```json
{
  "schema": 1,
  "games": [ { "id": "kcd2", "repo": "Cairn-client/mod-kcd2" }, { "id": "cp77" } ],
  "packs": [ { "id": "core", "repo": "Cairn-client/core" } ],
  "components": { "kcd2-game": { "kind": "steam-app", "name": "Kingdom Come: Deliverance II", "appId": 1771300 } },
  "blocked": [ { "pack": "kcd2", "version": "0.1.0", "reason": "Saves can be lost." } ],
  "notices": [ { "id": "sleep", "title": "Sleep", "text": "...", "kind": "known-issue", "games": ["kcd2"] } ]
}
```

* **games** -- `id`, and `repo` when the game has a mod repository (its releases carry the pack and `game.json`).
* **packs** -- repositories of packs that are not a game's mod: shared parts a pack `dependsOn`, and content packs.
* **components** -- something a game needs, defined once and named by every game that needs it: two games that need the same thing say so
  by name. `steam-app` (an app id) is checked and linked by the launcher ("Steam library", "Store page"); another kind is shown by name only.
* **blocked** -- a mod `version`, or every version up to `max`. A blocked version is never offered; a player who has it is warned on
  the game's page and in the ready check; if the newest release is blocked the page says it is on hold.
* **notices** -- `known-issue` or `news`. Shown on Home (the player can hide one) and always under Help. `games` limits it to players who have
  one of them; `fromLauncher` / `untilLauncher` limit it to launcher versions; `until` (UTC date) ends it.

## `game.json` (schema 1)

`id`, `name`, `developer`, `franchise`, `supported` (false lists a game with no multiplayer yet: its page shows, Host and Join stay off),
`steamAppId` (the app the game runs from), `storeAppId` (the app whose store page describes it, when different), `exe`, `description`,
`accent` (`#rrggbb`), `images` (`background`, `logo`, `hero`, `poster`, `screenshots`), `requires` (component ids, or inline definitions),
`features` (the "What the mod does" card: groups of `name` / `detail` / `status` = `ready`, `inTest`, `planned`) and `content` (ids of content packs).

A game that has a mod repository keeps its `game.json` and pictures in `game/` there (`tools/Test-GameJson.ps1` checks them) and the
release workflow attaches them to every release as flat assets.

## Packs: more than a game's mod

`pack.json` and `pack-index.json` (docs/PACKS.md) have a few fields for this:

* `kind` -- `mod` (the default: programs and a mod folder) or `content` (only files). `installTo: "start-save"` makes a content pack copy
  its files into the launcher's start-save folder (never overwriting what the player has); no other target exists.
* `dependsOn: [ { "id": "core", "min": "1.2.0" } ]` -- other packs this one needs, at that version or newer. The launcher downloads what is
  missing or too old first (the repository of each comes from `packs` in `games.json`), and does not download the pack if one cannot be had.
  Dependencies are downloaded and kept up to date; **a mod's programs must still be in its own pack** (a contract cannot yet name a program in another pack).
* `menuProtocol` -- the version of the in-game menu messages (`menu_state`, the `menuRequest` function) the pack speaks. The launcher draws
  the menu and the mod sends its items; this launcher speaks 1 and refuses a pack that needs a newer one with "the mod's menu needs a newer launcher".

## Release channels

Each game's mod follows **stable** (the latest release) or **beta** (the newest release, pre-releases included), chosen by the player with
the **Beta** switch on the mod row of the game's page (`ModChannels` in `settings.json`). In the mod repository, `RELEASE_CHANNEL`
(`stable` or `beta`) says how the next tagged release is published: `beta` makes it a GitHub pre-release. A beta older than the stable
release is never offered. Both players should use the same mod version (the same MAJOR.MINOR).

## What is still in the launcher

The UI and the engine: the pack store, the queue and footer, the in-game menu drawer, the setup checklist. KCD2's Modding Tools setup
(finding the tools, linking the game's files) is still KCD2 code, and the setup checklist still names KCD2's Steam ids; making setup
steps data too is the next step when a second game needs them.
