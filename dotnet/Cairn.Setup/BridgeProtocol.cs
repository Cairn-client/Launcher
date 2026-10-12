// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System.Globalization;
using System.Text;
using System.Text.Json;

namespace Cairn.Setup;

/// <summary>What a game's bridge says to the launcher.</summary>
public abstract record BridgeIn;

/// <summary>The first message of a connection: the protocol the bridge speaks and the game it belongs to (a plain id).</summary>
public sealed record BridgeHello(int Protocol, string Game) : BridgeIn;

/// <summary>One row of the in-game menu: the same shape the launcher already draws for every game.</summary>
public sealed record BridgeMenuItem(string Id, string Group, string Kind, bool? On, bool Locked, string Value, string Label, string Help, string Gate, bool Disabled);

/// <summary>The whole menu as the game's bridge describes it.</summary>
public sealed record BridgeMenu(string Title, string Blocked, string Note, IReadOnlyList<BridgeMenuItem> Items) : BridgeIn;

/// <summary>A notice to show (several lines joined by "^^") until it is hidden.</summary>
public sealed record BridgeNotice(bool Show, string Text) : BridgeIn;

/// <summary>One save a game has: a world, a character, a run. <c>Kind</c> is "world" or "character"; <c>When</c> is UTC, or null when the bridge does not know.</summary>
public sealed record BridgeSaveSlot(string Id, string Name, string Detail, string Kind, DateTime? When);

/// <summary>The saves the game has, as its bridge lists them (the whole list each time).</summary>
public sealed record BridgeSaves(IReadOnlyList<BridgeSaveSlot> Slots) : BridgeIn;

/// <summary>A spoken line to show for a few seconds.</summary>
public sealed record BridgeSubtitle(double Seconds, string Text) : BridgeIn;

/// <summary>
/// The conversation between the launcher and a game's bridge: a program that ships in the game's pack (its <c>companion</c>) and knows
/// how to talk to that one game. The launcher is the same for every game; a game that needs something new changes its bridge and its pack,
/// not the launcher. One JSON object per line, UTF-8, over a local named pipe whose name the launcher gives the companion in the
/// environment variable <see cref="PipeEnvironmentVariable"/>.
/// <code>
/// bridge to launcher   {"t":"hello","protocol":1,"game":"noita"}                 first, always
///                      {"t":"menu","title":"Mod menu","blocked":"","note":"","items":[{"id":"x","group":"g","kind":"onoff","on":true,"label":"..","help":".."}]}
///                      {"t":"notice","text":"Ann joined^^Press Insert"}  /  {"t":"notice_hide"}
///                      {"t":"subtitle","seconds":4,"text":".."}
/// launcher to bridge   {"t":"welcome","protocol":1}                               the answer to a hello this launcher speaks
///                      {"t":"attach"}   the launcher draws the menu now          {"t":"state"}   send the menu again
///                      {"t":"do","id":"x","verb":"enter"}   (verb: enter or nudge)   {"t":"bye","reason":".."}
/// </code>
/// Everything a bridge sends is checked here before the launcher uses it: sizes are capped, names are plain, an unknown message is ignored.
/// </summary>
public static class BridgeProtocol
{
    /// <summary>The newest version of these messages this launcher speaks; a bridge that needs a newer one is told so and the connection ends.</summary>
    public const int Version = 1;
    public const string PipeEnvironmentVariable = "CAIRN_BRIDGE_PIPE";
    public const int MaxLine = 16 * 1024;
    public const int MaxItems = 64;
    public const int MaxSaves = 100;

    /// <summary>Reads one line a bridge sent; null when it is not a message this launcher understands (damaged, too big, unknown, or unsafe).</summary>
    public static BridgeIn? Parse(string line)
    {
        if (line.Length is 0 or > MaxLine) return null;
        try
        {
            using var doc = JsonDocument.Parse(line);
            var r = doc.RootElement;
            if (r.ValueKind != JsonValueKind.Object) return null;
            string S(JsonElement e, string name, int max) =>
                e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? Clean(v.GetString() ?? "", max) : "";
            switch (S(r, "t", 20))
            {
                case "hello":
                    return r.TryGetProperty("protocol", out var p) && p.ValueKind == JsonValueKind.Number && p.TryGetInt32(out int proto) && proto >= 1 && IsId(S(r, "game", 40))
                        ? new BridgeHello(proto, S(r, "game", 40)) : null;
                case "menu":
                {
                    if (!r.TryGetProperty("items", out var arr) || arr.ValueKind != JsonValueKind.Array) return null;
                    var items = new List<BridgeMenuItem>();
                    foreach (var e in arr.EnumerateArray())
                    {
                        if (items.Count >= MaxItems) break;
                        if (e.ValueKind != JsonValueKind.Object) continue;
                        string id = S(e, "id", 40), kind = S(e, "kind", 10);
                        if (!IsId(id) || kind is not ("onoff" or "choice" or "key" or "action" or "info")) continue;   // a damaged row is left out, not the whole menu
                        bool? on = e.TryGetProperty("on", out var o) ? o.ValueKind switch { JsonValueKind.True => true, JsonValueKind.False => false, _ => null } : null;
                        bool locked = e.TryGetProperty("locked", out var l) && l.ValueKind == JsonValueKind.True;
                        bool disabled = e.TryGetProperty("disabled", out var d) && d.ValueKind == JsonValueKind.True;
                        items.Add(new BridgeMenuItem(id, S(e, "group", 40), kind, on, locked, S(e, "value", 60), S(e, "label", 80), S(e, "help", 200), S(e, "gate", 4), disabled));
                    }
                    return new BridgeMenu(S(r, "title", 60), S(r, "blocked", 60), S(r, "note", 200), items);
                }
                case "notice":
                {
                    string text = S(r, "text", 400);
                    return text.Length == 0 ? null : new BridgeNotice(true, text);
                }
                case "notice_hide":
                    return new BridgeNotice(false, "");
                case "saves":
                {
                    if (!r.TryGetProperty("slots", out var arr) || arr.ValueKind != JsonValueKind.Array) return null;
                    var slots = new List<BridgeSaveSlot>();
                    foreach (var e in arr.EnumerateArray())
                    {
                        if (slots.Count >= MaxSaves) break;
                        if (e.ValueKind != JsonValueKind.Object) continue;
                        string id = S(e, "id", 60), name = S(e, "name", 80), kind = S(e, "kind", 10);
                        if (!IsSaveId(id) || name.Length == 0 || kind is not ("world" or "character")) continue;
                        DateTime? when = DateTime.TryParse(S(e, "when", 40), CultureInfo.InvariantCulture, DateTimeStyles.AdjustToUniversal | DateTimeStyles.AssumeUniversal, out var w) ? w : null;
                        slots.Add(new BridgeSaveSlot(id, name, S(e, "detail", 120), kind, when));
                    }
                    return new BridgeSaves(slots);
                }
                case "subtitle":
                {
                    string text = S(r, "text", 400);
                    if (text.Length == 0 || !r.TryGetProperty("seconds", out var s) || s.ValueKind != JsonValueKind.Number || !s.TryGetDouble(out double secs)) return null;
                    return new BridgeSubtitle(Math.Clamp(secs, 1, 15), text);
                }
                default:
                    return null;
            }
        }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException) { return null; }
    }

    /// <summary>A plain id: letters, digits, dash and underscore (a game, a menu row). It goes into messages and is shown nowhere as markup.</summary>
    public static bool IsId(string s) => s.Length is > 0 and <= 40 && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_');

    /// <summary>A save's id as the game's bridge names it: it goes back in a message to delete it, so it is kept plain (letters, digits, <c>- _ .</c>).</summary>
    public static bool IsSaveId(string s) => s.Length is > 0 and <= 60 && !s.Contains("..") && s.All(c => char.IsAsciiLetterOrDigit(c) || c is '-' or '_' or '.');

    // text from a bridge is shown on screen: no control characters, no more than the cap (the "^^" line separator stays)
    private static string Clean(string s, int max)
    {
        var sb = new StringBuilder(Math.Min(s.Length, max));
        foreach (char c in s)
        {
            if (sb.Length >= max) break;
            if (c >= ' ' && c != '\u007f') sb.Append(c);
        }
        return sb.ToString().Trim();
    }

    public static string Welcome() => Line(w => { w.WriteString("t", "welcome"); w.WriteNumber("protocol", Version); });
    public static string Attach() => Line(w => w.WriteString("t", "attach"));
    public static string AskState() => Line(w => w.WriteString("t", "state"));
    /// <summary>"Send the list of saves again."</summary>
    public static string AskSaves() => Line(w => w.WriteString("t", "saves_ask"));
    /// <summary>"The player deletes this save"; null when the id is not a plain save id. The bridge decides what that means in its game and answers with the new list.</summary>
    public static string? DeleteSave(string id) => IsSaveId(id) ? Line(w => { w.WriteString("t", "save_delete"); w.WriteString("id", id); }) : null;
    public static string Bye(string reason) => Line(w => { w.WriteString("t", "bye"); w.WriteString("reason", Clean(reason, 120)); });

    /// <summary>"Do this to that row"; null when the id is not a plain id or the verb is not one the menu has.</summary>
    public static string? Do(string id, string verb) =>
        IsId(id) && verb is "enter" or "nudge" ? Line(w => { w.WriteString("t", "do"); w.WriteString("id", id); w.WriteString("verb", verb); }) : null;

    private static string Line(Action<Utf8JsonWriter> write)
    {
        using var ms = new MemoryStream();
        using (var w = new Utf8JsonWriter(ms))
        {
            w.WriteStartObject();
            write(w);
            w.WriteEndObject();
        }
        return Encoding.UTF8.GetString(ms.ToArray());
    }
}
