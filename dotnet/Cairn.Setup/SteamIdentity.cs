// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
namespace Cairn.Setup;

/// <summary>
/// Who is signed in to the local Steam client and which friends it lists, read from Steam's own files (no Steam API, so nothing shows
/// this player as "playing" anything): config\loginusers.vdf for the account and its name, userdata\&lt;account&gt;\config\localconfig.vdf for the
/// friends. Read only; the ids go to the master server's presence feature and nowhere else.
/// </summary>
public static class SteamIdentity
{
    /// <summary>The offset between a 32-bit Steam account id and the 64-bit Steam id of an individual account.</summary>
    public const ulong IndividualBase = 76561197960265728UL;

    public sealed record Local(string Id, string Name, IReadOnlyList<string> Friends);

    /// <summary>The signed-in account and its friends, or null when Steam's files do not say.</summary>
    public static Local? Read(string? steamRoot)
    {
        if (string.IsNullOrEmpty(steamRoot)) return null;
        try
        {
            string users = Path.Combine(steamRoot, "config", "loginusers.vdf");
            if (!File.Exists(users)) return null;
            var me = MostRecentUser(File.ReadAllText(users));
            if (me is null) return null;
            ulong id = ulong.Parse(me.Value.Id);
            ulong account = id - IndividualBase;
            var friends = new List<string>();
            string cfg = Path.Combine(steamRoot, "userdata", account.ToString(), "config", "localconfig.vdf");
            if (File.Exists(cfg)) friends = FriendIds(File.ReadAllText(cfg));
            return new Local(me.Value.Id, me.Value.Name, friends);
        }
        catch { return null; }
    }

    /// <summary>loginusers.vdf: the user with "MostRecent" "1" -> (steam id, persona name).</summary>
    public static (string Id, string Name)? MostRecentUser(string vdf)
    {
        var root = Vdf.Parse(vdf);
        if (root.Child("users") is not { } users) return null;
        foreach (var u in users.Children)
        {
            if (u.Value("MostRecent") != "1") continue;
            string name = u.Value("PersonaName") ?? "";
            if (ulong.TryParse(u.Key, out _) && name.Length > 0) return (u.Key, name);
        }
        return null;
    }

    /// <summary>localconfig.vdf: the account ids under "friends" -> 64-bit Steam ids (a section that is not a friend entry, such as a count, is skipped).</summary>
    public static List<string> FriendIds(string vdf)
    {
        var ids = new List<string>();
        var root = Vdf.Parse(vdf);
        var friends = root.Find("friends");
        if (friends is null) return ids;
        foreach (var f in friends.Children)
        {
            if (!uint.TryParse(f.Key, out uint account) || account == 0) continue;
            ids.Add((IndividualBase + account).ToString());
        }
        return ids;
    }

    /// <summary>A tiny reader of Valve's text KeyValues format: "key" "value" pairs and "key" { ... } sections.</summary>
    internal sealed class Vdf
    {
        public string Key = "";
        public string? Text;
        public List<Vdf> Children = new();

        public string? Value(string key) => Children.FirstOrDefault(c => c.Text is not null && string.Equals(c.Key, key, StringComparison.OrdinalIgnoreCase))?.Text;
        public Vdf? Child(string key) => Children.FirstOrDefault(c => c.Text is null && string.Equals(c.Key, key, StringComparison.OrdinalIgnoreCase));

        /// <summary>The first section with this name, at any depth.</summary>
        public Vdf? Find(string key)
        {
            if (Text is null && string.Equals(Key, key, StringComparison.OrdinalIgnoreCase)) return this;
            foreach (var c in Children) if (c.Find(key) is { } hit) return hit;
            return null;
        }

        public static Vdf Parse(string text)
        {
            int i = 0;
            var root = new Vdf { Key = "" };
            ReadSection(text, ref i, root);
            return root;
        }

        private static void ReadSection(string s, ref int i, Vdf into)
        {
            while (true)
            {
                string? key = Token(s, ref i);
                if (key is null || key == "}") return;
                Skip(s, ref i);
                if (i < s.Length && s[i] == '{')
                {
                    i++;
                    var child = new Vdf { Key = key };
                    ReadSection(s, ref i, child);
                    into.Children.Add(child);
                }
                else
                {
                    string? value = Token(s, ref i);
                    if (value is null) return;
                    into.Children.Add(new Vdf { Key = key, Text = value });
                }
            }
        }

        private static void Skip(string s, ref int i)
        {
            while (i < s.Length)
            {
                if (char.IsWhiteSpace(s[i])) { i++; continue; }
                if (s[i] == '/' && i + 1 < s.Length && s[i + 1] == '/') { while (i < s.Length && s[i] != '\n') i++; continue; }
                break;
            }
        }

        private static string? Token(string s, ref int i)
        {
            Skip(s, ref i);
            if (i >= s.Length) return null;
            if (s[i] == '}') { i++; return "}"; }
            if (s[i] == '{') return null;
            if (s[i] != '"')
            {
                int st = i;
                while (i < s.Length && !char.IsWhiteSpace(s[i]) && s[i] != '{' && s[i] != '}') i++;
                return s[st..i];
            }
            i++;
            var sb = new System.Text.StringBuilder();
            while (i < s.Length && s[i] != '"')
            {
                if (s[i] == '\\' && i + 1 < s.Length) { i++; sb.Append(s[i] switch { 'n' => '\n', 't' => '\t', var c => c }); i++; continue; }
                sb.Append(s[i++]);
            }
            i++;
            return sb.ToString();
        }
    }
}
