// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
// Portions from the original project, marczukmichal/kcd2-multiplayer; its author keeps their copyright (AUTHORS.md).
using System;

namespace Cairn.Launcher.Services
{
    public class UiService
    {
        public event Action<string>? OnShowError;

        public void ShowError(string message)
        {
            OnShowError?.Invoke(message);
        }

        public void LogError(Exception? ex, string message)
        {
            Log.Error(ex, message);
            OnShowError?.Invoke(message);
        }
    }
}