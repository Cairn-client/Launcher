# Moving Kingdom Come: Deliverance II out of the launcher

The goal: the launcher is only a launcher. A game, KCD2 included, is its catalog entry (`game.json`) plus its pack (its mod repository's release);
adding or changing a game never needs a launcher release, except when the platform itself gains a capability.

Where KCD2 stands (measured on 0.3.13: about 15,400 lines in `Cairn.Launcher`, about 1,500 more in `Cairn.Setup` for its setup):

| Part | Where it lives today | About | Becomes |
| --- | --- | --- | --- |
| The in-game mod menu, notices, saves list | the mod's bridge (`CairnBridgeKcd2.exe`); the launcher draws | done | nothing more to move |
| **DLSS 5 / OptiScaler** (a setting, an install/remove tool run, a menu submenu) | launcher: `Dlss5.cs`, `Dlss5Service.cs`, `Home.Dlss5.cs`, parts of the settings window and the menu driver | 740 lines + hooks | a game **option** (below): the pack's program does the work, the bridge draws the submenu |
| **Setup checklist** (Steam, the game, the Modding Tools, hard links, C++ runtimes, lip-sync data, the mod placed) | `Cairn.Setup` (`SetupChecklist`, `SetupDetect`, `Workspace`, `ModInstall`, ...) and `Home.FirstRun.cs` | ~1,500 + 475 | pack-provided **setup steps**: the pack's program detects and does them, the launcher shows the list |
| **Host and Join** (relay, agent, injecting the plugin, the game started with the right arguments, the host code, friends' invites) | `Home.razor.cs`, `Home.ConnectPath.cs`, `Home.Beta.cs`, `Home.BetaPatch.cs`, `RelayReuse`, `NetService`, `AgentStatusBanner` | ~2,500 | a **session program** in the pack that starts and watches everything and reports its state (host code, players, errors) |
| **Start game takeover and characters** (the game's own main menu as the start screen, the agent's helper, start saves, Henry) | `Home.StartGame.cs`, `MenuTakeover.cs`, `ConnectGate.cs`, `AgentHelper.cs`, `Home.CarryScreen.cs` | ~1,300 | the session program (it has the game's console and log already) |
| Delete all saves, the log bundle, the bug report | `SaveCleaner.cs`, `LogBundle.cs`, `Home.Cairn.cs` | ~600 | bridge messages and the session program |

## The platform capabilities that make it possible (each is a launcher release, once)

1. **Game options**: toggles a game offers (DLSS 5 is one). `game.json` lists them; the launcher's settings window shows them; the pack's program does what a
   tick means (`check`, `apply on|off`) and reports. The bridge can draw submenus for them. *Used by KCD2's DLSS 5; any game can have its own.*
2. **Pack-provided setup steps**: the pack's program lists the steps it needs (and can do them) over the bridge, before the game runs.
3. **Pack-provided sessions**: Host and Join ask the pack's program to start a session and show what it reports (a code to share, who is in, what went wrong).

## Order, and what proves each stage

Each stage ships on the **beta** channel of the launcher's data first (the old code stays in the launcher and is used when a pack does not have the new part),
is tried in the game by a player, and only then does the old code go in a later release. A stage that fails its trial is not widened.

1. Options and DLSS 5. *Proof: DLSS 5 ticked and unticked in the game on an RTX card, the overlay submenu in the game's menu.*
2. Setup steps. *Proof: a PC with a step missing (the Modding Tools, the lip-sync data) is led through it; a ready PC shows all green with no change.*
3. Sessions: Host, Join, invites. *Proof: two players, host and join, with the old pack and the new pack side by side.*
4. Start game takeover and characters. *Proof: new adventure, continue, a joiner with a new character, a returning joiner.*
5. Delete the old code from the launcher once its stage is proven and one stable release has passed with the new path.

What stays in the launcher by design: the game list and pages, the pack store and updates, the catalog, the bridge server, the in-game menu drawing, the
queue and footer, settings, friends and presence (a launcher feature a game's session program may use).
