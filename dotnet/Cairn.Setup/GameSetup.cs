// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Setup;

/// <summary>The kinds of check a game's <c>setup</c> list (its game.json) can ask for. A fixed set: the catalog names a check, the launcher does it.</summary>
public static class SetupKinds
{
    public const string SteamRunning = "steam-running";
    public const string GameFound = "game-found";
    public const string ModInstalled = "mod-installed";
    public const string SteamApp = "steam-app";
    public const string GameFile = "game-file";
    public const string FreeDisk = "free-disk";

    public static bool IsKnown(string kind) => kind is SteamRunning or GameFound or ModInstalled or SteamApp or GameFile or FreeDisk;
}

/// <summary>
/// One line of a game's setup: what to check (<c>kind</c>), how it is called on the Ready check (<c>title</c>), what to tell the player when it is
/// not so (<c>detail</c>), and its parameters (<c>appId</c> of a Steam app, <c>path</c> of a file in the game's folder, <c>mb</c> of free disk).
/// An <c>optional</c> line is a warning when it fails, not a stop.
/// </summary>
public sealed record SetupRule(string Kind, string Title, string Detail, uint AppId = 0, string Path = "", int Mb = 0, bool Optional = false);

public enum SetupState { Ok, Warn, Fail }

public sealed record SetupResult(string Title, SetupState State, string Detail);

/// <summary>What the checks ask of this PC; the launcher answers from Steam, the game's folder and the pack store, the tests from a fake.</summary>
public interface IGameSetupHost
{
    bool SteamRunning();
    bool GameFound();
    bool ModInstalled();
    bool SteamAppInstalled(uint appId);
    bool GameFileExists(string relativePath);
    /// <summary>Free space, in MB, on the drive of the game's folder; null when it is not known.</summary>
    long? FreeDiskMb();
}

/// <summary>Does a game's setup list: the same Ready check for every game whose setup is data (a game with a mod that runs its own session).</summary>
public static class GameSetup
{
    public const int MaxRules = 12;

    /// <summary>What a game with no <c>setup</c> of its own is asked: Steam is running, the game is found, the mod is installed.</summary>
    public static IReadOnlyList<SetupRule> Defaults { get; } = new[]
    {
        new SetupRule(SetupKinds.SteamRunning, "Steam", "Start Steam and sign in: the game starts through it."),
        new SetupRule(SetupKinds.GameFound, "Game", "Cairn has not found the game on this PC. Use Locate game."),
        new SetupRule(SetupKinds.ModInstalled, "Mod", "Download the mod first: it is installed in the game's folder."),
    };

    public static IReadOnlyList<SetupResult> Evaluate(IReadOnlyList<SetupRule> rules, IGameSetupHost host)
    {
        var results = new List<SetupResult>();
        foreach (var r in rules.Take(MaxRules))
        {
            (bool ok, string good, string title) = r.Kind switch
            {
                SetupKinds.SteamRunning => (host.SteamRunning(), "Steam is running.", "Steam"),
                SetupKinds.GameFound => (host.GameFound(), "Found on this PC.", "Game"),
                SetupKinds.ModInstalled => (host.ModInstalled(), "Installed in the game's folder.", "Mod"),
                SetupKinds.SteamApp => (host.SteamAppInstalled(r.AppId), "Installed in your Steam library.", "Steam app"),
                SetupKinds.GameFile => (host.GameFound() && host.GameFileExists(r.Path), "There.", "File"),
                SetupKinds.FreeDisk => (host.FreeDiskMb() is not { } mb || mb >= r.Mb, "Enough free space.", "Disk space"),   // an unknown size is not a failure
                _ => (true, "", r.Title),
            };
            if (!SetupKinds.IsKnown(r.Kind)) continue;   // a check this launcher does not know is skipped, not failed
            string shownTitle = r.Title.Length > 0 ? r.Title : title;
            results.Add(ok ? new SetupResult(shownTitle, SetupState.Ok, good)
                           : new SetupResult(shownTitle, r.Optional ? SetupState.Warn : SetupState.Fail, r.Detail.Length > 0 ? r.Detail : "Not so yet."));
        }
        return results;
    }

    /// <summary>The setup list of a game's <c>game.json</c>, read with its limits; null when it is not valid (the whole file is then refused).</summary>
    internal static List<SetupRule>? Parse(System.Text.Json.JsonElement root)
    {
        var rules = new List<SetupRule>();
        foreach (var e in GameList.Items(root, "setup"))
        {
            if (e.ValueKind != System.Text.Json.JsonValueKind.Object || rules.Count >= MaxRules) return null;
            string kind = GameList.Str(e, "kind");
            if (!SetupKinds.IsKnown(kind)) return null;
            string title = CatalogLimits.Text(GameList.Str(e, "title"), 60), detail = CatalogLimits.Text(GameList.Str(e, "detail"), 200);
            uint appId = e.TryGetProperty("appId", out var a) && a.ValueKind == System.Text.Json.JsonValueKind.Number && a.TryGetUInt32(out var au) ? au : 0;
            string path = GameList.Str(e, "path");
            int mb = e.TryGetProperty("mb", out var m) && m.ValueKind == System.Text.Json.JsonValueKind.Number && m.TryGetInt32(out var mv) ? mv : 0;
            bool optional = e.TryGetProperty("optional", out var o) && o.ValueKind == System.Text.Json.JsonValueKind.True;
            // each kind needs its parameter, and a path stays inside the game's folder
            if (kind == SetupKinds.SteamApp && appId == 0) return null;
            if (kind == SetupKinds.GameFile && !IsGameFilePath(path)) return null;
            if (kind == SetupKinds.FreeDisk && mb is < 1 or > 1_000_000) return null;
            rules.Add(new SetupRule(kind, title, detail, appId, kind == SetupKinds.GameFile ? path : "", kind == SetupKinds.FreeDisk ? mb : 0, optional));
        }
        return rules;
    }

    /// <summary>A file in the game's folder: relative, forward slashes, 1 to 6 plain segments, never <c>..</c>, a drive or a hidden name.</summary>
    internal static bool IsGameFilePath(string s)
    {
        if (s.Length is 0 or > 160 || s.Contains("..") || s.Contains(':') || s.Contains('\\') || s.StartsWith('/') || s.EndsWith('/')) return false;
        var parts = s.Split('/');
        return parts.Length <= 6 && parts.All(p => p.Length > 0 && p[0] != '.' && p.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.' or ' '));
    }
}
