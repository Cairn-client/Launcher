# Cairn Project Contributors & Upstream Lineage

Cairn Client is free software under the GNU General Public License version 3 ([LICENSE](LICENSE)).

## Cairn Client (multi-game architecture and overhaul)

- cmpnn-romain: architecture refactoring, multi-game abstraction, UI redesign

## Kingdom Come: Deliverance II

The KD2M fork of Kingdom Come: Together, which Cairn Client grew out of: the KCD2 co-op work (shared world, NPCs and fights, doors and weather, faces and voices, items and horses), the launcher's menus and setup checklist, the installer and releases.

- cmpnn-romain: <https://github.com/cmpnn-romain/KD2M>
- DeepFriedDepp: <https://github.com/DeepFriedDepp/KingdomCome-Together>
- marczukmichal: <https://github.com/marczukmichal/kcd2-multiplayer>

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
