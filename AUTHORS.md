# Cairn Project Contributors & Upstream Lineage

Cairn Client is free software under the GNU General Public License version 3 ([LICENSE](LICENSE)).

## Cairn Client (multi-game architecture and overhaul)

- cmpnn-romain: architecture refactoring, multi-game abstraction, UI redesign

## The games' mods

Each game's mod lives in its own repository of the Cairn-client organisation, forked from the project listed for it. The mod's code and credits are its authors'; Cairn only
packs it (and, for some games, the loader it runs on) and installs it. Each keeps its own licence.

| Game | Mod | Upstream | Licence |
| --- | --- | --- | --- |
| Kingdom Come: Deliverance II | Kingdom Come: Together | DeepFriedDepp: <https://github.com/DeepFriedDepp/KingdomCome-Together> | GPL-3.0 |
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

| Component | Game | Source | Notes |
| --- | --- | --- | --- |
| BepInEx | Dyson Sphere Program | <https://github.com/BepInEx/BepInEx> | The build Dyson Sphere Program mods use, from the Thunderstore package xiaoye97/BepInEx |
| UE4SS | Voices of the Void | <https://github.com/UE4SS-RE/RE-UE4SS> | The build of the Thunderstore package Thunderstore/unreal_shimloader |
| Harmony | Kerbal Space Program, RimWorld | <https://github.com/pardeike/Harmony> | With the Luna Multiplayer client, and inside Prepatcher |
| Prepatcher | RimWorld | <https://github.com/Zetrith/Prepatcher> | By Zetrith (MIT) |
| MelonLoader | Slime Rancher 2 | <https://github.com/LavaGang/MelonLoader> | Apache-2.0 |
| Dead Cells Core Modding API | Dead Cells | <https://github.com/dead-cells-core-modding/core> | MIT |
| Chairloader | Prey | <https://github.com/thelivingdiamond/Chairloader> | By thelivingdiamond (GPL-3.0). Not carried by the pack: the player installs it |
| F4SE (Fallout 4 Script Extender) | Fallout 4 | <https://f4se.silverlock.org/> | By the Silverlock team. Not carried by the pack: Cairn downloads it from its authors' site on the player's PC, checked against a pinned hash, and the player can also install it by hand |
| Starlight | Slime Rancher 2 | by ThatFinn | A requirement of that mod. Its licence forbids bundling it, so Cairn does not carry it: the player takes it from its author's page |
## Additional libraries and modules

Not part of this repository's own code; each keeps its own license.

| Library | Used for |
| --- | --- |
| Photino.Blazor | Native window host for the Blazor launcher UI |
| Serilog (with the Extensions, Settings.Configuration, Console and File sinks) | Logging |
| DiscordRichPresence | Discord presence |
| System.Text.Json | JSON |
| xunit, coverlet | Tests |
| Steamworks | Steam networking and friends, through the installed Steam client |
| DLSS 5 Autopilot by Kizzuwatnaa (MIT, <https://github.com/Kizzuwatnaa/DLSS5-Autopilot>) | The optional DLSS 5 setting downloads its release at run time and runs its command line; nothing of it ships with Cairn. The parts it fetches (ReShade, OptiScaler and its forks, the add-ons, NVIDIA's runtimes) keep their own licences |
## Terms that stay with the code

- Keep the author credits above and in the source files' headers, and the link to the official repository
  (<https://github.com/DeepFriedDepp/KingdomCome-Together>).
- A modified version must say that it is one. Cairn Client is a modified version, not the official Kingdom Come: Together.

## Whose games they are

The games Cairn Client supports, their assets and their content (including the artwork shown on the game pages) are not ours: they belong to their studios and publishers. Cairn Client is unofficial and free, and is not affiliated with or endorsed by any of them.
