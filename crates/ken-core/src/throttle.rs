//! One throttle for all background work (knowledge-core decision 34).
//!
//! A person picks a pace, Gentle, Normal or Fast, or pauses: one app setting
//! over everything Ken does on its own. Every background Claude call (the
//! one-line summaries, drift's re-reads, the lint, the supersession pass,
//! ingest and the build queue) takes a turn here
//! ([`claude_turn`]): it waits while paused, waits out the pace's gap since
//! the last call ended, and takes one of the hour's calls. One call runs at
//! a time across the app, however many repos are open. Local work on the
//! graphics card or the processor (embedding for meaning search, OCR) waits
//! while paused and rests after each piece ([`rest_after`]), so it uses at
//! most the pace's share of the time. Other steps (a build item placed in
//! the inbox, a cloud file brought down) take a [`step_turn`]: the pause and
//! the gap, no call counted.
//!
//! It replaces the shared two-second pace, the summaries' hourly cap and
//! the `backgroundExtraction` switch, which Pause now is.
//!
//! The throttle also keeps what the status panel shows of the work itself:
//! what is running ([`begin`]) and what failed, with why ([`fail`]).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How fast background work goes. Gentle is the default, so a fresh install
/// makes at most 20 background Claude calls an hour, as before the throttle;
/// Normal and Fast are a person's choice in Settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pace {
    #[default]
    Gentle,
    Normal,
    Fast,
}

/// The one app setting: a pace, and whether background work is paused.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    #[serde(default)]
    pub pace: Pace,
    #[serde(default)]
    pub paused: bool,
}

/// What a pace allows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// The least time between the end of one background Claude call (or
    /// step) and the start of the next.
    pub gap: Duration,
    /// Most background Claude calls in any rolling hour.
    pub calls_per_hour: usize,
    /// The share of the time local work (embedding, OCR) may use: after a
    /// piece that took `t`, it rests `t * (1 - duty) / duty`.
    pub duty: f64,
    /// How often a knowledge base's checks (drift, lint, the graph) are
    /// looked at.
    pub checks_every: Duration,
}

impl Pace {
    pub fn limits(self) -> Limits {
        match self {
            Pace::Gentle => Limits {
                gap: Duration::from_secs(10),
                calls_per_hour: 20,
                duty: 0.25,
                checks_every: Duration::from_secs(15 * 60),
            },
            Pace::Normal => Limits {
                gap: Duration::from_secs(2),
                calls_per_hour: 60,
                duty: 0.5,
                checks_every: Duration::from_secs(5 * 60),
            },
            Pace::Fast => Limits {
                gap: Duration::ZERO,
                calls_per_hour: 240,
                duty: 1.0,
                checks_every: Duration::from_secs(60),
            },
        }
    }
}

/// Something running now, for the status panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    #[serde(skip)]
    pub id: u64,
    /// What kind of work: `summaries`, `checks`, `ingest`, `build`, `analysis`, `meaning`, `ocr`, `cloud`.
    pub kind: String,
    /// What it is working on, in a person's words.
    pub label: String,
    /// How far, when it knows: `done` of `total`.
    pub done: u64,
    pub total: u64,
}

/// Something that failed, and why, for the status panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub kind: String,
    pub label: String,
    pub reason: String,
    /// Unix seconds.
    pub at: i64,
}

/// The throttle as the status panel reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub setting: Setting,
    /// Background Claude calls made in the last hour, of `calls_per_hour`.
    pub calls_last_hour: usize,
    pub calls_per_hour: usize,
    /// Whether a background Claude call is running now.
    pub calling: bool,
    pub running: Vec<Job>,
    pub failed: Vec<Failure>,
}

/// Failures kept for the panel, newest last.
const MAX_FAILURES: usize = 30;

/// How long a call may hold the turn before another may go anyway: a turn
/// whose holder vanished without ending must not stop the app's background
/// work for good.
const TURN_HOLD: Duration = Duration::from_secs(20 * 60);

/// How often a wait looks again at the pause, the gap and the stop flag.
const POLL: Duration = Duration::from_millis(250);

struct State {
    setting: Setting,
    /// No background step before this instant.
    next: Option<Instant>,
    /// A Claude call holds the turn until this instant (or until it ends).
    held_until: Option<Instant>,
    /// When the calls of the last hour were made.
    calls: VecDeque<Instant>,
    jobs: Vec<Job>,
    failed: VecDeque<Failure>,
    next_id: u64,
}

static STATE: Mutex<State> = Mutex::new(State {
    setting: Setting { pace: Pace::Gentle, paused: false },
    next: None,
    held_until: None,
    calls: VecDeque::new(),
    jobs: Vec::new(),
    failed: VecDeque::new(),
    next_id: 1,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Set the pace and the pause. Takes effect at once: a wait in progress
/// sees it on its next look.
pub fn set(setting: Setting) {
    state().setting = setting;
}

pub fn setting() -> Setting {
    state().setting
}

/// What the current pace allows.
pub fn limits() -> Limits {
    setting().pace.limits()
}

pub fn paused() -> bool {
    setting().paused
}

/// Drop the calls older than an hour, then count one at `now` when fewer
/// than `cap` remain. Whether it was counted.
fn take_call_at(calls: &mut VecDeque<Instant>, now: Instant, cap: usize) -> bool {
    while calls.front().is_some_and(|t| now.saturating_duration_since(*t) >= Duration::from_secs(3600)) {
        calls.pop_front();
    }
    if calls.len() >= cap {
        return false;
    }
    calls.push_back(now);
    true
}

/// How long local work rests after a piece that took `took`, at `duty`.
pub fn rest_for(took: Duration, duty: f64) -> Duration {
    if duty >= 1.0 || duty <= 0.0 {
        return Duration::ZERO;
    }
    took.mul_f64((1.0 - duty) / duty)
}

/// Sleep about `d` in short pieces, waking early when `stop` says so.
/// Whether it slept through.
fn sleep_unless(d: Duration, stop: &dyn Fn() -> bool) -> bool {
    let until = Instant::now() + d;
    loop {
        if stop() {
            return false;
        }
        let now = Instant::now();
        if now >= until {
            return true;
        }
        std::thread::sleep((until - now).min(POLL));
    }
}

/// Wait while background work is paused. False when `stop` says to stop.
pub fn wait_unpaused(stop: &dyn Fn() -> bool) -> bool {
    loop {
        if stop() {
            return false;
        }
        if !paused() {
            return true;
        }
        std::thread::sleep(POLL);
    }
}

/// A background Claude call's turn, held until it is dropped. When it
/// drops, the pace's gap starts.
pub struct Turn(());

impl Drop for Turn {
    fn drop(&mut self) {
        let mut s = state();
        s.held_until = None;
        let gap = s.setting.pace.limits().gap;
        s.next = Some(Instant::now() + gap);
    }
}

/// Wait for a background Claude call's turn: while paused, while another
/// call holds the turn, until the gap since the last one ended, and until
/// the hour has a call left. The call is counted when the turn is given.
/// None when `stop` said to stop while waiting.
pub fn claude_turn(stop: &dyn Fn() -> bool) -> Option<Turn> {
    loop {
        if stop() {
            return None;
        }
        {
            let mut s = state();
            let now = Instant::now();
            let free = !s.setting.paused
                && s.held_until.is_none_or(|t| now >= t)
                && s.next.is_none_or(|t| now >= t);
            if free {
                let cap = s.setting.pace.limits().calls_per_hour;
                if take_call_at(&mut s.calls, now, cap) {
                    s.held_until = Some(now + TURN_HOLD);
                    return Some(Turn(()));
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Wait for a background step that is not a Claude call: while paused and
/// until the gap since the last step ended; the gap starts again now. False
/// when `stop` said to stop.
pub fn step_turn(stop: &dyn Fn() -> bool) -> bool {
    loop {
        if stop() {
            return false;
        }
        {
            let mut s = state();
            let now = Instant::now();
            if !s.setting.paused && s.next.is_none_or(|t| now >= t) {
                let gap = s.setting.pace.limits().gap;
                s.next = Some(now + gap);
                return true;
            }
        }
        std::thread::sleep(POLL);
    }
}

/// After a piece of local work that took `took`, rest so that kind of work
/// uses at most the pace's share of the time, then wait out a pause. False
/// when `stop` said to stop.
pub fn rest_after(took: Duration, stop: &dyn Fn() -> bool) -> bool {
    sleep_unless(rest_for(took, limits().duty), stop) && wait_unpaused(stop)
}

/// Something running, shown until the guard drops.
pub struct Running(u64);

impl Running {
    /// How far it has got.
    pub fn progress(&self, done: u64, total: u64) {
        if let Some(j) = state().jobs.iter_mut().find(|j| j.id == self.0) {
            j.done = done;
            j.total = total;
        }
    }

    /// What it is working on now.
    pub fn relabel(&self, label: &str) {
        if let Some(j) = state().jobs.iter_mut().find(|j| j.id == self.0) {
            j.label = label.to_string();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        state().jobs.retain(|j| j.id != self.0);
    }
}

/// Show `label` as running work of `kind` until the guard drops.
pub fn begin(kind: &str, label: &str) -> Running {
    let mut s = state();
    let id = s.next_id;
    s.next_id += 1;
    s.jobs.push(Job { id, kind: kind.to_string(), label: label.to_string(), done: 0, total: 0 });
    Running(id)
}

/// Record that work of `kind` on `label` failed, and why. A later failure of
/// the same work replaces the earlier one.
pub fn fail(kind: &str, label: &str, reason: &str) {
    let mut s = state();
    s.failed.retain(|f| !(f.kind == kind && f.label == label));
    s.failed.push_back(Failure {
        kind: kind.to_string(),
        label: label.to_string(),
        reason: reason.to_string(),
        at: crate::engine::now_epoch(),
    });
    while s.failed.len() > MAX_FAILURES {
        s.failed.pop_front();
    }
}

/// Forget a failure once the work went through.
pub fn clear_failure(kind: &str, label: &str) {
    state().failed.retain(|f| !(f.kind == kind && f.label == label));
}

/// The throttle and the work it holds, for the status panel.
pub fn snapshot() -> Snapshot {
    let mut s = state();
    let now = Instant::now();
    let cap = s.setting.pace.limits().calls_per_hour;
    while s.calls.front().is_some_and(|t| now.saturating_duration_since(*t) >= Duration::from_secs(3600)) {
        s.calls.pop_front();
    }
    Snapshot {
        setting: s.setting,
        calls_last_hour: s.calls.len(),
        calls_per_hour: cap,
        calling: s.held_until.is_some_and(|t| now < t),
        running: s.jobs.clone(),
        failed: s.failed.iter().cloned().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gentle_is_slower_than_normal_and_normal_than_fast() {
        let (g, n, f) = (Pace::Gentle.limits(), Pace::Normal.limits(), Pace::Fast.limits());
        assert!(g.gap > n.gap && n.gap > f.gap);
        assert!(g.calls_per_hour < n.calls_per_hour && n.calls_per_hour < f.calls_per_hour);
        assert!(g.duty < n.duty && n.duty <= f.duty);
        assert!(g.checks_every > n.checks_every && n.checks_every > f.checks_every);
    }

    #[test]
    fn the_setting_reads_and_writes_as_json() {
        let s: Setting = serde_json::from_str(r#"{"pace":"gentle","paused":true}"#).unwrap();
        assert_eq!(s, Setting { pace: Pace::Gentle, paused: true });
        assert_eq!(serde_json::from_str::<Setting>("{}").unwrap(), Setting::default());
        assert_eq!(Setting::default().pace, Pace::Gentle, "a fresh install goes gently");
        assert_eq!(Setting::default().pace.limits().calls_per_hour, 20, "20 background Claude calls an hour, as before");
    }

    #[test]
    fn an_hour_holds_only_its_calls() {
        let mut calls = VecDeque::new();
        let start = Instant::now();
        for _ in 0..3 {
            assert!(take_call_at(&mut calls, start, 3));
        }
        assert!(!take_call_at(&mut calls, start + Duration::from_secs(1800), 3), "the hour is full");
        assert!(take_call_at(&mut calls, start + Duration::from_secs(3600), 3), "an hour on, the first calls have left the window");
    }

    #[test]
    fn local_work_rests_by_the_pace() {
        let took = Duration::from_secs(2);
        assert_eq!(rest_for(took, Pace::Gentle.limits().duty), Duration::from_secs(6));
        assert_eq!(rest_for(took, Pace::Normal.limits().duty), Duration::from_secs(2));
        assert_eq!(rest_for(took, Pace::Fast.limits().duty), Duration::ZERO);
    }

    /// One test touches the global state, so the others never race it.
    #[test]
    fn pause_holds_every_turn_and_the_panel_sees_the_work() {
        set(Setting { pace: Pace::Fast, paused: true });
        let asked = std::cell::Cell::new(0);
        let stop_soon = || {
            asked.set(asked.get() + 1);
            asked.get() > 3
        };
        assert!(claude_turn(&stop_soon).is_none(), "paused: no call");
        asked.set(0);
        assert!(!step_turn(&stop_soon), "paused: no step");
        asked.set(0);
        assert!(!wait_unpaused(&stop_soon));

        set(Setting { pace: Pace::Fast, paused: false });
        let never = || false;
        let turn = claude_turn(&never).expect("unpaused, a call may go");
        assert!(snapshot().calling);
        drop(turn);
        assert!(!snapshot().calling);
        assert!(snapshot().calls_last_hour >= 1);

        let job = begin("build", "Game: overview");
        job.progress(1, 4);
        assert!(snapshot().running.iter().any(|j| j.label == "Game: overview" && j.done == 1 && j.total == 4));
        drop(job);
        assert!(!snapshot().running.iter().any(|j| j.label == "Game: overview"));

        fail("ingest", "notes.md", "first");
        fail("ingest", "notes.md", "second");
        let failed: Vec<_> = snapshot().failed.into_iter().filter(|f| f.label == "notes.md").collect();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].reason, "second");
        clear_failure("ingest", "notes.md");
        assert!(!snapshot().failed.iter().any(|f| f.label == "notes.md"));
        set(Setting::default());
    }
}
