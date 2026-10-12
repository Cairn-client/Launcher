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

* The installer (`installer/Cairn.iss`, `CairnSetup.exe` from `dotnet/Cairn.SetupHost`) and the `Cairn.Setup` classes it uses (`SetupProbe`, the workspace links, `GameLocator`): the
  installer's first-run pages find Steam and the Modding Tools. They duplicate what the pack's `setup` does and are the next thing to retire.
* The settings.json keys of the KCD2 session (`GamePath`, `HostPort`, `ConnectGate`, ...) are the player's file; the launcher passes them to the session as they are.
