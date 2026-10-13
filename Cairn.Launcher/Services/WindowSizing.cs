// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports.
using Cairn.Launcher.Models;
using Photino.NET;
using Serilog;

namespace Cairn.Launcher.Services
{
    /// <summary>
    /// The window's smallest size. The pages are laid out for <see cref="WindowPrefs.MinWidth"/> x <see cref="WindowPrefs.MinHeight"/> pixels of page, and a screen pixel
    /// is not a page pixel: the page says how many it is (devicePixelRatio, which follows the monitor the window is on), and the minimum in screen pixels is that times
    /// the layout minimum. Asking the system for the scale instead was wrong on a PC whose monitors are not all at the same scale.
    /// </summary>
    public static class WindowSizing
    {
        public static PhotinoWindow? Window;
        private static double _last;

        /// <summary>From the page: its devicePixelRatio. Sets the minimum, and grows the window when it is already smaller than that.</summary>
        public static void Apply(double ratio)
        {
            if (Window is not { } w || ratio < 0.5 || ratio > 8 || Math.Abs(ratio - _last) < 0.001) return;
            _last = ratio;
            int minW = (int)Math.Ceiling(WindowPrefs.MinWidth * ratio), minH = (int)Math.Ceiling(WindowPrefs.MinHeight * ratio);
            try
            {
                w.SetMinSize(minW, minH);
                if (!w.Maximized && (w.Width < minW || w.Height < minH)) w.SetSize(Math.Max(w.Width, minW), Math.Max(w.Height, minH));
                Log.Information("window: scale {Ratio}, smallest {W}x{H}", ratio, minW, minH);
            }
            catch (Exception ex) { Log.Debug(ex, "window: the minimum size could not be set"); }
        }
    }
}
