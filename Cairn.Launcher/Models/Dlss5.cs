// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.RegularExpressions;

namespace Cairn.Launcher.Models
{
    /// <summary>What the graphics card can do for DLSS 5: the architecture sets the cost (RTX 50 runs it as NVIDIA shipped it, 20 and 30 at about half the frame rate).</summary>
    public enum GpuClass { None, Rtx20, Rtx30, Rtx40, Rtx50 }

    /// <summary>What to ask of DLSS 5 Autopilot (https://github.com/Kizzuwatnaa/DLSS5-Autopilot, MIT): look only, install, or take it out again.</summary>
    public enum Dlss5Mode { Check, Install, Remove }

    /// <summary>
    /// Optional DLSS 5 for a game, through DLSS 5 Autopilot. Cairn does not carry it and does not port it: the launcher downloads its release (checked
    /// against the SHA256 the release publishes), and runs its command line on the game's folder. The tool then fetches every part from its own
    /// publisher. This is the pure half: reading the card's name, the command line, the release's file list and checksums.
    /// </summary>
    public static class Dlss5
    {
        public const string Repo = "Kizzuwatnaa/DLSS5-Autopilot";

        /// <summary>The route asked for: OptiScaler replaces the game's upscaler and runs the model over it, and its overlay opens on Insert.</summary>
        public const string Route = "optiscaler";

        /// <summary>The virtual key that opens OptiScaler's overlay (its own default).</summary>
        public const int OverlayVk = 0x2D;

        /// <summary>The id of the menu row the launcher adds to the game's menu while DLSS 5 is in the game.</summary>
        public const string MenuItemId = "dlss5_overlay";

        private static readonly Regex Geforce = new(@"\bRTX\s*([2-5])0\d\d\b", RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);
        private static readonly Regex Pro = new(@"\bRTX\s*A\d{3,4}\b", RegexOptions.IgnoreCase | RegexOptions.CultureInvariant);

        /// <summary>The class of one graphics adapter's name ("NVIDIA GeForce RTX 4070 Ti"); None for anything that is not an RTX 20 or newer.</summary>
        public static GpuClass ClassifyGpu(string? name)
        {
            if (string.IsNullOrWhiteSpace(name)) return GpuClass.None;
            if (name.Contains("RTX", StringComparison.OrdinalIgnoreCase) && name.Contains("Ada", StringComparison.OrdinalIgnoreCase)) return GpuClass.Rtx40;   // "RTX 2000 Ada" is a 40-class card
            if (Pro.IsMatch(name)) return GpuClass.Rtx30;                                                                                              // "RTX A4000": Ampere
            var m = Geforce.Match(name);
            return m.Success ? m.Groups[1].Value[0] switch { '2' => GpuClass.Rtx20, '3' => GpuClass.Rtx30, '4' => GpuClass.Rtx40, _ => GpuClass.Rtx50 } : GpuClass.None;
        }

        /// <summary>The best card of the PC (a laptop has its integrated one beside the NVIDIA one).</summary>
        public static GpuClass Best(IEnumerable<string> names) => names.Select(ClassifyGpu).DefaultIfEmpty(GpuClass.None).Max();

        public static bool Supported(GpuClass c) => c != GpuClass.None;

        /// <summary>One line for the settings window.</summary>
        public static string Describe(GpuClass c) => c switch
        {
            GpuClass.Rtx50 => "RTX 50 series: full speed.",
            GpuClass.Rtx40 => "RTX 40 series: moderate cost.",
            GpuClass.Rtx30 => "RTX 30 series: heavy, about half your frame rate at full model resolution (lower it in the overlay).",
            GpuClass.Rtx20 => "RTX 20 series: heavy, about half your frame rate at full model resolution (lower it in the overlay).",
            _ => "Needs an NVIDIA RTX card, 20 series or newer.",
        };

        /// <summary>The tool's command line for a game's folder. Nothing is a shell string: each argument is its own.</summary>
        public static IReadOnlyList<string> ToolArgs(string gameDir, Dlss5Mode mode) => mode switch
        {
            Dlss5Mode.Check => new[] { gameDir, "--check", "--route", Route },
            Dlss5Mode.Install => new[] { gameDir, "--route", Route },
            _ => new[] { gameDir, "--remove" },
        };

        /// <summary>The file name of the Windows build among a release's assets (<c>DLSS5-Autopilot-v2.0.8-win64.zip</c>), or null.</summary>
        public static string? PickZip(IEnumerable<string> assetNames) =>
            assetNames.FirstOrDefault(n => n.EndsWith("-win64.zip", StringComparison.OrdinalIgnoreCase) && n.StartsWith("DLSS5-Autopilot", StringComparison.OrdinalIgnoreCase));

        /// <summary>A SHA256SUMS file: "&lt;64 hex&gt;  &lt;name&gt;" per line (a "*" before the name marks binary mode). Name -> lower-case hex.</summary>
        public static IReadOnlyDictionary<string, string> ParseSums(string text)
        {
            var d = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (string raw in (text ?? "").Split('\n'))
            {
                string line = raw.Trim();
                if (line.Length < 66) continue;
                string hex = line[..64];
                if (!hex.All(Uri.IsHexDigit) || !char.IsWhiteSpace(line[64])) continue;
                string name = line[65..].Trim().TrimStart('*');
                if (name.Length > 0) d[name] = hex.ToLower(CultureInfo.InvariantCulture);
            }
            return d;
        }

        /// <summary>The downloaded file is the one the release published: the hash is the listed one, in any letter case.</summary>
        public static bool HashMatches(string? expectedHex, string actualHex) =>
            !string.IsNullOrEmpty(expectedHex) && expectedHex.Length == 64 && string.Equals(expectedHex, actualHex, StringComparison.OrdinalIgnoreCase);

        /// <summary>A release tag is a plain version ("v2.0.8"): it becomes a folder name, so nothing else is accepted.</summary>
        public static bool IsSafeTag(string? tag) => !string.IsNullOrEmpty(tag) && tag.Length <= 24 && Regex.IsMatch(tag, @"^v?\d+(\.\d+){1,3}$");

        /// <summary>
        /// The PowerShell script that starts the tool with its output in two files, waits for it and returns its exit code. Every value is a single-quoted
        /// literal (a quote inside one is doubled), and each tool argument is wrapped in double quotes for the tool's own command line.
        /// </summary>
        public static string StartScript(string exe, IReadOnlyList<string> toolArgs, string outFile, string errFile)
        {
            static string L(string s) => "'" + s.Replace("'", "''") + "'";
            string args = string.Join(",", toolArgs.Select(a => L("\"" + a.Replace("\"", "") + "\"")));
            return $"$p = Start-Process -FilePath {L(exe)} -ArgumentList @({args}) -Wait -PassThru -NoNewWindow -RedirectStandardOutput {L(outFile)} -RedirectStandardError {L(errFile)}; exit $p.ExitCode";
        }

        /// <summary>The menu row the launcher adds for DLSS 5. It is the launcher's own: the mod does not know about it.</summary>
        public static MenuItem MenuItem(bool inSubmenu, bool active = true) => active
            ? new(MenuItemId, "Graphics", MenuKind.Action, null, false, "",
                inSubmenu ? "Close the DLSS 5 overlay" : "Open the DLSS 5 overlay (OptiScaler)",
                "Opens DLSS 5 Neural Rendering, upscaler and frame generation settings in Cairn's menu.")
            : new(MenuItemId, "Graphics", MenuKind.Action, null, false, "Disabled",
                "DLSS 5 / OptiScaler",
                "Disabled: enable DLSS 5 (Neural Rendering) in Cairn Launcher settings.",
                Disabled: true);

        public sealed record SettingDef(
            string Id,
            string Group,
            string Section,
            string Key,
            MenuKind Kind,
            string Label,
            string Help,
            IReadOnlyList<(string IniValue, string Label)> Choices);

        public static readonly IReadOnlyList<SettingDef> Settings = new List<SettingDef>
        {
            new("dlss5_nr_enabled", "DLSS 5 Neural Rendering", "DlssNr", "Enabled", MenuKind.OnOff, "DLSS-NR", "DLSS 5 Neural Rendering: synthesises detail before frame generation. RTX cards only.", Array.Empty<(string, string)>()),
            new("dlss5_nr_scale", "DLSS 5 Neural Rendering", "DlssNr", "WorkingScale", MenuKind.Choice, "Model resolution", "Resolution the neural model runs at. Lower (0.75x or 0.67x) greatly improves FPS on RTX 20/30/40.", new[]
            {
                ("auto", "auto (1.00x)"), ("0.5", "0.50x"), ("0.67", "0.67x"), ("0.75", "0.75x"), ("1.0", "1.00x"), ("1.33", "1.33x")
            }),
            new("dlss5_nr_transfer", "DLSS 5 Neural Rendering", "DlssNr", "TransferStrength", MenuKind.Choice, "Transfer strength", "How far the frame moves toward the neural model's result (0.0 to 2.0).", new[]
            {
                ("auto", "auto (1.00)"), ("0.5", "0.50"), ("0.75", "0.75"), ("1.0", "1.00"), ("1.25", "1.25"), ("1.5", "1.50"), ("2.0", "2.00")
            }),
            new("dlss5_nr_colour", "DLSS 5 Neural Rendering", "DlssNr", "ColourStrength", MenuKind.Choice, "Colour strength", "1.0 brings the neural model's own colours; 0.0 keeps game's exact hue.", new[]
            {
                ("auto", "auto (1.00)"), ("0.0", "0.00"), ("0.25", "0.25"), ("0.5", "0.50"), ("0.75", "0.75"), ("1.0", "1.00")
            }),
            new("dlss5_nr_maxratio", "DLSS 5 Neural Rendering", "DlssNr", "MaxRatio", MenuKind.Choice, "Max brightness", "Caps pixel brightening multiplier to prevent over-bright artifacts.", new[]
            {
                ("auto", "auto (2.0x)"), ("1.5", "1.5x"), ("2.0", "2.0x"), ("2.5", "2.5x"), ("3.0", "3.0x"), ("4.0", "4.0x")
            }),
            new("dlss5_nr_debug", "DLSS 5 Neural Rendering", "DlssNr", "DebugView", MenuKind.Choice, "Debug view", "Visualise neural model input, raw output, or amplified difference.", new[]
            {
                ("auto", "Off"), ("1", "Input"), ("2", "Raw Output"), ("3", "Amplified Diff")
            }),
            new("dlss5_fg_enabled", "Frame Generation", "FrameGen", "Enabled", MenuKind.OnOff, "Frame generation", "Enables OptiScaler Frame Generation interpolation.", Array.Empty<(string, string)>()),
            new("dlss5_fg_input", "Frame Generation", "FrameGen", "FGInput", MenuKind.Choice, "FG Input", "Frame generation input provider (auto, dlssg, nvngxfg, fsrfg, upscaler).", new[]
            {
                ("auto", "auto"), ("dlssg", "dlssg"), ("nvngxfg", "nvngxfg"), ("fsrfg", "fsrfg"), ("upscaler", "upscaler")
            }),
            new("dlss5_fg_output", "Frame Generation", "FrameGen", "FGOutput", MenuKind.Choice, "FG Output", "Frame generation backend provider (auto, fsrfg, xefg, dlssg).", new[]
            {
                ("auto", "auto"), ("fsrfg", "fsrfg"), ("xefg", "xefg"), ("dlssg", "dlssg")
            }),
            new("dlss5_upscaler", "Upscaler", "Upscalers", "Dx12Upscaler", MenuKind.Choice, "Dx12 Upscaler", "Upscaler used for Dx12: auto, DLSS, FSR 2.2, XeSS, or FFX.", new[]
            {
                ("auto", "auto"), ("dlss", "dlss"), ("fsr22", "fsr22"), ("xess", "xess"), ("ffx", "ffx")
            }),
            new("dlss5_sharpness", "Enhancements", "Sharpness", "Sharpness", MenuKind.Choice, "Sharpness", "Post-processing sharpness filter strength.", new[]
            {
                ("auto", "auto"), ("off", "Off"), ("0.2", "0.2"), ("0.4", "0.4"), ("0.6", "0.6"), ("0.8", "0.8"), ("1.0", "1.0")
            }),
            new("dlss5_ratio", "Enhancements", "UpscaleRatio", "UpscaleRatioOverride", MenuKind.Choice, "Resolution scaling", "Resolution scale override (Quality, Balanced, Performance).", new[]
            {
                ("auto", "auto"), ("1.3", "Quality (1.3x)"), ("1.5", "Balanced (1.5x)"), ("1.7", "Perf (1.7x)"), ("2.0", "Ultra Perf (2.0x)")
            }),
        };

        /// <summary>Finds the OptiScaler.ini file in the game's folder or beside the game's executable.</summary>
        public static string? FindOptiScalerIni(string? gameRoot, System.Diagnostics.Process? game = null)
        {
            try
            {
                if (game is not null)
                {
                    try
                    {
                        string? exe = game.MainModule?.FileName;
                        if (!string.IsNullOrEmpty(exe))
                        {
                            string dir = System.IO.Path.GetDirectoryName(exe)!;
                            string cand = System.IO.Path.Combine(dir, "OptiScaler.ini");
                            if (System.IO.File.Exists(cand)) return cand;
                        }
                    }
                    catch { }
                }
                if (!string.IsNullOrEmpty(gameRoot) && System.IO.Directory.Exists(gameRoot))
                {
                    string cand = System.IO.Path.Combine(gameRoot, "OptiScaler.ini");
                    if (System.IO.File.Exists(cand)) return cand;
                    var found = System.IO.Directory.GetFiles(gameRoot, "OptiScaler.ini", System.IO.SearchOption.AllDirectories);
                    if (found.Length > 0) return found[0];
                }
            }
            catch { }
            return null;
        }

        /// <summary>Ensures OptiScaler's own shortcut key is disabled (-1) so pressing Insert never opens OptiScaler's native menu.</summary>
        public static void EnsureShortcutKeyDisabled(string? iniPath)
        {
            if (string.IsNullOrEmpty(iniPath) || !System.IO.File.Exists(iniPath)) return;
            string current = ReadIniValue(iniPath, "Menu", "ShortcutKey", "");
            if (current != "-1")
            {
                WriteIniValue(iniPath, "Menu", "ShortcutKey", "-1");
            }
        }

        /// <summary>Reads an INI value under a specific section, returning fallback if not found.</summary>
        public static string ReadIniValue(string? iniPath, string section, string key, string fallback = "auto")
        {
            if (string.IsNullOrEmpty(iniPath) || !System.IO.File.Exists(iniPath)) return fallback;
            try
            {
                var lines = System.IO.File.ReadAllLines(iniPath);
                bool inSection = false;
                foreach (string raw in lines)
                {
                    string trimmed = raw.Trim();
                    if (trimmed.StartsWith("[", StringComparison.Ordinal) && trimmed.EndsWith("]", StringComparison.Ordinal))
                    {
                        string sec = trimmed[1..^1].Trim();
                        inSection = sec.Equals(section, StringComparison.OrdinalIgnoreCase);
                        continue;
                    }
                    if (inSection)
                    {
                        if (trimmed.StartsWith(";", StringComparison.Ordinal) || trimmed.StartsWith("#", StringComparison.Ordinal)) continue;
                        int eq = trimmed.IndexOf('=');
                        if (eq > 0)
                        {
                            string k = trimmed[..eq].Trim();
                            if (k.Equals(key, StringComparison.OrdinalIgnoreCase))
                            {
                                return trimmed[(eq + 1)..].Trim();
                            }
                        }
                    }
                }
            }
            catch { }
            return fallback;
        }

        /// <summary>Writes or updates an INI value under a specific section, preserving existing lines and comments.</summary>
        public static void WriteIniValue(string? iniPath, string section, string key, string value)
        {
            if (string.IsNullOrEmpty(iniPath) || !System.IO.File.Exists(iniPath)) return;
            try
            {
                var lines = System.IO.File.ReadAllLines(iniPath).ToList();
                int sectionStart = -1;
                int sectionEnd = lines.Count;

                for (int i = 0; i < lines.Count; i++)
                {
                    string trimmed = lines[i].Trim();
                    if (trimmed.StartsWith("[", StringComparison.Ordinal) && trimmed.EndsWith("]", StringComparison.Ordinal))
                    {
                        string sec = trimmed[1..^1].Trim();
                        if (sectionStart >= 0)
                        {
                            sectionEnd = i;
                            break;
                        }
                        if (sec.Equals(section, StringComparison.OrdinalIgnoreCase))
                        {
                            sectionStart = i;
                        }
                    }
                }

                if (sectionStart < 0)
                {
                    lines.Add($"[{section}]");
                    lines.Add($"{key}={value}");
                }
                else
                {
                    int keyIndex = -1;
                    for (int i = sectionStart + 1; i < sectionEnd; i++)
                    {
                        string line = lines[i].TrimStart();
                        if (line.StartsWith(";", StringComparison.Ordinal) || line.StartsWith("#", StringComparison.Ordinal)) continue;
                        int eq = line.IndexOf('=');
                        if (eq > 0)
                        {
                            string k = line[..eq].Trim();
                            if (k.Equals(key, StringComparison.OrdinalIgnoreCase))
                            {
                                keyIndex = i;
                                break;
                            }
                        }
                    }

                    if (keyIndex >= 0)
                    {
                        lines[keyIndex] = $"{key}={value}";
                    }
                    else
                    {
                        lines.Insert(sectionStart + 1, $"{key}={value}");
                    }
                }

                System.IO.File.WriteAllLines(iniPath, lines);
            }
            catch { }
        }

        /// <summary>Builds the dedicated DLSS 5 / OptiScaler menu state for Cairn's in-game overlay.</summary>
        public static MenuState BuildSubmenu(string gameTitle, string? iniPath)
        {
            var items = new List<MenuItem>
            {
                new("dlss5_back", "DLSS 5", MenuKind.Action, null, false, "", "< Back to mod menu", "Return to the main mod menu.")
            };

            foreach (var def in Settings)
            {
                if (def.Kind == MenuKind.OnOff)
                {
                    string raw = ReadIniValue(iniPath, def.Section, def.Key, def.Section == "DlssNr" ? "true" : "false");
                    bool on = raw.Equals("true", StringComparison.OrdinalIgnoreCase);
                    items.Add(new MenuItem(def.Id, def.Group, MenuKind.OnOff, on, false, on ? "On" : "Off", def.Label, def.Help));
                }
                else if (def.Kind == MenuKind.Choice)
                {
                    string raw = ReadIniValue(iniPath, def.Section, def.Key, "auto");
                    var match = def.Choices.FirstOrDefault(c => c.IniValue.Equals(raw, StringComparison.OrdinalIgnoreCase));
                    string display = match.Label ?? (def.Choices.Count > 0 ? def.Choices[0].Label : raw);
                    items.Add(new MenuItem(def.Id, def.Group, MenuKind.Choice, null, false, display, def.Label, def.Help));
                }
            }

            return new MenuState(gameTitle, "DLSS 5 / OptiScaler", "", "Press Esc or select '< Back' to return", items);
        }

        /// <summary>Cycles or toggles a DLSS 5 / OptiScaler setting and saves it to OptiScaler.ini immediately.</summary>
        public static void StepSetting(string? iniPath, string id, int direction)
        {
            if (string.IsNullOrEmpty(iniPath)) return;
            var def = Settings.FirstOrDefault(s => s.Id == id);
            if (def is null) return;

            if (def.Kind == MenuKind.OnOff)
            {
                string raw = ReadIniValue(iniPath, def.Section, def.Key, def.Section == "DlssNr" ? "true" : "false");
                bool on = raw.Equals("true", StringComparison.OrdinalIgnoreCase);
                bool next = !on;
                WriteIniValue(iniPath, def.Section, def.Key, next ? "true" : "false");
            }
            else if (def.Kind == MenuKind.Choice && def.Choices.Count > 0)
            {
                string raw = ReadIniValue(iniPath, def.Section, def.Key, "auto");
                int idx = -1;
                for (int i = 0; i < def.Choices.Count; i++)
                {
                    if (def.Choices[i].IniValue.Equals(raw, StringComparison.OrdinalIgnoreCase))
                    {
                        idx = i;
                        break;
                    }
                }
                if (idx < 0) idx = 0;
                int nextIdx = (idx + direction + def.Choices.Count) % def.Choices.Count;
                WriteIniValue(iniPath, def.Section, def.Key, def.Choices[nextIdx].IniValue);
            }
        }
    }
}
