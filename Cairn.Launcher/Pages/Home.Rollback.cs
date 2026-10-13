// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Launcher.Models;
using Cairn.Setup;
using Serilog;

namespace Cairn.Launcher.Pages
{
    /// <summary>
    /// Undoing a bad update: an update keeps the version it replaced (PackStore.PreviousDir), and "Go back" puts it in the game again.
    /// The version that was left is held back, so the background updates do not bring it straight back; the player can still update by hand.
    /// </summary>
    public partial class Home
    {
        // the earlier version of a mod, read once per installed version (a render must not read files)
        private readonly Dictionary<string, (string Current, InstalledPack? Previous)> previousPackCache = new(StringComparer.OrdinalIgnoreCase);

        private InstalledPack? PreviousPackOf(GameProfile g)
        {
            string current = InstalledPackOf(g)?.Version ?? "";
            if (previousPackCache.TryGetValue(g.Id, out var c) && c.Current == current) return c.Previous;
            var prev = current.Length == 0 ? null : PackStore.ReadPrevious(PacksRoot, g.Id);
            previousPackCache[g.Id] = (current, prev);
            return prev;
        }

        private async Task RollbackPackAsync(string gameId)
        {
            if (packBusyId is not null || GameProfileRegistry.Get(gameId) is not { } game) return;
            if (gameRunning) { SetPackMessage("Close the game first.", error: true); return; }
            if (PackStore.ReadPrevious(PacksRoot, gameId) is not { } old) { SetPackMessage("There is no earlier version to go back to.", error: true); return; }
            string left = InstalledPackOf(game)?.Version ?? "";

            // what the version being left put in the game comes out first, the way Remove does it
            if (IsExternalMod(game))
            {
                if (GameFolderOf(game) is { } folder)
                {
                    var r = await Task.Run(() => ExternalInstall.Unplace(folder, PacksRoot, gameId));
                    if (!r.Ok) { SetPackMessage($"Could not take the mod out of the game ({r.Message}).", error: true); return; }
                }
            }
            else if (PackSetupProgram(gameId) is { } program)
            {
                var r = await PackSetup.DoAsync(program, "unplace-mod", GameExeOf(gameId), _ => { }, _ => { });
                if (!r.Ok && r.Message.Length > 0) { SetPackMessage($"Could not take the mod out of the game ({r.Message}).", error: true); return; }
            }
            if (gameId == packSessionGameId) ReleasePackSession();

            var back = await Task.Run(() => PackStore.Rollback(PacksRoot, gameId));
            installedPackCache.Remove(gameId);
            previousPackCache.Remove(gameId);
            ModLua.Reload();
            if (!back.Ok) { SetPackMessage($"Could not go back: {back.Message}.", error: true); return; }

            if (left.Length > 0) { settings.HeldBackMods[gameId] = left; WriteSettings("a mod version held back"); }
            Log.Information("packs: {Game} went back from {From} to {To}", gameId, left, old.Version);
            await InstallModAsync(gameId);
            SetPackMessage($"{game.DisplayName} mod is back to {old.Version}. {left} will not be installed by itself again.", error: false);
            if (gameId == ActiveGameId) await CheckSetupAgainAsync();
        }
    }
}
