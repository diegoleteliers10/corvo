//! A pomodoro timer: focus and break intervals with OS notifications
//! when each interval ends.
//!
//! Routing follows the corvo page model: root search exposes quick
//! commands (start, pause, stop) with the live countdown as a result
//! row, `pomodoro 40` starts a custom focus immediately, and the
//! dedicated page (`pomodoro-page:` queries) shows the same controls.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use corvo_core::{
    phosphor_svgs, Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext,
    Icon, SearchContext, SearchResult,
};

const DEFAULT_WORK: Duration = Duration::from_secs(25 * 60);
const DEFAULT_BREAK: Duration = Duration::from_secs(5 * 60);

/// Longest interval the parser accepts, in minutes.
const MAX_MINUTES: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Focus,
    Break,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::Break => "Break",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TimerState {
    Idle,
    Running {
        phase: Phase,
        ends_at: Instant,
        duration: Duration,
    },
    Paused {
        phase: Phase,
        remaining: Duration,
        duration: Duration,
    },
}

fn timer_state() -> &'static Mutex<TimerState> {
    static STATE: OnceLock<Mutex<TimerState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(TimerState::Idle))
}

fn watcher_started() -> &'static OnceLock<()> {
    static STARTED: OnceLock<()> = OnceLock::new();
    &STARTED
}

/// What the UI paints: the phase on screen and the time left on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub phase: Phase,
    pub paused: bool,
    /// Time left in the interval.
    pub remaining: Duration,
    /// Total length of the interval, for progress rendering.
    pub duration: Duration,
}

impl Snapshot {
    pub fn phase_label(&self) -> &'static str {
        self.phase.label()
    }
}

/// Starts a focus or break interval, replacing any previous one.
pub fn start(phase: Phase, duration: Duration) {
    if let Ok(mut state) = timer_state().lock() {
        *state = TimerState::Running {
            phase,
            ends_at: Instant::now() + duration,
            duration,
        };
    }
    ensure_watcher();
}

/// Pauses the running interval. Returns false when there is nothing to
/// pause.
pub fn pause() -> bool {
    let Ok(mut state) = timer_state().lock() else {
        return false;
    };
    match *state {
        TimerState::Running {
            phase,
            ends_at,
            duration,
        } => {
            let remaining = ends_at.saturating_duration_since(Instant::now());
            *state = TimerState::Paused {
                phase,
                remaining,
                duration,
            };
            true
        }
        _ => false,
    }
}

/// Resumes a paused interval. Returns the phase resumed, if any.
pub fn resume() -> Option<Phase> {
    let Ok(mut state) = timer_state().lock() else {
        return None;
    };
    match *state {
        TimerState::Paused {
            phase,
            remaining,
            duration,
        } => {
            *state = TimerState::Running {
                phase,
                ends_at: Instant::now() + remaining,
                // Resume keeps the original interval length so the
                // progress bar does not jump.
                duration,
            };
            Some(phase)
        }
        _ => None,
    }
}

/// Stops everything. Returns true when a timer was actually running.
pub fn stop() -> bool {
    let Ok(mut state) = timer_state().lock() else {
        return false;
    };
    let was_active = !matches!(*state, TimerState::Idle);
    *state = TimerState::Idle;
    was_active
}

/// Ends the current interval now, starting the break after a focus or
/// going idle after a break, with the same notification an interval end
/// produces on its own. Returns false when idle.
pub fn skip() -> bool {
    let finished = {
        let Ok(mut state) = timer_state().lock() else {
            return false;
        };
        let phase = match *state {
            TimerState::Idle => return false,
            TimerState::Running { phase, .. } | TimerState::Paused { phase, .. } => phase,
        };
        advance_state(&mut state, phase, Instant::now());
        phase
    };
    notify_finished(finished);
    true
}

/// True while an interval runs (the UI's 1 Hz pump keys off this).
pub fn is_running() -> bool {
    timer_state()
        .lock()
        .map(|state| matches!(*state, TimerState::Running { .. }))
        .unwrap_or(false)
}

/// The phase and remaining time, or None when idle.
pub fn snapshot() -> Option<Snapshot> {
    let state = timer_state().lock().ok()?;
    match *state {
        TimerState::Idle => None,
        TimerState::Running {
            phase,
            ends_at,
            duration,
        } => Some(Snapshot {
            phase,
            paused: false,
            remaining: ends_at
                .saturating_duration_since(Instant::now())
                .min(duration),
            duration,
        }),
        TimerState::Paused {
            phase,
            remaining,
            duration,
        } => Some(Snapshot {
            phase,
            paused: true,
            remaining,
            duration,
        }),
    }
}

fn advance_state(state: &mut TimerState, finished: Phase, now: Instant) {
    *state = match finished {
        Phase::Focus => TimerState::Running {
            phase: Phase::Break,
            ends_at: now + DEFAULT_BREAK,
            duration: DEFAULT_BREAK,
        },
        Phase::Break => TimerState::Idle,
    };
}

fn expire_state(state: &mut TimerState, now: Instant) -> Option<Phase> {
    let TimerState::Running { phase, ends_at, .. } = *state else {
        return None;
    };
    if now < ends_at {
        return None;
    }
    advance_state(state, phase, now);
    Some(phase)
}

fn notify_finished(finished: Phase) {
    let (title, body) = match finished {
        Phase::Focus => (
            "Focus finished",
            format!("Time for a {} minute break.", DEFAULT_BREAK.as_secs() / 60),
        ),
        Phase::Break => (
            "Break over",
            "Start a new focus whenever you are ready.".to_string(),
        ),
    };
    if let Err(error) = corvo_platform::notify(title, &body) {
        corvo_platform::diagnostics::record_error("pomodoro", "notify_failed");
        eprintln!("corvo: pomodoro notification failed: {error}");
    }
}

fn ensure_watcher() {
    watcher_started().get_or_init(|| {
        std::thread::spawn(|| loop {
            let finished = timer_state()
                .lock()
                .ok()
                .and_then(|mut state| expire_state(&mut state, Instant::now()));
            if let Some(phase) = finished {
                notify_finished(phase);
            }
            std::thread::sleep(Duration::from_millis(250));
        });
    });
}

fn parse_minutes(input: &str) -> Option<u64> {
    input
        .split_whitespace()
        .find_map(|word| word.trim_end_matches(['m', 'M']).parse::<u64>().ok())
        .filter(|minutes| (1..=MAX_MINUTES).contains(minutes))
}

fn format_remaining(remaining: Duration) -> String {
    let total = remaining.as_secs();
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn action_icon(kind: &str) -> Icon {
    use phosphor_svgs::style::regular as icons;
    Icon::Svg(match kind {
        "start" => icons::PLAY,
        "pause" => icons::PAUSE,
        "stop" => icons::STOP,
        "skip" => icons::SKIP_FORWARD,
        "break" => icons::COFFEE,
        "status" => icons::TIMER,
        _ => icons::TIMER,
    })
}

fn control_row(key: &str, label: &str, score: i32) -> SearchResult {
    SearchResult {
        id: format!("pomodoro:{key}"),
        title: label.to_owned(),
        subtitle: None,
        icon: action_icon(key.split(':').next().unwrap_or("status")),
        score,
        accessory: None,
    }
}

fn status_row(score: i32) -> SearchResult {
    match snapshot() {
        Some(current) => {
            let state = if current.paused { "paused" } else { "running" };
            SearchResult {
                id: "pomodoro:status".into(),
                title: format!(
                    "{} · {}",
                    current.phase_label(),
                    format_remaining(current.remaining)
                ),
                subtitle: Some(state.to_owned()),
                icon: action_icon("status"),
                score,
                accessory: Some(current.phase_label().to_owned()),
            }
        }
        None => SearchResult {
            id: "pomodoro:status".into(),
            title: "Pomodoro".into(),
            subtitle: Some("No timer running".into()),
            icon: action_icon("status"),
            score,
            accessory: None,
        },
    }
}

/// The control set for the current state, shared by root search and the
/// dedicated page.
fn control_results(custom_minutes: Option<u64>) -> Vec<SearchResult> {
    let minutes = custom_minutes.unwrap_or(DEFAULT_WORK.as_secs() / 60);
    let mut results = Vec::new();
    if snapshot().is_some() {
        results.push(status_row(960));
        results.push(control_row("pause", "Pause", 950));
        results.push(control_row("resume", "Resume", 949));
        results.push(control_row("skip", "Skip to Next Interval", 940));
        results.push(control_row("stop", "Stop Timer", 930));
    } else {
        results.push(SearchResult {
            id: format!("pomodoro:start-focus:{minutes}"),
            title: format!("Start Focus ({minutes} min)"),
            subtitle: Some("Pomodoro".into()),
            icon: action_icon("start"),
            score: 960,
            accessory: None,
        });
        let break_minutes = DEFAULT_BREAK.as_secs() / 60;
        results.push(SearchResult {
            id: format!("pomodoro:start-break:{break_minutes}"),
            title: format!("Start Break ({break_minutes} min)"),
            subtitle: Some("Pomodoro".into()),
            icon: action_icon("break"),
            score: 950,
            accessory: None,
        });
    }
    results
}

fn open_result(score: i32) -> SearchResult {
    SearchResult {
        id: "pomodoro:open".into(),
        title: "Pomodoro Timer".into(),
        subtitle: Some("Commands".into()),
        icon: action_icon("status"),
        score,
        accessory: None,
    }
}

#[derive(Default)]
pub struct PomodoroCommand;

corvo_core::register_command!(PomodoroCommand);

#[async_trait::async_trait]
impl Command for PomodoroCommand {
    fn id(&self) -> &'static str {
        "pomodoro"
    }

    fn keywords(&self) -> &'static [&'static str] {
        keywords()
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        // Dedicated page: the typed input is an optional custom duration.
        if let Some(page_input) = query.strip_prefix("pomodoro-page:") {
            let mut results = control_results(parse_minutes(page_input));
            if results.len() > ctx.max_results {
                results.truncate(ctx.max_results);
            }
            return results;
        }

        let trimmed = query.trim();
        if trimmed.is_empty() {
            return vec![open_result(1000)];
        }

        // Keyword-first queries act as quick commands; the first number
        // after the keyword is a custom focus length in minutes.
        let first = trimmed.split_whitespace().next().unwrap_or_default();
        if keywords().contains(&first.to_lowercase().as_str()) {
            let mut results = control_results(parse_minutes(trimmed));
            results.push(open_result(500));
            results.truncate(ctx.max_results);
            return results;
        }

        corvo_core::search_match_score(trimmed, &["Pomodoro Timer", "pomodoro focus break timer"])
            .map(|score| vec![open_result(score + 120)])
            .unwrap_or_default()
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some(key) = result_id.strip_prefix("pomodoro:") else {
            return Err(CommandError::NotFound);
        };
        let message = if key == "open" {
            return Ok(Action::ShowToast("Pomodoro Timer".into()));
        } else if key == "status" {
            match snapshot() {
                Some(current) => format!(
                    "{} — {} left",
                    current.phase_label(),
                    format_remaining(current.remaining)
                ),
                None => "No timer running".to_string(),
            }
        } else if let Some(minutes) = key
            .strip_prefix("start-focus:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            start(Phase::Focus, Duration::from_secs(minutes * 60));
            format!("Focus started — {minutes} min")
        } else if let Some(minutes) = key
            .strip_prefix("start-break:")
            .and_then(|value| value.parse::<u64>().ok())
        {
            start(Phase::Break, Duration::from_secs(minutes * 60));
            format!("Break started — {minutes} min")
        } else if key == "pause" {
            if pause() {
                "Timer paused".to_string()
            } else {
                return Err(CommandError::NotFound);
            }
        } else if key == "resume" {
            match resume() {
                Some(phase) => format!("{} resumed", phase.label()),
                None => return Err(CommandError::NotFound),
            }
        } else if key == "stop" {
            if stop() {
                "Timer stopped".to_string()
            } else {
                return Err(CommandError::NotFound);
            }
        } else if key == "skip" {
            if skip() {
                "Interval skipped".to_string()
            } else {
                return Err(CommandError::NotFound);
            }
        } else {
            return Err(CommandError::NotFound);
        };
        Ok(Action::ShowToast(message))
    }

    fn actions(&self, result_id: &str) -> Vec<CommandAction> {
        let Some(key) = result_id.strip_prefix("pomodoro:") else {
            return Vec::new();
        };
        let label = match key {
            "open" => Some("Open Pomodoro Timer"),
            "status" => Some("Show Status"),
            "pause" => Some("Pause Timer"),
            "resume" => Some("Resume Timer"),
            "stop" => Some("Stop Timer"),
            "skip" => Some("Skip Interval"),
            other if other.starts_with("start-focus:") || other.starts_with("start-break:") => {
                Some("Run")
            }
            _ => None,
        };
        label
            .map(|label| {
                vec![CommandAction {
                    id: "pomodoro:primary".into(),
                    label: label.into(),
                    action: Action::ShowToast("Pomodoro Timer".into()),
                    icon: action_icon(key.split(':').next().unwrap_or("status")),
                    group: ActionGroup::Primary,
                    hotkey: Some("enter"),
                }]
            })
            .unwrap_or_default()
    }
}

fn keywords() -> &'static [&'static str] {
    &["pomodoro", "timer", "focus"]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The timer is a process-wide static; tests that drive it must not
    /// interleave.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn search(query: &str) -> Vec<SearchResult> {
        smol::block_on(<PomodoroCommand as Command>::search(
            &PomodoroCommand,
            query,
            &SearchContext::default(),
        ))
    }

    fn execute(id: &str) -> Result<Action, CommandError> {
        smol::block_on(<PomodoroCommand as Command>::execute(
            &PomodoroCommand,
            id,
            &ExecutionContext::default(),
        ))
    }

    #[test]
    fn expiry_preserves_stopped_paused_and_restarted_timers() {
        let now = Instant::now();
        let mut state = TimerState::Idle;
        assert_eq!(expire_state(&mut state, now), None);
        state = TimerState::Paused {
            phase: Phase::Focus,
            remaining: Duration::ZERO,
            duration: DEFAULT_WORK,
        };
        let paused = state.clone();
        assert_eq!(expire_state(&mut state, now), None);
        assert_eq!(state, paused);
        state = TimerState::Running {
            phase: Phase::Focus,
            ends_at: now + DEFAULT_WORK,
            duration: DEFAULT_WORK,
        };
        let restarted = state.clone();
        assert_eq!(expire_state(&mut state, now), None);
        assert_eq!(state, restarted);
        assert_eq!(
            expire_state(&mut state, now + DEFAULT_WORK),
            Some(Phase::Focus)
        );
        assert_eq!(expire_state(&mut state, now + DEFAULT_WORK), None);
        assert_eq!(
            expire_state(&mut state, now + DEFAULT_WORK + DEFAULT_BREAK),
            Some(Phase::Break)
        );
        assert_eq!(state, TimerState::Idle);
    }

    #[test]
    fn timer_state_machine_transitions() {
        let _guard = TEST_LOCK.lock().unwrap();
        stop();
        assert_eq!(snapshot(), None);
        assert!(!pause());

        start(Phase::Focus, Duration::from_secs(60));
        let snap = snapshot().unwrap();
        assert_eq!(snap.phase, Phase::Focus);
        assert!(!snap.paused);

        assert!(pause());
        let snap = snapshot().unwrap();
        assert!(snap.paused);
        assert!(snap.remaining <= Duration::from_secs(60));

        assert_eq!(resume(), Some(Phase::Focus));
        assert!(is_running());

        assert!(stop());
        assert_eq!(snapshot(), None);
    }

    #[test]
    fn focus_advances_to_break_and_break_to_idle() {
        let _guard = TEST_LOCK.lock().unwrap();
        stop();
        start(Phase::Focus, Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(400));
        let snap = snapshot().expect("focus flowed into a break");
        assert_eq!(snap.phase, Phase::Break);

        assert!(skip());
        assert_eq!(snapshot(), None, "skipping the break ends the session");
    }

    #[test]
    fn root_quick_commands_respond_to_keywords() {
        let _guard = TEST_LOCK.lock().unwrap();
        stop();
        // Match: keyword with a custom length.
        let results = search("pomodoro 40");
        assert!(results
            .iter()
            .any(|result| result.id == "pomodoro:start-focus:40"));

        // Match: bare keyword on the page namespace.
        let results = search("pomodoro-page:");
        assert!(results
            .iter()
            .any(|result| result.id == "pomodoro:start-focus:25"));

        // Match: empty query surfaces the entry.
        assert_eq!(search("")[0].id, "pomodoro:open");

        // Non-match.
        assert!(search("hello world").is_empty());
    }

    #[test]
    fn execute_controls_the_timer() {
        let _guard = TEST_LOCK.lock().unwrap();
        stop();
        execute("pomodoro:start-focus:40").unwrap();
        assert_eq!(snapshot().unwrap().phase, Phase::Focus);

        execute("pomodoro:pause").unwrap();
        assert!(snapshot().unwrap().paused);

        execute("pomodoro:resume").unwrap();
        assert!(is_running());

        execute("pomodoro:stop").unwrap();
        assert_eq!(snapshot(), None);

        assert_eq!(
            execute("pomodoro:unknown").unwrap_err(),
            CommandError::NotFound
        );
    }

    #[test]
    fn minute_parsing_is_bounded() {
        assert_eq!(parse_minutes("pomodoro 40"), Some(40));
        assert_eq!(parse_minutes("pomodoro 90m"), Some(90));
        assert_eq!(parse_minutes("pomodoro"), None);
        assert_eq!(parse_minutes("pomodoro 0"), None);
        assert_eq!(parse_minutes("pomodoro 100000"), None);
    }
}
