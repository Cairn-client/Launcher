// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
// Portions from the original project, marczukmichal/kcd2-multiplayer; its author keeps their copyright (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Cairn.Launcher.Components.Shared
{
    /// <summary>
    /// gained a real palette, pulled from the in-game dice overlay's
    /// own art direction ( SS2 -- aged parchment,
    /// oak, iron, candle-warm gold, tavern shadow) so the launcher and the
    /// in-game UI read as one product. Defaults here match wwwroot/css/site.css's
    /// `:root` block; Home.razor.cs pushes <see cref="ToCssVariables"/> over
    /// that default via JS interop on startup and on <see cref="Globals.OnStyleChanged"/>,
    /// so this class -- not the CSS file -- is the live source of truth.
    /// </summary>
    public class StyleProfile : BaseProfile
    {
        public string Id { get; set; } = Guid.NewGuid().ToString();

        public string ProfileName { get; set; } = string.Empty;
        public string FontFamily { get; set; } = "Plus Jakarta Sans";
        public string SpecialFontFamily { get; set; } = "Plus Jakarta Sans";
        public int FontSize { get; set; } = 14;
        public int FontWeight { get; set; } = 400;
        public string FontColor { get; set; } = "#1e1e1e";

        public string ColorGold { get; set; } = "#E8A35A";
        public string ColorGoldBright { get; set; } = "#F2BA80";
        public string ColorParchment { get; set; } = "#FBFBFA";
        public string ColorParchmentDim { get; set; } = "#B5BCC0";
        public string ColorOak { get; set; } = "#1D2125";
        public string ColorOakLit { get; set; } = "#2B3036";
        public string ColorIron { get; set; } = "#8A949B";
        public string ColorInk { get; set; } = "#0B0C0D";
        public string ColorShadow { get; set; } = "#0C0C0C";
        public string ColorBlood { get; set; } = "#C24D43";
        public string ColorDangerBright { get; set; } = "#F87171";
        public string ColorWarn { get; set; } = "#E8A35A";
        public string ColorDim { get; set; } = "#4B5258";
        public string ColorMuted { get; set; } = "#8A949B";
        public string ColorMutedDim { get; set; } = "#646D74";
        public string ColorOnline { get; set; } = "#6FCF8A";
        public string ColorTrack { get; set; } = "#060707";

        /// <summary>
        /// CSS custom-property name/value pairs for this profile. Applied to
        /// <c>document.documentElement</c> via the <c>applyCairnTheme</c> JS
        /// function (wwwroot/index.html), which just calls
        /// <c>style.setProperty</c> for each entry -- the shadow-rgb triple is
        /// derived here rather than stored, so a profile only ever has to
        /// carry one hex value per colour.
        /// </summary>
        public Dictionary<string, string> ToCssVariables() => new()
        {
            ["--cairn-gold"] = ColorGold,
            ["--cairn-gold-bright"] = ColorGoldBright,
            ["--cairn-parchment"] = ColorParchment,
            ["--cairn-parchment-dim"] = ColorParchmentDim,
            ["--cairn-oak"] = ColorOak,
            ["--cairn-oak-lit"] = ColorOakLit,
            ["--cairn-iron"] = ColorIron,
            ["--cairn-ink"] = ColorInk,
            ["--cairn-shadow"] = ColorShadow,
            ["--cairn-shadow-rgb"] = HexToRgbTriple(ColorShadow),
            ["--cairn-blood"] = ColorBlood,
            ["--cairn-danger-bright"] = ColorDangerBright,
            ["--cairn-warn"] = ColorWarn,
            ["--cairn-dim"] = ColorDim,
            ["--cairn-muted"] = ColorMuted,
            ["--cairn-muted-dim"] = ColorMutedDim,
            ["--cairn-online"] = ColorOnline,
            ["--cairn-track"] = ColorTrack,
            ["--cairn-font-family"] = $"'{FontFamily}', ui-sans-serif, system-ui, 'Segoe UI', sans-serif",
            ["--cairn-font-special"] = $"'{SpecialFontFamily}', ui-sans-serif, system-ui, 'Segoe UI', sans-serif",
        };

        private static string HexToRgbTriple(string hex)
        {
            hex = hex.TrimStart('#');
            byte r = Convert.ToByte(hex[..2], 16);
            byte g = Convert.ToByte(hex.Substring(2, 2), 16);
            byte b = Convert.ToByte(hex.Substring(4, 2), 16);
            return $"{r}, {g}, {b}";
        }

        public void SaveProfileToFile()
        {
            base.SaveProfileToFile(Globals.StylesProfilesDirectoryPath, $"{ProfileName}_{Id}");
        }
    }
}
