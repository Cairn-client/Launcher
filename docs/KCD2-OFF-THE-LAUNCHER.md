# Kingdom Come: Deliverance II is a pack, not launcher code

The goal: the launcher is only a launcher. A game, KCD2 included, is its catalog entry (`game.json`) plus its pack (its mod repository's release);
adding or changing a game never needs a launcher release, except when the platform itself gains a capability.

## What moved, and where it lives now

| Part | Now in | Protocol |
| --- | --- | --- |
| The in-game mod menu, notices, spoken lines, the saves list | the mod's bridge (`CairnBridgeKcd2.exe`, started with the game) | [BRIDGE.md](BRIDGE.md) |
| DLSS 5 / OptiScaler (an option and a menu submenu) | the pack's program (`options`) and bridge | [PACKS.md](PACKS.md) |
| The setup checklist (Steam, the game, the Modding Tools, the links, the mod placed, the runtimes, the lip-sync data) | the pack's program (`setup check`, `setup do`) | [PACKS.md](PACKS.md) |
| Host and Join: the relay, the game's start, the CONNECT gate, the plugin, the agent, Steam friends, the connection test | the pack's program (`session`) | [SESSION.md](SESSION.md) |
| Start Game / Join Game on the game's main menu, the host's start save, the joiner's character | the pack's program (`session`) | [SESSION.md](SESSION.md) |
| Delete all saves, Add a start save, the game's side of Save the logs | the pack's program (`saves delete-all`, `startsave add`, `logs collect`) | [PACKS.md](PACKS.md) |

The launcher draws what the program says and passes the player's choices back. The old launcher code for all of it is deleted: a pack that does not do these things
(a mod older than 0.2.11) is told to be updated from the game's page.

## What stays in the launcher, by design

The game list and pages, the pack store and updates, the catalog, the bridge server, the in-game menu drawing, the download queue, settings, friends and
presence (a launcher feature), the update window.

## What is still KCD2-shaped

* Nothing else in the launcher or its installer knows KCD2: the installer installs the launcher and nothing about any game (it no longer looks for Steam or the Modding Tools), and
  the launcher has no code for the Modding Tools build, the workspace links or the mod folder.
* The game's executable is a setting only for KCD2 (`GameExeIsSetting`: the player's `GamePath` key, set by the pack's setup from what it found, with the pack's own note shown beside it); every other game is found in Steam or where the player pointed, and
  the pack is handed that file. The folder a game's options belong in is the pack's (`found.gameDir` of `setup check`); a pack that does not report it (a KCD2 mod before 0.2.13) has no option status until it is updated. Everything else that names a pack (setup, session, start-game, logs, saves, the menu bridge) is looked up by the selected game's id, so a second game whose pack says
  `"session": true` / `"setup": true` is run the same way, with no launcher change.
* The settings.json keys of the KCD2 session (`GamePath`, `HostPort`, `ConnectGate`, ...) are the player's file; the launcher passes them to the session as they are.
