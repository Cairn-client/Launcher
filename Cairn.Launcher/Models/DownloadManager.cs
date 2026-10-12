// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Cairn.Setup;

namespace Cairn.Launcher.Models
{
    public enum DownloadState { Queued, Downloading, Installing, Done, Failed, Cancelled }

    /// <summary>One mod download in the queue (a first download or an update).</summary>
    public sealed class DownloadItem
    {
        public DownloadItem(string gameId, string gameName, string? fromVersion, PackInfo info, Uri url)
        {
            GameId = gameId; GameName = gameName; FromVersion = fromVersion; Info = info; Url = url;
        }

        public string Id { get; } = Guid.NewGuid().ToString("N")[..8];
        public string GameId { get; }
        public string GameName { get; }
        /// <summary>The version on this PC before the download; null for a first download.</summary>
        public string? FromVersion { get; }
        public PackInfo Info { get; }
        public Uri Url { get; }
        public string ToVersion => Info.Version;
        public long Size => Info.Size;

        public DownloadState State { get; internal set; } = DownloadState.Queued;
        public double Progress { get; internal set; }
        public long BytesDone => (long)(Math.Clamp(Progress, 0, 1) * Size);
        public string Message { get; internal set; } = "";
        public DateTime? StartedUtc { get; internal set; }
        public DateTime? FinishedUtc { get; internal set; }

        public bool IsActive => State is DownloadState.Downloading or DownloadState.Installing;
        public bool IsWaiting => State == DownloadState.Queued;
        public bool IsFinished => State is DownloadState.Done or DownloadState.Failed or DownloadState.Cancelled;

        internal CancellationTokenSource Cts { get; } = new();
        internal TaskCompletionSource<bool> Completion { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        /// <summary>Completes when the item is finished; true when it was downloaded and installed.</summary>
        public Task<bool> Finished => Completion.Task;
    }

    /// <summary>
    /// The queue behind the footer: mod downloads run one after the other, in the order shown; a waiting one can be moved or taken
    /// out, the running one can be cancelled. The download itself is the runner's (Pages/Home.Packs.cs), so what a finished download
    /// must do (refresh the pack, put the mod in the game) stays where it was.
    /// </summary>
    public sealed class DownloadManager
    {
        /// <summary>Does one item: report progress as a fraction of the file, honour the token, say how it went.</summary>
        public delegate Task<(bool Ok, string Message)> Runner(DownloadItem item, IProgress<double> progress, CancellationToken ct);

        private readonly object gate = new();
        private readonly List<DownloadItem> items = new();
        private readonly Runner runner;
        private bool pumping;

        // speed: bytes of the running item over the last few seconds
        private readonly Queue<(long Ticks, long Bytes)> window = new();
        private long lastNotify;
        private long totalBytes;

        public DownloadManager(Runner runner) { this.runner = runner; }

        /// <summary>Something changed (state, order, progress); raised on whatever thread did it, progress at most ~7 times a second.</summary>
        public event Action? Changed;

        public IReadOnlyList<DownloadItem> Snapshot() { lock (gate) return items.ToList(); }
        public DownloadItem? Active { get { lock (gate) return items.FirstOrDefault(i => i.IsActive); } }
        public int WaitingCount { get { lock (gate) return items.Count(i => i.IsWaiting); } }
        public bool Busy { get { lock (gate) return items.Any(i => i.IsActive || i.IsWaiting); } }
        public DownloadItem? ItemOf(string gameId) { lock (gate) return items.FirstOrDefault(i => !i.IsFinished && i.GameId == gameId); }

        /// <summary>Bytes of this session's downloads so far.</summary>
        public long TotalBytes => Interlocked.Read(ref totalBytes);

        /// <summary>Bytes per second of the running download, averaged over the last ~3 s; 0 when nothing runs.</summary>
        public double SpeedBytesPerSecond
        {
            get
            {
                lock (gate)
                {
                    if (!items.Any(i => i.State == DownloadState.Downloading)) return 0;
                    long now = Stopwatch.GetTimestamp();
                    Trim(now);
                    if (window.Count < 2) return 0;
                    var first = window.Peek();
                    var last = window.Last();
                    double secs = (last.Ticks - first.Ticks) / (double)Stopwatch.Frequency;
                    // nothing arrived for a while: the rate has fallen, say so instead of repeating the last burst
                    double idle = (now - last.Ticks) / (double)Stopwatch.Frequency;
                    if (secs <= 0.05) return 0;
                    return (last.Bytes - first.Bytes) / Math.Max(secs, secs + idle);
                }
            }
        }

        /// <summary>Estimated seconds left for the running download, or null when unknown.</summary>
        public TimeSpan? Remaining
        {
            get
            {
                var a = Active;
                double v = SpeedBytesPerSecond;
                if (a is null || a.State != DownloadState.Downloading || v < 1) return null;
                return TimeSpan.FromSeconds((a.Size - a.BytesDone) / v);
            }
        }

        /// <summary>Adds a download at the end of the queue; a game that is already waiting or running keeps its place.</summary>
        public DownloadItem Enqueue(DownloadItem item)
        {
            lock (gate)
            {
                var same = items.FirstOrDefault(i => !i.IsFinished && i.GameId == item.GameId);
                if (same is not null) return same;
                items.Add(item);
            }
            Raise(force: true);
            Pump();
            return item;
        }

        /// <summary>Moves a waiting download one place up (-1) or down (+1) among the waiting ones.</summary>
        public void Move(string id, int delta)
        {
            lock (gate)
            {
                var waiting = items.Where(i => i.IsWaiting).ToList();
                int at = waiting.FindIndex(i => i.Id == id);
                int to = at + delta;
                if (at < 0 || to < 0 || to >= waiting.Count) return;
                Swap(waiting[at], waiting[to]);
            }
            Raise(force: true);
        }

        /// <summary>Puts a waiting download first in line (right after the running one).</summary>
        public void MoveFirst(string id)
        {
            lock (gate)
            {
                var item = items.FirstOrDefault(i => i.Id == id && i.IsWaiting);
                if (item is null) return;
                items.Remove(item);
                int firstWaiting = items.FindIndex(i => i.IsWaiting);
                items.Insert(firstWaiting < 0 ? items.Count : firstWaiting, item);
            }
            Raise(force: true);
        }

        /// <summary>Takes a waiting download out of the queue, or stops the running one.</summary>
        public void Cancel(string id)
        {
            DownloadItem? running = null;
            lock (gate)
            {
                var item = items.FirstOrDefault(i => i.Id == id);
                if (item is null || item.IsFinished) return;
                if (item.IsWaiting)
                {
                    item.State = DownloadState.Cancelled;
                    item.Message = "Cancelled";
                    item.FinishedUtc = DateTime.UtcNow;
                    item.Completion.TrySetResult(false);
                }
                else running = item;
            }
            running?.Cts.Cancel();
            Raise(force: true);
        }

        /// <summary>Forgets the finished ones (done, failed, cancelled).</summary>
        public void ClearFinished()
        {
            lock (gate) items.RemoveAll(i => i.IsFinished);
            Raise(force: true);
        }

        /// <summary>The installing step of the running item (the download is complete, the mod is being put in the game).</summary>
        public void MarkInstalling(DownloadItem item)
        {
            item.State = DownloadState.Installing;
            item.Progress = 1;
            Raise(force: true);
        }

        private void Swap(DownloadItem a, DownloadItem b)
        {
            int i = items.IndexOf(a), j = items.IndexOf(b);
            items[i] = b; items[j] = a;
        }

        private void Pump()
        {
            lock (gate)
            {
                if (pumping) return;
                pumping = true;
            }
            _ = Task.Run(async () =>
            {
                while (true)
                {
                    DownloadItem? next;
                    lock (gate)
                    {
                        next = items.FirstOrDefault(i => i.IsWaiting);
                        if (next is null) { pumping = false; window.Clear(); break; }
                        next.State = DownloadState.Downloading;
                        next.StartedUtc = DateTime.UtcNow;
                        window.Clear();
                    }
                    Raise(force: true);
                    await RunOne(next);
                }
                Raise(force: true);
            });
        }

        private async Task RunOne(DownloadItem item)
        {
            long lastBytes = 0;
            var progress = new Progress<double>(p =>
            {
                item.Progress = p;
                long bytes = item.BytesDone;
                long delta = bytes - lastBytes;
                if (delta > 0) { Interlocked.Add(ref totalBytes, delta); lastBytes = bytes; }
                lock (gate)
                {
                    long now = Stopwatch.GetTimestamp();
                    // a sample about every 250 ms is enough for a speed over three seconds
                    if (window.Count == 0 || (now - window.Last().Ticks) * 4 >= Stopwatch.Frequency) window.Enqueue((now, bytes));
                    Trim(now);
                }
                Raise(force: false);
            });
            bool ok = false;
            try
            {
                var (success, message) = await runner(item, progress, item.Cts.Token).ConfigureAwait(false);
                ok = success;
                item.State = success ? DownloadState.Done : DownloadState.Failed;
                item.Message = message;
            }
            catch (OperationCanceledException)
            {
                item.State = DownloadState.Cancelled;
                item.Message = "Cancelled";
            }
            catch (Exception ex)
            {
                item.State = DownloadState.Failed;
                item.Message = ex.Message;
            }
            item.FinishedUtc = DateTime.UtcNow;
            if (ok) item.Progress = 1;
            item.Completion.TrySetResult(ok);
        }

        private void Trim(long now)
        {
            while (window.Count > 1 && (now - window.Peek().Ticks) > 3 * Stopwatch.Frequency) window.Dequeue();
        }

        private void Raise(bool force)
        {
            long now = Environment.TickCount64;
            if (!force && now - Interlocked.Read(ref lastNotify) < 150) return;
            Interlocked.Exchange(ref lastNotify, now);
            Changed?.Invoke();
        }
    }
}
