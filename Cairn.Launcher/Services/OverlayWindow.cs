// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using Cairn.Launcher.Models;
using Brush = System.Windows.Media.Brush;
using Brushes = System.Windows.Media.Brushes;
using Color = System.Windows.Media.Color;
using FontFamily = System.Windows.Media.FontFamily;
using Orientation = System.Windows.Controls.Orientation;
using Window = System.Windows.Window;
using MenuItem = Cairn.Launcher.Models.MenuItem;
using Size = System.Windows.Size;
using HorizontalAlignment = System.Windows.HorizontalAlignment;
using Rect = System.Windows.Rect;

namespace Cairn.Launcher.Services
{
    /// <summary>
    /// The shared in-game menu, drawn by the launcher over the game: a card on the right of the game's window, the same look for every
    /// game (only the items differ). It never takes the focus or the mouse (click-through, no activation), so the game keeps both.
    /// It lives on its own thread with its own message loop, which is also where the keyboard hook runs.
    /// </summary>
    internal sealed class OverlayWindow : IDisposable
    {
        [DllImport("user32.dll")] private static extern int GetWindowLong(IntPtr hWnd, int nIndex);
        [DllImport("user32.dll")] private static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);
        [DllImport("user32.dll")] private static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);

        private const int GWL_EXSTYLE = -20;
        private const int WS_EX_TRANSPARENT = 0x20, WS_EX_TOOLWINDOW = 0x80, WS_EX_NOACTIVATE = 0x08000000;
        private static readonly IntPtr HWND_TOPMOST = new(-1);
        private const uint SWP_NOSIZE = 0x1, SWP_NOMOVE = 0x2, SWP_NOACTIVATE = 0x10;

        private const double CardWidth = 430;

        private readonly Thread _thread;
        private readonly ManualResetEventSlim _ready = new();
        private Dispatcher? _dispatcher;
        private Window? _window;
        private Window? _subWindow;     // a partner's spoken line, along the bottom of the game
        private string _subText = "";
        private Window? _noticeWindow;  // a notice of the mod (a partner joining), at the top centre
        private string _noticeText = "";
        private double _subWidth;

        /// <summary>The overlay's own thread: where the keyboard hook has to be installed too.</summary>
        public Dispatcher? Dispatcher => _dispatcher;

        public OverlayWindow()
        {
            _thread = new Thread(Run) { IsBackground = true, Name = "Cairn in-game menu" };
            _thread.SetApartmentState(ApartmentState.STA);
            _thread.Start();
            _ready.Wait(TimeSpan.FromSeconds(5));
        }

        private void Run()
        {
            _dispatcher = Dispatcher.CurrentDispatcher;
            _window = new Window
            {
                WindowStyle = WindowStyle.None,
                AllowsTransparency = true,
                Background = Brushes.Transparent,
                ShowInTaskbar = false,
                ShowActivated = false,
                Topmost = true,
                ResizeMode = ResizeMode.NoResize,
                SizeToContent = SizeToContent.Height,
                Width = CardWidth,
                Focusable = false,
                Title = "Cairn menu",
            };
            _window.SourceInitialized += (_, _) =>
            {
                var h = new WindowInteropHelper(_window).Handle;
                SetWindowLong(h, GWL_EXSTYLE, GetWindowLong(h, GWL_EXSTYLE) | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW);
            };
            _subWindow = new Window
            {
                WindowStyle = WindowStyle.None,
                AllowsTransparency = true,
                Background = Brushes.Transparent,
                ShowInTaskbar = false,
                ShowActivated = false,
                Topmost = true,
                ResizeMode = ResizeMode.NoResize,
                SizeToContent = SizeToContent.WidthAndHeight,
                Focusable = false,
                Title = "Cairn subtitle",
            };
            _subWindow.SourceInitialized += (_, _) =>
            {
                var h = new WindowInteropHelper(_subWindow).Handle;
                SetWindowLong(h, GWL_EXSTYLE, GetWindowLong(h, GWL_EXSTYLE) | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW);
            };
            _noticeWindow = new Window
            {
                WindowStyle = WindowStyle.None,
                AllowsTransparency = true,
                Background = Brushes.Transparent,
                ShowInTaskbar = false,
                ShowActivated = false,
                Topmost = true,
                ResizeMode = ResizeMode.NoResize,
                SizeToContent = SizeToContent.WidthAndHeight,
                Focusable = false,
                Title = "Cairn notice",
            };
            _noticeWindow.SourceInitialized += (_, _) =>
            {
                var h = new WindowInteropHelper(_noticeWindow).Handle;
                SetWindowLong(h, GWL_EXSTYLE, GetWindowLong(h, GWL_EXSTYLE) | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW);
            };
            _ready.Set();
            Dispatcher.Run();
        }

        /// <summary>Draws the menu (or hides it). <paramref name="gameClient"/> is the game's client area in screen pixels.</summary>
        public void Render(bool visible, MenuState state, int selected, string blockedText, Int32Rect gameClient)
        {
            _dispatcher?.BeginInvoke(() =>
            {
              try
              {
                if (_window is null) return;
                if (!visible || gameClient.Width < 200 || gameClient.Height < 200) { if (_window.IsVisible) _window.Hide(); return; }

                var dpi = VisualTreeHelper.GetDpi(_window);
                double scale = dpi.DpiScaleX <= 0 ? 1 : dpi.DpiScaleX;
                _window.Content = Build(state, selected, blockedText);
                _window.Left = (gameClient.X + gameClient.Width) / scale - CardWidth - 28;
                _window.Top = gameClient.Y / scale + Math.Max(24, gameClient.Height / scale * 0.07);
                if (!_window.IsVisible) _window.Show();
                if (Environment.GetEnvironmentVariable("CAIRN_OVERLAY_DEBUG") == "1") System.IO.File.AppendAllText(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "cairn-overlay-debug.txt"), $"{DateTime.Now:HH:mm:ss.fff} show left={_window.Left} top={_window.Top} w={_window.ActualWidth} h={_window.ActualHeight} vis={_window.IsVisible} scale={scale} rect={gameClient}{Environment.NewLine}");
                var h = new WindowInteropHelper(_window).Handle;
                SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
              }
              catch (Exception ex)
              {
                // a drawing fault costs a frame, never the launcher
                try { System.IO.File.AppendAllText(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "cairn-overlay-error.txt"), DateTime.Now + " " + ex + Environment.NewLine); } catch (Exception) { }
              }
            });
        }

        /// <summary>
        /// Shows (or hides) a partner's spoken line as a subtitle, centred along the bottom of the game's window: the same dark card as the
        /// menu, readable on any scene. Like the menu it never takes the focus or the mouse.
        /// </summary>
        public void RenderSubtitle(bool visible, string text, Int32Rect gameClient)
        {
            _dispatcher?.BeginInvoke(() =>
            {
              try
              {
                if (_subWindow is null) return;
                if (!visible || text.Length == 0 || gameClient.Width < 200 || gameClient.Height < 200) { if (_subWindow.IsVisible) _subWindow.Hide(); return; }

                var dpi = VisualTreeHelper.GetDpi(_subWindow);
                double scale = dpi.DpiScaleX <= 0 ? 1 : dpi.DpiScaleX;
                double width = Math.Clamp(gameClient.Width / scale * 0.58, 480, 1180);
                double size = Math.Clamp(gameClient.Height / scale * 0.026, 17, 30);
                if (text != _subText || Math.Abs(width - _subWidth) > 1)
                {
                    _subText = text; _subWidth = width;
                    var line = new TextBlock
                    {
                        Text = text, FontFamily = Font, FontSize = size, Foreground = Parchment, TextWrapping = TextWrapping.Wrap,
                        TextAlignment = TextAlignment.Center, MaxWidth = width - 56, FontWeight = FontWeights.Medium,
                    };
                    _subWindow.Content = new Border
                    {
                        Background = Ink, BorderBrush = Oak, BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(12),
                        Padding = new Thickness(28, 12, 28, 14), Child = line,
                    };
                    _subWindow.UpdateLayout();
                }
                if (!_subWindow.IsVisible) _subWindow.Show();
                _subWindow.UpdateLayout();
                double w = _subWindow.ActualWidth > 0 ? _subWindow.ActualWidth : width, h2 = _subWindow.ActualHeight;
                _subWindow.Left = (gameClient.X + gameClient.Width / 2.0) / scale - w / 2;
                _subWindow.Top = (gameClient.Y + gameClient.Height) / scale - h2 - Math.Max(36, gameClient.Height / scale * 0.12);
                var hnd = new WindowInteropHelper(_subWindow).Handle;
                SetWindowPos(hnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
              }
              catch (Exception ex)
              {
                try { System.IO.File.AppendAllText(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "cairn-overlay-error.txt"), DateTime.Now + " subtitle " + ex + Environment.NewLine); } catch (Exception) { }
              }
            });
        }

        /// <summary>
        /// Shows (or hides) a notice of the mod at the top centre of the game's window, in the menu's own look: the brand line and the
        /// words on the same dark card. It takes neither the focus nor the mouse.
        /// </summary>
        public void RenderNotice(bool visible, string text, Int32Rect gameClient)
        {
            _dispatcher?.BeginInvoke(() =>
            {
              try
              {
                if (_noticeWindow is null) return;
                if (!visible || text.Length == 0 || gameClient.Width < 200 || gameClient.Height < 200) { if (_noticeWindow.IsVisible) _noticeWindow.Hide(); return; }

                var dpi = VisualTreeHelper.GetDpi(_noticeWindow);
                double scale = dpi.DpiScaleX <= 0 ? 1 : dpi.DpiScaleX;
                if (text != _noticeText)
                {
                    _noticeText = text;
                    var stack = new StackPanel { MinWidth = 300 };
                    stack.Children.Add(Pad(Text("CAIRN", 11, Gold, FontWeights.SemiBold, 3), 0, 0, 0, 6));
                    var line = Text(text, Math.Clamp(gameClient.Height / scale * 0.026, 17, 28), Parchment, FontWeights.Medium);
                    line.MaxWidth = 640;
                    stack.Children.Add(line);
                    _noticeWindow.Content = new Border
                    {
                        Background = Ink, BorderBrush = Oak, BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(14),
                        Padding = new Thickness(26, 16, 26, 18), Child = stack,
                    };
                    _noticeWindow.UpdateLayout();
                }
                if (!_noticeWindow.IsVisible) _noticeWindow.Show();
                _noticeWindow.UpdateLayout();
                double w = _noticeWindow.ActualWidth > 0 ? _noticeWindow.ActualWidth : 360;
                _noticeWindow.Left = (gameClient.X + gameClient.Width / 2.0) / scale - w / 2;
                _noticeWindow.Top = gameClient.Y / scale + Math.Max(40, gameClient.Height / scale * 0.1);
                var hnd = new WindowInteropHelper(_noticeWindow).Handle;
                SetWindowPos(hnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
              }
              catch (Exception ex)
              {
                try { System.IO.File.AppendAllText(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "cairn-overlay-error.txt"), DateTime.Now + " notice " + ex + Environment.NewLine); } catch (Exception) { }
              }
            });
        }

        /// <summary>Saves what the overlay shows as a picture (for looking at the design without a game).</summary>
        public void Snapshot(string path)
        {
            _dispatcher?.Invoke(() =>
            {
                if (_window?.Content is not FrameworkElement fe) return;
                fe.Measure(new Size(CardWidth, double.PositiveInfinity));
                fe.Arrange(new Rect(0, 0, CardWidth, fe.DesiredSize.Height));
                fe.UpdateLayout();
                var bmp = new System.Windows.Media.Imaging.RenderTargetBitmap((int)(CardWidth * 2), (int)(fe.DesiredSize.Height * 2), 192, 192, PixelFormats.Pbgra32);
                bmp.Render(fe);
                var enc = new System.Windows.Media.Imaging.PngBitmapEncoder();
                enc.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(bmp));
                using var fs = System.IO.File.Create(path);
                enc.Save(fs);
            });
        }

        // ---- the look ---------------------------------------------------------------------------------------------------

        private static readonly Brush Ink = Frozen(new SolidColorBrush(Color.FromArgb(0xF2, 0x15, 0x16, 0x1A)));
        private static readonly Brush Track = Frozen(new SolidColorBrush(Color.FromRgb(0x10, 0x11, 0x13)));
        private static readonly Brush Oak = Frozen(new SolidColorBrush(Color.FromRgb(0x3A, 0x3D, 0x44)));
        private static readonly Brush Gold = Frozen(new SolidColorBrush(Color.FromRgb(0xE8, 0xC8, 0x72)));
        private static readonly Brush Parchment = Frozen(new SolidColorBrush(Color.FromRgb(0xEC, 0xEC, 0xEE)));
        private static readonly Brush Dim = Frozen(new SolidColorBrush(Color.FromRgb(0xB6, 0xB8, 0xBE)));
        private static readonly Brush Muted = Frozen(new SolidColorBrush(Color.FromRgb(0x8A, 0x8E, 0x97)));
        private static readonly Brush Warn = Frozen(new SolidColorBrush(Color.FromRgb(0xD9, 0x94, 0x4E)));
        private static readonly Brush SelBg = Frozen(new SolidColorBrush(Color.FromArgb(0x26, 0xFF, 0xFF, 0xFF)));
        private static readonly FontFamily Font = new("Segoe UI");

        private static Brush Frozen(Brush b) { b.Freeze(); return b; }

        private static TextBlock Text(string s, double size, Brush brush, FontWeight? weight = null, double spacing = 0) =>
            new TextBlock
            {
                Text = s, FontFamily = Font, FontSize = size, Foreground = brush,
                FontWeight = weight ?? FontWeights.Normal, TextWrapping = TextWrapping.Wrap,
                TextTrimming = TextTrimming.None,
                VerticalAlignment = VerticalAlignment.Center,
            }.WithSpacing(spacing);

        private static UIElement Build(MenuState state, int selected, string blockedText)
        {
            var root = new StackPanel();

            // header: the brand, the game, the title
            var head = new DockPanel { Margin = new Thickness(20, 16, 20, 10), LastChildFill = false };
            var brand = Text("CAIRN", 12, Gold, FontWeights.SemiBold, 3);
            DockPanel.SetDock(brand, Dock.Left);
            head.Children.Add(brand);
            var title = Text(state.Title.Length > 0 ? state.Title : "Mod menu", 12, Dim);
            DockPanel.SetDock(title, Dock.Right);
            head.Children.Add(title);
            root.Children.Add(head);
            root.Children.Add(new Border { Height = 1, Background = Oak });

            if (blockedText.Length > 0)
            {
                root.Children.Add(Pad(Text(blockedText, 14, Warn), 20, 18, 20, 18));
            }
            else if (state.Items.Count == 0)
            {
                root.Children.Add(Pad(Text("Loading the menu...", 14, Muted), 20, 18, 20, 18));
            }
            else
            {
                var list = new StackPanel { Margin = new Thickness(10, 6, 10, 6) };
                string? group = null;
                for (int i = 0; i < state.Items.Count; i++)
                {
                    var it = state.Items[i];
                    if (it.Kind == MenuKind.Info) continue;   // information is shown under the list
                    if (it.Group != group)
                    {
                        group = it.Group;
                        list.Children.Add(Pad(Text(group.ToUpperInvariant(), 10.5, Muted, FontWeights.SemiBold, 2), 12, 12, 12, 4));
                    }
                    list.Children.Add(Row(it, i == selected));
                }
                root.Children.Add(list);

                var info = state.Items.Where(x => x.Kind == MenuKind.Info).ToList();
                if (info.Count > 0)
                {
                    var line = Text(string.Join("   ·   ", info.Select(x => x.Label + " " + x.Value)), 11, Muted);
                    root.Children.Add(Pad(line, 22, 2, 22, 8));
                }

                var sel = selected >= 0 && selected < state.Items.Count ? state.Items[selected] : null;
                string help = sel is null ? "" : sel.Locked ? "Set by the host: only the host can change this." : sel.Help;
                if (help.Length > 0)
                    root.Children.Add(new Border
                    {
                        Margin = new Thickness(12, 2, 12, 8), Padding = new Thickness(12, 9, 12, 9), CornerRadius = new CornerRadius(8),
                        Background = Track, Child = Text(help, 12, Dim),
                    });
            }

            if (state.Note.Length > 0)
                root.Children.Add(Pad(Text(state.Note, 12, Gold), 22, 0, 22, 8));

            root.Children.Add(new Border { Height = 1, Background = Oak, Margin = new Thickness(0, 4, 0, 0) });
            root.Children.Add(Footer());

            return new Border
            {
                Background = Ink, BorderBrush = Oak, BorderThickness = new Thickness(1), CornerRadius = new CornerRadius(14),
                Child = root, Margin = new Thickness(0),
            };
        }

        private static UIElement Row(MenuItem it, bool selected)
        {
            var grid = new Grid { Margin = new Thickness(0) };
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(3) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            var bar = new Border { Background = selected ? (it.Disabled ? Muted : Gold) : Brushes.Transparent, CornerRadius = new CornerRadius(2), Margin = new Thickness(0, 6, 0, 6) };
            Grid.SetColumn(bar, 0);
            grid.Children.Add(bar);

            var label = Text(it.Label, 14, it.Disabled ? Muted : (selected ? Parchment : Dim), selected && !it.Disabled ? FontWeights.SemiBold : FontWeights.Normal);
            label.Margin = new Thickness(12, 8, 8, 8);
            Grid.SetColumn(label, 1);
            grid.Children.Add(label);

            UIElement value = ValueFor(it, selected);
            if (value is FrameworkElement fe) fe.Margin = new Thickness(0, 0, 12, 0);
            Grid.SetColumn(value, 2);
            grid.Children.Add(value);

            return new Border { CornerRadius = new CornerRadius(8), Background = selected ? SelBg : Brushes.Transparent, Child = grid, Opacity = it.Disabled ? 0.55 : 1.0 };
        }

        private static UIElement ValueFor(MenuItem it, bool selected)
        {
            if (it.Disabled)
            {
                return Text(it.Value.Length > 0 ? it.Value : "Disabled", 11, Muted);
            }
            switch (it.Kind)
            {
                case MenuKind.OnOff:
                    return Toggle(it.On == true, it.Locked);
                case MenuKind.Action:
                    return selected ? KeyCap("Enter") : Text("", 11, Muted);
                default:
                {
                    // a choice or a key: its current word on a quiet chip
                    var chip = Chip(it.Value, it.Locked ? Muted : Parchment);
                    return it.Locked ? WithHostTag(chip) : chip;
                }
            }
        }

        private static readonly Brush TrackOn = Frozen(new SolidColorBrush(Color.FromRgb(0x3E, 0x8E, 0x58)));
        private static readonly Brush TrackOff = Frozen(new SolidColorBrush(Color.FromRgb(0x2A, 0x2C, 0x32)));
        private static readonly Brush ChipBg = Frozen(new SolidColorBrush(Color.FromRgb(0x24, 0x26, 0x2B)));
        private static readonly Brush KeyBg = Frozen(new SolidColorBrush(Color.FromRgb(0x2B, 0x2D, 0x33)));
        private static readonly Brush KeyEdge = Frozen(new SolidColorBrush(Color.FromRgb(0x14, 0x15, 0x18)));

        /// <summary>A switch: a track with a knob, green and to the right when on. The host's levers on a joiner are dimmed and tagged.</summary>
        private static UIElement Toggle(bool on, bool locked)
        {
            var knob = new Border
            {
                Width = 16, Height = 16, CornerRadius = new CornerRadius(8),
                Background = on ? Parchment : Muted,
                HorizontalAlignment = on ? HorizontalAlignment.Right : HorizontalAlignment.Left,
                VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(3, 0, 3, 0),
            };
            var track = new Border
            {
                Width = 42, Height = 22, CornerRadius = new CornerRadius(11),
                Background = on ? TrackOn : TrackOff, Child = knob, VerticalAlignment = VerticalAlignment.Center,
                Opacity = locked ? 0.55 : 1,
            };
            return locked ? WithHostTag(track) : track;
        }

        private static UIElement WithHostTag(UIElement inner)
        {
            var p = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
            var tag = Text("HOST", 10, Muted, FontWeights.SemiBold, 1);
            tag.Margin = new Thickness(0, 0, 8, 0);
            p.Children.Add(tag);
            p.Children.Add(inner);
            return p;
        }

        /// <summary>A value as a flat chip: no outline, a quiet fill.</summary>
        private static UIElement Chip(string text, Brush color) =>
            new Border
            {
                CornerRadius = new CornerRadius(7), Padding = new Thickness(11, 4, 11, 4), Background = ChipBg,
                Child = Text(text, 12.5, color, FontWeights.SemiBold), VerticalAlignment = VerticalAlignment.Center,
            };

        /// <summary>A key as a keyboard shows it: a flat cap with a darker lower edge.</summary>
        private static UIElement KeyCap(string label) =>
            new Border
            {
                MinWidth = 24, Height = 24, CornerRadius = new CornerRadius(6), Padding = new Thickness(6, 0, 6, 0),
                Background = KeyBg, BorderBrush = KeyEdge, BorderThickness = new Thickness(0, 0, 0, 2),
                Child = new TextBlock
                {
                    Text = label, FontFamily = Font, FontSize = 12, FontWeight = FontWeights.SemiBold, Foreground = Parchment,
                    HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center,
                },
                VerticalAlignment = VerticalAlignment.Center,
            };

        private static UIElement Footer()
        {
            var p = new WrapPanel { Margin = new Thickness(16, 10, 12, 12) };
            foreach (var (keys, what) in new[]
            {
                (new[] { "\u2191", "\u2193" }, "Choose"), (new[] { "\u2190", "\u2192" }, "Change"),
                (new[] { "Enter" }, "Select"), (new[] { "Esc" }, "Close"),
            })
            {
                var item = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(0, 0, 11, 4) };
                foreach (var k in keys)
                {
                    var cap = KeyCap(k);
                    if (cap is FrameworkElement fe) fe.Margin = new Thickness(0, 0, 4, 0);
                    item.Children.Add(cap);
                }
                var label = Text(what, 11.5, Muted);
                label.Margin = new Thickness(3, 0, 0, 0);
                item.Children.Add(label);
                p.Children.Add(item);
            }
            return p;
        }

        private static UIElement Pad(UIElement e, double l, double t, double r, double b)
        {
            if (e is FrameworkElement fe) fe.Margin = new Thickness(l, t, r, b);
            return e;
        }

        public void Dispose()
        {
            try { _dispatcher?.BeginInvokeShutdown(DispatcherPriority.Normal); } catch (Exception) { }
        }
    }

    internal static class TextExtensions
    {
        /// <summary>Letter spacing is not a TextBlock property: a small trick for the brand and the group names.</summary>
        public static TextBlock WithSpacing(this TextBlock t, double spacing)
        {
            if (spacing > 0 && !string.IsNullOrEmpty(t.Text))
                t.Text = string.Join(" ", t.Text.Select(c => c.ToString()));   // a hair space between letters
            return t;
        }
    }
}
