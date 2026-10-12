// Copyright (C) 2026 the Kingdom Come: Together contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. This project's own code only; Kingdom Come: Deliverance II and its
// content belong to Warhorse Studios and PLAION. Unofficial, free, not affiliated with or endorsed by them.
// Portions from the original project, marczukmichal/kcd2-multiplayer; its author keeps their copyright (AUTHORS.md).
// See https://aka.ms/new-console-template for more information
using Photino.Blazor;
using Microsoft.Extensions.DependencyInjection;
using Cairn.Launcher;
using System.IO;
using System;
using Cairn.Launcher.Services;
using Microsoft.Extensions.Logging;
using Serilog;
using Cairn.Launcher.Components.Shared;
using Cairn.Launcher.Models;
using System.Linq;

class Program
{
    [STAThread]
    static void Main(string[] args)
    {
        if (args.Length > 1 && args[0] == "--menu-probe") { try { Cairn.Launcher.Services.MenuDemo.Probe(args[1]); } catch (Exception ex) { File.WriteAllText(Path.Combine(Path.GetTempPath(), "cairn-menu-probe-error.txt"), ex.ToString()); } return; }
        if (args.Contains("--menu-demo"))
        {
            try { Cairn.Launcher.Services.MenuDemo.Run(); }
            catch (Exception ex) { File.WriteAllText(Path.Combine(Path.GetTempPath(), "cairn-menu-demo-error.txt"), ex.ToString()); }
            return;
        }   // the in-game menu over the screen, no game needed
        Log.Logger = new LoggerConfiguration()
            .MinimumLevel.Debug()
            // combat visualisation (item J): Blazor's per-component render/init Debug lines
            // were ~80% of both real testers' log files -- every state change
            // logs a full render pass over the modal tree. The launcher's own
            // Debug logging stays; the framework's render narration goes.
            .MinimumLevel.Override("Microsoft.AspNetCore.Components", Serilog.Events.LogEventLevel.Information)
            .WriteTo.File(Path.Combine(Globals.AppFolder, "app.log"),
                rollingInterval: RollingInterval.Day,
                retainedFileCountLimit: 10)
            .CreateLogger();

        try
        {
            Log.Information("=== Start Cairn launcher {Version} (handshake {Handshake}) ===", Globals.CairnVersion, Globals.Version);

            // settings.json, custom_servers.json and favorites.json are read relative to the working
            // directory. A launcher started with another one (a tester's ran from C:\WINDOWS\system32: "settings.json
            // not written -- access to ...system32\settings.json.tmp is denied") read and wrote nothing of its own.
            // An installed launcher (Setup's manifest beside it) works in its own folder, where Setup put them.
            try
            {
                string own = Path.GetFullPath(AppContext.BaseDirectory).TrimEnd('\\', '/');
                string cwd = Path.GetFullPath(Environment.CurrentDirectory).TrimEnd('\\', '/');
                if (!string.Equals(own, cwd, StringComparison.OrdinalIgnoreCase) && File.Exists(Path.Combine(own, "install-manifest.txt")))
                {
                    Environment.CurrentDirectory = own;
                    Log.Information("MP-LAUNCH working folder was not the install folder; using the install folder for settings.json");
                }
            }
            catch (Exception ex) { Log.Warning("MP-LAUNCH working folder could not be set ({Kind})", ex.GetType().Name); }

            var appBuilder = PhotinoBlazorAppBuilder.CreateDefault(args);

            var baseDir = AppDomain.CurrentDomain.BaseDirectory;
            var wwwrootPath = Path.Combine(baseDir, "wwwroot");

            if (Directory.Exists(wwwrootPath))
            {
                appBuilder.Services.AddSingleton<Microsoft.Extensions.FileProviders.IFileProvider>(
                    new Microsoft.Extensions.FileProviders.PhysicalFileProvider(wwwrootPath));
            }

            appBuilder.Services.AddLogging(loggingBuilder =>
            {
                loggingBuilder.ClearProviders();
                loggingBuilder.AddSerilog(dispose: true);
            });

            appBuilder.Services.AddSingleton<Cairn.Launcher.Services.UiService>();
            appBuilder.Services.AddSingleton<Cairn.Launcher.Services.NetService>();
            appBuilder.RootComponents.Add<App>("#app");

            var app = appBuilder.Build();

            // the window opens where it was left (size, position, maximised); a size or screen that no longer fits falls back to maximised
            var windowPrefs = WindowPrefs.Load();
            bool restore = windowPrefs is { Maximized: false } wp
                && wp.IsUsable((x, y) => System.Windows.Forms.Screen.AllScreens.Any(s => s.WorkingArea.Contains(x, y)));
            app.MainWindow
                .SetTitle("Cairn Client")   // the name players see (the log line above keeps its old tag)
                .SetSize(restore ? windowPrefs!.Width : 1600, restore ? windowPrefs!.Height : 900)
                .SetMaximized(!restore)
                .SetResizable(true)
                .SetContextMenuEnabled(false);
            if (restore) app.MainWindow.SetLocation(new System.Drawing.Point(windowPrefs!.X, windowPrefs.Y));
            else app.MainWindow.Center();

            // remember changes, a moment after the last one (a drag fires many)
            var current = windowPrefs ?? new WindowPrefs();
            System.Threading.Timer? saveTimer = null;
            void SaveSoon() => (saveTimer ??= new System.Threading.Timer(_ => current.Save())).Change(600, System.Threading.Timeout.Infinite);
            app.MainWindow.RegisterSizeChangedHandler((_, size) => { if (!current.Maximized && size.Width > 0) { current.Width = size.Width; current.Height = size.Height; SaveSoon(); } });
            app.MainWindow.RegisterLocationChangedHandler((_, at) => { if (!current.Maximized && at.X > -30000) { current.X = at.X; current.Y = at.Y; SaveSoon(); } });
            app.MainWindow.RegisterMaximizedHandler((_, _) => { current.Maximized = true; SaveSoon(); });
            app.MainWindow.RegisterRestoredHandler((_, _) => { current.Maximized = app.MainWindow.Maximized; SaveSoon(); });   // also fires when it comes back from the taskbar


            // <ApplicationIcon> in the csproj only sets the .exe's own
            // file icon (Explorer, shortcuts, taskbar when not running).
            // Photino's actual window -- title bar, and the taskbar icon
            // while it's running -- is a separate runtime setting.
            //
            // this used to pass the bare relative "app.ico".
            // Photino's IconFile setter does probe AppContext.BaseDirectory
            // when the relative path is not found, but it then STORES the
            // relative string, and the later startup-parameter validation
            // re-checks File.Exists against the process working directory
            // and throws "WindowIconFile: app.ico cannot be found" (observed
            // on a tester's machine in 0.22.4, code-verified against
            // Photino.NET's PhotinoWindow IconFile setter). So the launcher
            // hard-crashed whenever it was started from any directory other
            // than its own. Resolve the path ourselves and never let a missing
            // icon take the whole launcher down -- an icon is decoration.
            var iconPath = Path.Combine(AppContext.BaseDirectory, "app.ico");
            if (File.Exists(iconPath))
                app.MainWindow.SetIconFile(iconPath);
            else
                Log.Warning("app.ico not found beside the launcher ({IconPath}); window icon skipped", iconPath);

            // the tray icon: open Cairn from it, exit, and a balloon when a friend starts hosting while this window is behind another
            app.MainWindow.RegisterFocusInHandler((_, _) => TrayService.WindowFocused = true);
            app.MainWindow.RegisterFocusOutHandler((_, _) => TrayService.WindowFocused = false);
            TrayService.Start(iconPath,
                open: () => { app.MainWindow.SetMinimized(false); app.MainWindow.SetTopMost(true); app.MainWindow.SetTopMost(false); },
                exit: () => { TrayService.Stop(); Environment.Exit(0); });

            AppDomain.CurrentDomain.UnhandledException += (sender, error) =>
            {
                var ex = error.ExceptionObject as Exception;
                Log.Fatal(ex, "Unexpected expection!");
                app.MainWindow.ShowMessage("Fatal Error", error.ExceptionObject.ToString());
            };

            app.Run();

            // combat visualisation (item I): the launcher never logged anything on a normal
            // exit, so horse adoption's "two silent sub-second crashes" (33 boot
            // lines, then nothing) were indistinguishable from a tester
            // simply closing the window fast -- which the timeline suggests
            // (four boots in 42 s while fighting the dead master server).
            // With this marker, a future boot that ends without it IS crash
            // evidence, not a guess.
            TrayService.Stop();
            Log.Information("=== Launcher exiting (clean) ===");
        }
        catch (Exception ex)
        {
            Log.Fatal(ex, "App encountered an error while launching!");
        }
        finally
        {
            Log.CloseAndFlush();
        }
    }
}