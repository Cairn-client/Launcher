# Cairn Project Contributors & Upstream Lineage

Cairn Client is free software under the GNU General Public License version 3 ([LICENSE](LICENSE)).

## Cairn Client (multi-game architecture and overhaul)

- cmpnn-romain: architecture refactoring, multi-game abstraction, UI redesign

## The games' mods

Each game's mod lives in its own repository of the Cairn-client organisation, forked from the project listed for it. The mod's code and credits are its authors'; Cairn only
packs it (and, for some games, the loader it runs on) and installs it. Each keeps its own licence.

| Game | Mod | Upstream | Licence |
| --- | --- | --- | --- |
| Kingdom Come: Deliverance II | Kingdom Come: Together (the KD2M fork), which Cairn Client grew out of: the shared world, NPCs and fights, doors and weather, faces and voices, items and horses | cmpnn-romain: <https://github.com/cmpnn-romain/KD2M>; DeepFriedDepp: <https://github.com/DeepFriedDepp/KingdomCome-Together>; marczukmichal: <https://github.com/marczukmichal/kcd2-multiplayer> | GPL-3.0 |
| Noita | Noita Entangled Worlds | IntQuant and contributors: <https://github.com/IntQuant/noita_entangled_worlds> | Apache-2.0 |
| Dyson Sphere Program | Nebula Multiplayer Mod | NebulaModTeam: <https://github.com/NebulaModTeam/nebula> | GPL-3.0 |
| Voices of the Void | Multivoid | Pelmentor and the VOTV-MP team: <https://github.com/VOTV-MP/Multivoid> | MIT |
| Kerbal Space Program | Luna Multiplayer | the LunaMultiplayer team: <https://github.com/LunaMultiplayer/LunaMultiplayer> | MIT |
| RimWorld | Multiplayer | Zetrith and the RimWorld Multiplayer Team: <https://github.com/rwmt/Multiplayer> | MIT |
| Cities: Skylines | CSM (Cities: Skylines Multiplayer) | the CSM team: <https://github.com/CitiesSkylinesMultiplayer/CSM> | MIT |
| Slime Rancher 2 | Ranching Together (SR2MP) | pyeight and contributors: <https://github.com/pyeight/SlimeRancher2Multiplayer> | GPL-3.0 |
| Dead Cells | Dead Cells Multiplayer Mod | vaiserYT: <https://github.com/vaiserYT/DeadCellsMultiplayerMod> | MIT |
| Grand Theft Auto: San Andreas | CoopAndreas | Tornamic: <https://github.com/Tornamic/CoopAndreas> | GPL-3.0 |
| Prey | CoopPrototype (prey-coop) | x0rp01s0n and contributors: <https://github.com/x0rp01s0n/prey-coop> | GPL-3.0 |
| Fallout 4 | F4MP (Fallout 4 Multiplayer) | saywhy2me, from the F4MP of cokwa and its original authors: <https://github.com/saywhy2me/Fallout4Multiplayer> | GPL-3.0 |

What a mod's pack carries beyond the mod itself, each under its own licence and credited to its authors:

- BepInEx (Dyson Sphere Program), in the build Dyson Sphere Program mods use, from the Thunderstore package xiaoye97/BepInEx: <https://github.com/BepInEx/BepInEx>
- UE4SS, in the build of the Thunderstore package Thunderstore/unreal_shimloader (Voices of the Void): <https://github.com/UE4SS-RE/RE-UE4SS>
- Harmony, with the Luna Multiplayer client (Kerbal Space Program) and inside Prepatcher (RimWorld): <https://github.com/pardeike/Harmony>
- Prepatcher by Zetrith (MIT, RimWorld): <https://github.com/Zetrith/Prepatcher>
- MelonLoader (Apache-2.0, Slime Rancher 2): <https://github.com/LavaGang/MelonLoader>
- Dead Cells Core Modding API (MIT, Dead Cells): <https://github.com/dead-cells-core-modding/core>
- Chairloader by thelivingdiamond (GPL-3.0, Prey): <https://github.com/thelivingdiamond/Chairloader>. Prey's pack does not carry it: the player installs it.
- F4SE, the Fallout 4 Script Extender, by the Silverlock team (Fallout 4): <https://f4se.silverlock.org/>. Fallout 4's pack does not carry it: Cairn downloads it from its authors' site on the player's PC, checked against a pinned hash, and the player can also install it by hand.
- Starlight by ThatFinn (Slime Rancher 2) is a requirement of that mod. Its licence forbids bundling it, so Cairn does not carry it: the player takes it from its author's page.
## Additional libraries and modules

Not part of this repository's own code; each keeps its own license.

- Photino.Blazor: native window host for the Blazor launcher UI
- Serilog (with the Extensions, Settings.Configuration, Console and File sinks): logging
- DiscordRichPresence: Discord presence
- System.Text.Json: JSON
- xunit, coverlet: tests
- Steamworks (Steam networking and friends, through the installed Steam client)
- DLSS 5 Autopilot by Kizzuwatnaa (MIT, <https://github.com/Kizzuwatnaa/DLSS5-Autopilot>): the optional DLSS 5 setting downloads its release at run time and runs its command line; nothing of it ships with Cairn. The parts it fetches (ReShade, OptiScaler and its forks, the add-ons, NVIDIA's runtimes) keep their own licences

## Terms that stay with the code

- Keep the author credits above and in the source files' headers, and the link to the official repository
  (<https://github.com/DeepFriedDepp/KingdomCome-Together>).
- A modified version must say that it is one. Cairn Client is a modified version, not the official Kingdom Come: Together.

## Whose games they are

The games Cairn Client supports, their assets and their content (including the artwork shown on the game pages) are not ours: they belong to their studios and publishers. Cairn Client is unofficial and free, and is not affiliated with or endorsed by any of them.
