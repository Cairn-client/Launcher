// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The download queue: one pack at a time, in order, with the speed over the last few seconds. The window reads a snapshot of it; a worker
//! thread runs the queue and calls the runner (which downloads and installs) for each item.

use cairn_setup::packs::PackInfo;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Queued,
    Downloading,
    Installing,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub id: String,
    pub game_id: String,
    pub game_name: String,
    pub from_version: Option<String>,
    pub info: PackInfo,
    pub url: String,
    /// Where an external mod is placed once it is downloaded: the game's folder.
    pub place_in: Option<std::path::PathBuf>,
    pub state: DownloadState,
    pub progress: f64,
    pub message: String,
    pub started: Option<Instant>,
    pub finished: Option<Instant>,
}

impl DownloadItem {
    pub fn new(game_id: &str, game_name: &str, from_version: Option<String>, info: PackInfo, url: String) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: format!("d{}", NEXT.fetch_add(1, Ordering::Relaxed)),
            game_id: game_id.to_string(),
            game_name: game_name.to_string(),
            from_version,
            info,
            url,
            place_in: None,
            state: DownloadState::Queued,
            progress: 0.0,
            message: String::new(),
            started: None,
            finished: None,
        }
    }
    pub fn to_version(&self) -> &str {
        &self.info.version
    }
    pub fn size(&self) -> i64 {
        self.info.size
    }
    pub fn bytes_done(&self) -> i64 {
        (self.progress.clamp(0.0, 1.0) * self.info.size as f64) as i64
    }
    pub fn is_active(&self) -> bool {
        matches!(self.state, DownloadState::Downloading | DownloadState::Installing)
    }
    pub fn is_waiting(&self) -> bool {
        self.state == DownloadState::Queued
    }
    pub fn is_finished(&self) -> bool {
        matches!(self.state, DownloadState::Done | DownloadState::Failed | DownloadState::Cancelled)
    }
}

/// What the runner is given to report progress and to notice that the player cancelled.
pub struct RunContext {
    manager: Arc<Inner>,
    pub item_id: String,
    pub cancel: Arc<AtomicBool>,
}

impl RunContext {
    pub fn progress(&self, p: f64) {
        self.manager.set_progress(&self.item_id, p);
    }
    /// The download is done and the mod is being put in the game.
    pub fn installing(&self) {
        self.manager.set_installing(&self.item_id);
    }
}

/// Downloads and installs one item: `(ok, message)`.
pub type Runner = Arc<dyn Fn(&DownloadItem, &RunContext) -> (bool, String) + Send + Sync>;

struct State {
    items: Vec<DownloadItem>,
    cancels: Vec<(String, Arc<AtomicBool>)>,
    /// (when, bytes) samples of the running item.
    window: VecDeque<(Instant, i64)>,
    total_bytes: i64,
    stop: bool,
}

struct Inner {
    state: Mutex<State>,
    wake: Condvar,
    on_change: Box<dyn Fn() + Send + Sync>,
}

#[derive(Clone)]
pub struct DownloadManager {
    inner: Arc<Inner>,
}

impl DownloadManager {
    /// `on_change` is called when something the window shows changed (it asks for a repaint).
    pub fn new(runner: Runner, on_change: impl Fn() + Send + Sync + 'static) -> Self {
        let inner = Arc::new(Inner {
            state: Mutex::new(State {
                items: vec![],
                cancels: vec![],
                window: VecDeque::new(),
                total_bytes: 0,
                stop: false,
            }),
            wake: Condvar::new(),
            on_change: Box::new(on_change),
        });
        let worker = inner.clone();
        std::thread::spawn(move || worker.run(runner));
        Self { inner }
    }

    pub fn snapshot(&self) -> Vec<DownloadItem> {
        self.inner.state.lock().unwrap().items.clone()
    }

    pub fn active(&self) -> Option<DownloadItem> {
        self.inner.state.lock().unwrap().items.iter().find(|i| i.is_active()).cloned()
    }

    pub fn waiting_count(&self) -> usize {
        self.inner.state.lock().unwrap().items.iter().filter(|i| i.is_waiting()).count()
    }

    pub fn busy(&self) -> bool {
        self.inner
            .state
            .lock()
            .unwrap()
            .items
            .iter()
            .any(|i| i.is_active() || i.is_waiting())
    }

    /// The unfinished item of a game, if any.
    pub fn item_of(&self, game_id: &str) -> Option<DownloadItem> {
        self.inner
            .state
            .lock()
            .unwrap()
            .items
            .iter()
            .find(|i| !i.is_finished() && i.game_id.eq_ignore_ascii_case(game_id))
            .cloned()
    }

    pub fn item(&self, id: &str) -> Option<DownloadItem> {
        self.inner.state.lock().unwrap().items.iter().find(|i| i.id == id).cloned()
    }

    pub fn total_bytes(&self) -> i64 {
        self.inner.state.lock().unwrap().total_bytes
    }

    /// Bytes per second over the last few seconds; 0 when nothing is downloading.
    pub fn speed(&self) -> f64 {
        let s = self.inner.state.lock().unwrap();
        if !s.items.iter().any(|i| i.state == DownloadState::Downloading) || s.window.len() < 2 {
            return 0.0;
        }
        let (first, last) = (s.window.front().unwrap(), s.window.back().unwrap());
        let secs = last.0.duration_since(first.0).as_secs_f64();
        if secs <= 0.05 {
            return 0.0;
        }
        // nothing arrived for a while: the rate has fallen, say so instead of repeating the last burst
        let idle = last.0.elapsed().as_secs_f64();
        (last.1 - first.1) as f64 / secs.max(secs + idle)
    }

    /// Time left for the running download at the current speed.
    pub fn remaining(&self) -> Option<Duration> {
        let a = self.active().filter(|a| a.state == DownloadState::Downloading)?;
        let v = self.speed();
        (v >= 1.0).then(|| Duration::from_secs_f64(((a.size() - a.bytes_done()) as f64 / v).max(0.0)))
    }

    /// Adds an item; a game that already has one in the queue keeps that one. Returns the item's id.
    pub fn enqueue(&self, item: DownloadItem) -> String {
        let id = {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(same) = s
                .items
                .iter()
                .find(|i| !i.is_finished() && i.game_id.eq_ignore_ascii_case(&item.game_id))
            {
                return same.id.clone();
            }
            let id = item.id.clone();
            s.cancels.push((id.clone(), Arc::new(AtomicBool::new(false))));
            s.items.push(item);
            id
        };
        self.inner.changed();
        self.inner.wake.notify_all();
        id
    }

    /// Moves a waiting item up or down among the waiting ones.
    pub fn move_by(&self, id: &str, delta: isize) {
        {
            let mut s = self.inner.state.lock().unwrap();
            let waiting: Vec<usize> = s.items.iter().enumerate().filter(|(_, i)| i.is_waiting()).map(|(n, _)| n).collect();
            let Some(at) = waiting.iter().position(|n| s.items[*n].id == id) else {
                return;
            };
            let to = at as isize + delta;
            if to < 0 || to as usize >= waiting.len() {
                return;
            }
            s.items.swap(waiting[at], waiting[to as usize]);
        }
        self.inner.changed();
    }

    /// Makes a waiting item the next one.
    pub fn move_first(&self, id: &str) {
        {
            let mut s = self.inner.state.lock().unwrap();
            let Some(at) = s.items.iter().position(|i| i.id == id && i.is_waiting()) else {
                return;
            };
            let item = s.items.remove(at);
            let first = s.items.iter().position(|i| i.is_waiting()).unwrap_or(s.items.len());
            s.items.insert(first, item);
        }
        self.inner.changed();
    }

    /// Cancels a waiting item at once, or the running one at its next chunk.
    pub fn cancel(&self, id: &str) {
        {
            let mut s = self.inner.state.lock().unwrap();
            let Some(item) = s.items.iter_mut().find(|i| i.id == id) else {
                return;
            };
            if item.is_finished() {
                return;
            }
            if item.is_waiting() {
                item.state = DownloadState::Cancelled;
                item.message = "Cancelled".into();
                item.finished = Some(Instant::now());
            } else if let Some((_, flag)) = s.cancels.iter().find(|(i, _)| i == id) {
                flag.store(true, Ordering::Relaxed);
            }
        }
        self.inner.changed();
    }

    pub fn cancel_waiting(&self) {
        let ids: Vec<String> = self.snapshot().into_iter().filter(|i| i.is_waiting()).map(|i| i.id).collect();
        for id in ids {
            self.cancel(&id);
        }
    }

    pub fn clear_finished(&self) {
        {
            let mut s = self.inner.state.lock().unwrap();
            let gone: Vec<String> = s.items.iter().filter(|i| i.is_finished()).map(|i| i.id.clone()).collect();
            s.items.retain(|i| !i.is_finished());
            s.cancels.retain(|(id, _)| !gone.contains(id));
        }
        self.inner.changed();
    }
}

impl Drop for DownloadManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) <= 2 {
            self.inner.state.lock().unwrap().stop = true;
            self.inner.wake.notify_all();
        }
    }
}

impl Inner {
    fn changed(&self) {
        (self.on_change)();
    }

    fn set_progress(&self, id: &str, p: f64) {
        let bytes = {
            let mut s = self.state.lock().unwrap();
            let Some(item) = s.items.iter_mut().find(|i| i.id == id) else {
                return;
            };
            item.progress = p.clamp(0.0, 1.0);
            let bytes = item.bytes_done();
            let now = Instant::now();
            // a sample about every 250 ms is enough for a speed over three seconds
            if s.window
                .back()
                .is_none_or(|(t, _)| now.duration_since(*t) >= Duration::from_millis(250))
            {
                s.window.push_back((now, bytes));
            }
            while s.window.len() > 1 && now.duration_since(s.window.front().unwrap().0) > Duration::from_secs(3) {
                s.window.pop_front();
            }
            bytes
        };
        let _ = bytes;
        self.changed();
    }

    fn set_installing(&self, id: &str) {
        {
            let mut s = self.state.lock().unwrap();
            if let Some(item) = s.items.iter_mut().find(|i| i.id == id) {
                item.state = DownloadState::Installing;
                item.progress = 1.0;
            }
        }
        self.changed();
    }

    fn run(self: Arc<Self>, runner: Runner) {
        loop {
            let (item, cancel) = {
                let mut s = self.state.lock().unwrap();
                loop {
                    if s.stop {
                        return;
                    }
                    if let Some(n) = s.items.iter().position(|i| i.is_waiting()) {
                        s.items[n].state = DownloadState::Downloading;
                        s.items[n].started = Some(Instant::now());
                        s.window.clear();
                        let id = s.items[n].id.clone();
                        let cancel = s.cancels.iter().find(|(i, _)| *i == id).map(|(_, c)| c.clone()).unwrap_or_default();
                        break (s.items[n].clone(), cancel);
                    }
                    s = self.wake.wait(s).unwrap();
                }
            };
            self.changed();
            let ctx = RunContext {
                manager: self.clone(),
                item_id: item.id.clone(),
                cancel: cancel.clone(),
            };
            let before = item.bytes_done();
            let _ = before;
            let (ok, message) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner(&item, &ctx)))
                .unwrap_or((false, "the download failed unexpectedly".into()));
            {
                let mut s = self.state.lock().unwrap();
                let cancelled = cancel.load(Ordering::Relaxed);
                if let Some(it) = s.items.iter_mut().find(|i| i.id == item.id) {
                    it.state = if cancelled {
                        DownloadState::Cancelled
                    } else if ok {
                        DownloadState::Done
                    } else {
                        DownloadState::Failed
                    };
                    it.message = if cancelled { "Cancelled".into() } else { message };
                    it.finished = Some(Instant::now());
                    if ok && !cancelled {
                        it.progress = 1.0;
                    }
                    let done = it.bytes_done();
                    s.total_bytes += done;
                }
                s.window.clear();
            }
            self.changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn pack(id: &str, size: i64) -> PackInfo {
        PackInfo {
            id: id.into(),
            game: id.into(),
            version: "1.0.0".into(),
            file: format!("{id}.zip"),
            sha256: "a".repeat(64),
            size,
            launcher_min: String::new(),
            upstream: String::new(),
            depends_on: vec![],
            kind: "mod".into(),
        }
    }
    fn item(id: &str) -> DownloadItem {
        DownloadItem::new(id, id, None, pack(id, 1000), format!("https://x.test/{id}.zip"))
    }
    fn wait_for(m: &DownloadManager, mut done: impl FnMut(&DownloadManager) -> bool) {
        let start = Instant::now();
        while !done(m) {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn items_run_one_at_a_time_in_order() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let o = order.clone();
        let m = DownloadManager::new(
            Arc::new(move |it, ctx| {
                o.lock().unwrap().push(format!("start {}", it.game_id));
                ctx.progress(0.5);
                ctx.progress(1.0);
                o.lock().unwrap().push(format!("end {}", it.game_id));
                (true, "ok".into())
            }),
            || {},
        );
        m.enqueue(item("a"));
        m.enqueue(item("b"));
        wait_for(&m, |m| !m.busy());
        assert_eq!(*order.lock().unwrap(), vec!["start a", "end a", "start b", "end b"]);
        let all = m.snapshot();
        assert!(all.iter().all(|i| i.state == DownloadState::Done && i.progress == 1.0));
        assert_eq!(m.total_bytes(), 2000);
        m.clear_finished();
        assert!(m.snapshot().is_empty());
    }

    #[test]
    fn a_game_is_queued_once() {
        let gate = Arc::new(AtomicBool::new(false));
        let g = gate.clone();
        let m = DownloadManager::new(
            Arc::new(move |_, _| {
                while !g.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                (true, String::new())
            }),
            || {},
        );
        let first = m.enqueue(item("a"));
        let again = m.enqueue(item("A"));
        assert_eq!(first, again);
        assert_eq!(m.snapshot().len(), 1);
        assert!(m.item_of("a").is_some());
        gate.store(true, Ordering::Relaxed);
        wait_for(&m, |m| !m.busy());
        m.enqueue(item("a")); // finished ones do not count
        assert_eq!(m.snapshot().len(), 2);
    }

    #[test]
    fn a_waiting_item_can_be_moved_and_cancelled_and_the_running_one_stops_at_its_next_chunk() {
        let started = Arc::new(AtomicUsize::new(0));
        let s = started.clone();
        let m = DownloadManager::new(
            Arc::new(move |it, ctx| {
                s.fetch_add(1, Ordering::Relaxed);
                if it.game_id == "a" {
                    while !ctx.cancel.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    return (false, "stopped".into());
                }
                (true, String::new())
            }),
            || {},
        );
        let a = m.enqueue(item("a"));
        wait_for(&m, |m| m.active().is_some());
        let b = m.enqueue(item("b"));
        let c = m.enqueue(item("c"));
        m.move_first(&c);
        assert_eq!(
            m.snapshot()
                .iter()
                .filter(|i| i.is_waiting())
                .map(|i| i.game_id.clone())
                .collect::<Vec<_>>(),
            vec!["c", "b"]
        );
        m.move_by(&c, 1);
        assert_eq!(
            m.snapshot()
                .iter()
                .filter(|i| i.is_waiting())
                .map(|i| i.game_id.clone())
                .collect::<Vec<_>>(),
            vec!["b", "c"]
        );
        m.cancel(&b);
        assert_eq!(m.item(&b).unwrap().state, DownloadState::Cancelled);
        m.cancel(&a);
        wait_for(&m, |m| !m.busy());
        assert_eq!(m.item(&a).unwrap().state, DownloadState::Cancelled);
        assert_eq!(m.item(&c).unwrap().state, DownloadState::Done);
        assert_eq!(started.load(Ordering::Relaxed), 2); // b never started
    }

    #[test]
    fn a_failed_item_says_why_and_a_panic_does_not_end_the_queue() {
        let m = DownloadManager::new(
            Arc::new(|it, _| {
                if it.game_id == "boom" {
                    panic!("oops");
                }
                (false, "the server answered 404".into())
            }),
            || {},
        );
        let boom = m.enqueue(item("boom"));
        let bad = m.enqueue(item("bad"));
        wait_for(&m, |m| !m.busy());
        assert_eq!(m.item(&boom).unwrap().state, DownloadState::Failed);
        let b = m.item(&bad).unwrap();
        assert_eq!((b.state, b.message.as_str()), (DownloadState::Failed, "the server answered 404"));
    }

    #[test]
    fn the_speed_follows_the_bytes_that_arrive() {
        let m = DownloadManager::new(
            Arc::new(|_, ctx| {
                for i in 1..=10 {
                    ctx.progress(i as f64 / 10.0);
                    std::thread::sleep(Duration::from_millis(60));
                }
                (true, String::new())
            }),
            || {},
        );
        let mut it = item("a");
        it.info.size = 1_000_000;
        m.enqueue(it);
        wait_for(&m, |m| m.speed() > 100_000.0);
        assert!(m.remaining().is_some());
        wait_for(&m, |m| !m.busy());
        assert_eq!(m.speed(), 0.0);
    }

    #[test]
    fn the_window_is_told_when_something_changes() {
        let changes = Arc::new(AtomicUsize::new(0));
        let c = changes.clone();
        let m = DownloadManager::new(
            Arc::new(|_, ctx| {
                ctx.installing();
                (true, String::new())
            }),
            move || {
                c.fetch_add(1, Ordering::Relaxed);
            },
        );
        m.enqueue(item("a"));
        wait_for(&m, |m| !m.busy());
        assert!(changes.load(Ordering::Relaxed) >= 3);
    }
}
