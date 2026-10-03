//! Media control for the active player: play/pause, next, previous,
//! and what is playing right now. The OS backend lives in
//! `corvo-platform`: AppleScript to Spotify or Music on macOS, SMTC on
//! Windows, MPRIS on Linux.
//!
//! Quick commands: `play`, `pause`, `next`, and `prev` act from root
//! search immediately; the dedicated page shows the now-playing view.

use corvo_core::{
    phosphor_svgs, Action, Command, CommandError, ExecutionContext, Icon, SearchContext,
    SearchResult,
};
pub use corvo_platform::NowPlaying;

fn now_playing_cache() -> &'static std::sync::Mutex<Option<(std::time::Instant, NowPlaying)>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<(std::time::Instant, NowPlaying)>>>
        = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

/// Reads the active player. Blocking (one OS round trip); the UI calls
/// it through its unblock executor.
pub fn fetch_now_playing() -> Option<NowPlaying> {
    if let Ok(guard) = now_playing_cache().lock() {
        if let Some((fetched_at, cached)) = guard.as_ref() {
            if fetched_at.elapsed() < CACHE_TTL {
                return Some(cached.clone());
            }
        }
    }
    let fetched = corvo_platform::media_now_playing()?;
    if let Ok(mut guard) = now_playing_cache().lock() {
        *guard = Some((std::time::Instant::now(), fetched.clone()));
    }
    Some(fetched)
}

/// The cached value only, never touching the OS: safe on the search
/// path.
pub fn cached_now_playing() -> Option<NowPlaying> {
    let guard = now_playing_cache().lock().ok()?;
    let (fetched_at, cached) = guard.as_ref()?;
    (fetched_at.elapsed() < CACHE_TTL).then(|| cached.clone())
}

/// Runs a transport control on the active player. `action` is one of
/// `toggle`, `next`, or `previous`. Returns false when no player runs.
pub fn control(action: &str) -> bool {
    let transport = match action {
        "toggle" => corvo_platform::MediaTransport::Toggle,
        "next" => corvo_platform::MediaTransport::Next,
        "previous" => corvo_platform::MediaTransport::Previous,
        _ => return false,
    };
    let sent = corvo_platform::media_transport(transport);
    // Invalidate the cache so the next read reflects the change.
    if sent {
        if let Ok(mut guard) = now_playing_cache().lock() {
            *guard = None;
        }
    }
    sent
}

fn control_result(action: &str, label: &str, score: i32) -> SearchResult {
    SearchResult {
        id: format!("media:{action}"),
        title: label.to_owned(),
        subtitle: Some("Active player".into()),
        icon: Icon::Svg(match action {
            "toggle" => phosphor_svgs::style::regular::PLAY,
            "next" => phosphor_svgs::style::regular::SKIP_FORWARD,
            _ => phosphor_svgs::style::regular::SKIP_BACK,
        }),
        score,
        accessory: None,
        section: None,
    }
}

fn now_playing_result(playing: &NowPlaying, score: i32) -> SearchResult {
    SearchResult {
        id: "media:status".into(),
        title: playing.title.clone(),
        subtitle: Some(if playing.artist.is_empty() {
            playing.app.clone()
        } else {
            format!("{} · {}", playing.artist, playing.app)
        }),
        icon: Icon::Svg(phosphor_svgs::style::regular::MUSIC_NOTES),
        score,
        accessory: Some(if playing.playing { "Playing" } else { "Paused" }.into()),
        section: None,
    }
}

fn open_result(score: i32) -> SearchResult {
    SearchResult {
        id: "media:open".into(),
        title: "Media Control".into(),
        subtitle: Some("Now playing and transport".into()),
        icon: Icon::Svg(phosphor_svgs::style::regular::MUSIC_NOTES),
        score,
        accessory: None,
        section: None,
    }
}

#[derive(Default)]
pub struct MediaControlCommand;

corvo_core::register_command!(MediaControlCommand);

#[async_trait::async_trait]
impl Command for MediaControlCommand {
    fn id(&self) -> &'static str {
        "media-control"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "media", "music", "spotify", "play", "pause", "next", "prev", "previous", "track",
        ]
    }

    fn priority(&self) -> u8 {
        60
    }

    async fn search(&self, query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return vec![open_result(1000)];
        }

        let first = trimmed.split_whitespace().next().unwrap_or_default();
        let lowered = first.to_lowercase();
        match lowered.as_str() {
            // Single-word quick commands act straight from root.
            "play" | "pause" => {
                return vec![control_result("toggle", "Play / Pause", 1000)];
            }
            "next" => {
                return vec![control_result("next", "Next Track", 1000)];
            }
            "prev" | "previous" => {
                return vec![control_result("previous", "Previous Track", 1000)];
            }
            "media" | "music" | "spotify" | "track" => {}
            _ => {
                return corvo_core::search_match_score(
                    trimmed,
                    &["Media Control", "media music play pause next spotify"],
                )
                .map(|score| vec![open_result(score + 120)])
                .unwrap_or_default();
            }
        }

        let mut results = Vec::new();
        if let Some(playing) = cached_now_playing() {
            results.push(now_playing_result(&playing, 1010));
        }
        results.push(open_result(1000));
        results
    }

    async fn execute(
        &self,
        result_id: &str,
        _ctx: &ExecutionContext,
    ) -> Result<Action, CommandError> {
        let Some(key) = result_id.strip_prefix("media:") else {
            return Err(CommandError::NotFound);
        };
        if key == "open" {
            return Ok(Action::ShowToast("Media Control".into()));
        }
        if key == "status" {
            let summary = cached_now_playing()
                .map(|playing| format!("{} — {}", playing.title, playing.app))
                .unwrap_or_else(|| "No player running".into());
            return Ok(Action::ShowToast(summary));
        }
        if ["toggle", "next", "previous"].contains(&key) {
            let action = key.to_owned();
            return smol::unblock(move || control(&action))
                .await
                .then(|| Action::ShowToast("Media control sent".into()))
                .ok_or(CommandError::Platform("no player is running".into()));
        }
        Err(CommandError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(query: &str) -> Vec<SearchResult> {
        smol::block_on(<MediaControlCommand as Command>::search(
            &MediaControlCommand,
            query,
            &SearchContext::default(),
        ))
    }

    #[test]
    fn quick_commands_route_from_root() {
        // Match: one-word transport commands.
        assert_eq!(search("play")[0].id, "media:toggle");
        assert_eq!(search("pause")[0].id, "media:toggle");
        assert_eq!(search("next")[0].id, "media:next");
        assert_eq!(search("prev")[0].id, "media:previous");

        // Match: empty query.
        assert_eq!(search("")[0].id, "media:open");

        // Non-match.
        assert!(search("hello world").is_empty());
    }

    #[test]
    fn rejects_unknown_actions() {
        assert_eq!(
            smol::block_on(<MediaControlCommand as Command>::execute(
                &MediaControlCommand,
                "media:rewind",
                &ExecutionContext::default(),
            ))
            .unwrap_err(),
            CommandError::NotFound
        );
    }
}
