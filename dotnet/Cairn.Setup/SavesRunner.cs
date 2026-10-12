// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
namespace Cairn.Setup;

/// <summary>
/// A game's saves with the game closed: the pack's program (<c>optionsProgram</c>, the same one that does the options) answers
/// <c>saves list</c> with one bridge <c>saves</c> message and <c>saves delete &lt;id&gt;</c> with a <c>toast</c> and the new list (docs/BRIDGE.md). While the game
/// runs its bridge answers the same over its pipe, so the settings window shows the same saves either way.
/// </summary>
public static class SavesRunner
{
    public static readonly TimeSpan Timeout = TimeSpan.FromSeconds(30);

    /// <summary>The game's saves, or null when the program is not there or did not answer.</summary>
    public static async Task<IReadOnlyList<BridgeSaveSlot>?> ListAsync(string program, CancellationToken ct = default, TimeSpan? timeout = null)
    {
        var run = await OptionRunner.RunAsync(program, new[] { "saves", "list" }, timeout ?? Timeout, _ => { }, ct).ConfigureAwait(false);
        return (run.Lines ?? Array.Empty<string>()).Select(BridgeProtocol.Parse).OfType<BridgeSaves>().LastOrDefault()?.Slots;
    }

    /// <summary>Deletes one save the program listed. The message is what to tell the player; the list is the saves that are left (null when the program did not give it).</summary>
    public static async Task<(bool Ok, string Message, IReadOnlyList<BridgeSaveSlot>? Slots)> DeleteAsync(string program, string saveId, CancellationToken ct = default, TimeSpan? timeout = null)
    {
        if (!BridgeProtocol.IsSaveId(saveId)) return (false, "That save is not there any more.", null);
        var run = await OptionRunner.RunAsync(program, new[] { "saves", "delete", saveId }, timeout ?? Timeout, _ => { }, ct).ConfigureAwait(false);
        var messages = (run.Lines ?? Array.Empty<string>()).Select(BridgeProtocol.Parse).ToList();
        var toast = messages.OfType<BridgeToast>().LastOrDefault();
        var slots = messages.OfType<BridgeSaves>().LastOrDefault()?.Slots;
        if (toast is null) return (false, run.StartError ?? (run.TimedOut ? "It took too long and was stopped." : "The program did not answer."), slots);
        return (!toast.Error, toast.Text, slots);
    }

    /// <summary>Deletes every save of the game and every character and chest the mod kept (the program refuses while the game runs). The message is what to tell the player.</summary>
    public static async Task<(bool Ok, string Message)> DeleteAllAsync(string program, CancellationToken ct = default, TimeSpan? timeout = null)
    {
        var run = await OptionRunner.RunAsync(program, new[] { "saves", "delete-all" }, timeout ?? Timeout, _ => { }, ct).ConfigureAwait(false);
        var toast = (run.Lines ?? Array.Empty<string>()).Select(BridgeProtocol.Parse).OfType<BridgeToast>().LastOrDefault();
        if (toast is null) return (false, run.StartError ?? (run.TimedOut ? "It took too long and was stopped." : "The program did not answer."));
        return (!toast.Error, toast.Text);
    }
}
