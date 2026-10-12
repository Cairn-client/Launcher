// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;
using System.Windows.Forms;

namespace Cairn.Launcher.Models
{
    /// <summary>The notification-area icon: open Cairn, exit, and a balloon when a friend starts hosting while the window is not in front.</summary>
    public static class TrayService
    {
        private static NotifyIcon? icon;

        /// <summary>Set by the window's focus events; a notice is only worth showing when the launcher is not the window in front.</summary>
        public static bool WindowFocused { get; set; } = true;

        public static void Start(string iconPath, Action open, Action exit)
        {
            try
            {
                var menu = new ContextMenuStrip();
                menu.Items.Add("Open Cairn", null, (_, _) => open());
                menu.Items.Add("Exit", null, (_, _) => exit());
                icon = new NotifyIcon
                {
                    Text = "Cairn Client",
                    Icon = System.IO.File.Exists(iconPath) ? new System.Drawing.Icon(iconPath) : System.Drawing.SystemIcons.Application,
                    ContextMenuStrip = menu,
                    Visible = true,
                };
                icon.MouseClick += (_, e) => { if (e.Button == MouseButtons.Left) open(); };
                icon.BalloonTipClicked += (_, _) => open();
            }
            catch (Exception ex) when (ex is InvalidOperationException or System.ComponentModel.Win32Exception or ArgumentException)
            {
                icon = null;   // no tray (a restricted shell): the launcher works without it
            }
        }

        public static void Notify(string title, string text)
        {
            try
            {
                if (icon is null || WindowFocused) return;
                icon.BalloonTipTitle = title;
                icon.BalloonTipText = text;
                icon.ShowBalloonTip(6000);
            }
            catch (Exception ex) when (ex is InvalidOperationException or ObjectDisposedException) { /* a missed notice is not an error */ }
        }

        public static void Stop()
        {
            try { if (icon is not null) { icon.Visible = false; icon.Dispose(); icon = null; } }
            catch (Exception ex) when (ex is InvalidOperationException or ObjectDisposedException) { }
        }

    }
}
