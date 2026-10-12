// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
namespace Cairn.Setup;

/// <summary>
/// Cairn: NPC lip sync. The regular game keeps its facial-animation data in <c>Data\Facials\Facials_&lt;language&gt;-part*.pak</c>;
/// the Modding Tools do not have it, and their engine says so in the log (<c>Facial pak can't be opened
/// 'data/facials/facials_english.pak'</c>) -- so NPC mouths never move. The same hard-link mechanism that WorkspaceLinker uses
/// for the game's other paks puts the retail files where the Modding Tools look for them: no disk space used, nothing
/// written into the retail game, and deleting the Modding Tools' <c>Data\Facials</c> folder undoes it. Only <c>Facials_*.pak</c>
/// files are ever touched.
/// </summary>
public static class FacialsLink
{
    public const string TempSuffix = ".cairn-link";

    public sealed record Pair(string Source, string Target);

    /// <param name="Total">How many facial paks the regular game has (0: nothing to link, or the game / Modding Tools are not found).</param>
    public sealed record Report(IReadOnlyList<Pair> Missing, int Total)
    {
        public bool Complete => Missing.Count == 0;
        public static readonly Report None = new(Array.Empty<Pair>(), 0);
    }

    public sealed record Result(int Linked, int Failed, string? Failure);

    public static string SourceDir(string gameRoot) => Path.Combine(gameRoot, "Data", "Facials");
    public static string TargetDir(string moddingToolsRoot) => Path.Combine(moddingToolsRoot, "Data", "Facials");

    /// <summary>The retail facial paks the Modding Tools do not have yet (absent, or a different size: a partial or older copy).</summary>
    public static Report Check(string? gameRoot, string? moddingToolsRoot)
    {
        if (string.IsNullOrWhiteSpace(gameRoot) || string.IsNullOrWhiteSpace(moddingToolsRoot)) return Report.None;
        try
        {
            var src = SourceDir(gameRoot);
            
            var missing = new List<Pair>();
            int total = 0;
            foreach (var f in Directory.Exists(src) ? Directory.GetFiles(src, "Facials_*.pak") : Array.Empty<string>())
            {
                total++;
                var target = Path.Combine(TargetDir(moddingToolsRoot), Path.GetFileName(f));
                var t = new FileInfo(target);
                if (!t.Exists || t.Length != new FileInfo(f).Length) missing.Add(new Pair(f, target));
            }
            // The spoken lines ride the same road: the English voices live in Localization\*-part*.pak and IPL_*.pak, the
            // sound banks in Data\Sounds.pak. A Modding Tools copy without them has lips that move and no voice. Only a file that
            // is absent is linked (a file that exists is the Modding Tools' own and is never replaced); the language's text paks
            // (*_xml.pak) are never touched.
            var loc = Path.Combine(gameRoot, "Localization");
            if (Directory.Exists(loc))
                foreach (var f in Directory.GetFiles(loc, "*.pak"))
                {
                    var name = Path.GetFileName(f);
                    if (name.EndsWith("_xml.pak", StringComparison.OrdinalIgnoreCase)) continue;
                    
                    var target = Path.Combine(moddingToolsRoot, "Localization", name);
                    if (!File.Exists(target)) { total++; missing.Add(new Pair(f, target)); }
                }
            var sounds = Path.Combine(gameRoot, "Data", "Sounds.pak");
            if (File.Exists(sounds))
            {
                
                var target = Path.Combine(moddingToolsRoot, "Data", "Sounds.pak");
                if (!File.Exists(target)) { total++; missing.Add(new Pair(sounds, target)); }
            }
            return new Report(missing, total);
        }
        catch (Exception) { return Report.None; }
    }

    /// <summary>
    /// A hard link per missing pak (made under a temporary name and moved over the target in one step, like the workspace's).
    /// Hard links need no permission; across drives they are impossible and the result says so (copying 6 GB is not a setup step).
    /// </summary>
    public static Result Link(Report report, ILinkPrimitives? primitives = null)
    {
        var io = primitives ?? Win32LinkPrimitives.Instance;
        int linked = 0, failed = 0;
        string? failure = null;
        foreach (var p in report.Missing)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(p.Target)!);
                var tmp = p.Target + TempSuffix;
                if (File.Exists(tmp)) File.Delete(tmp);
                int err = io.CreateHardLink(tmp, p.Source);
                if (err != 0)
                {
                    failed++;
                    failure ??= WorkspaceLinker.IsHardLinkImpossible(err)
                        ? "The regular game and the Modding Tools are on different drives (or a file system without hard links), so the lip-sync data cannot be shared without copying 6 GB."
                        : $"Windows refused a link (error {err}).";
                    continue;
                }
                var old = new FileInfo(p.Target);
                if (old.Exists && old.LinkTarget is null && old.IsReadOnly) old.IsReadOnly = false;
                err = io.MoveReplace(tmp, p.Target);
                if (err != 0)
                {
                    try { File.Delete(tmp); } catch { }
                    failed++;
                    failure ??= $"Could not put the link in place (error {err}); close the game and try again.";
                    continue;
                }
                linked++;
            }
            catch (Exception ex)
            {
                failed++;
                failure ??= ex.GetType().Name;
            }
        }
        return new Result(linked, failed, failure);
    }
}
