//! Local diagnostic events. Records exclude log messages and panic payloads.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_FIELD_BYTES: usize = 160;
const MAX_RECENT_EVENTS: usize = 32;
const REPEAT_INTERVAL: Duration = Duration::from_secs(30);
static STATE: OnceLock<Mutex<Store>> = OnceLock::new();
static LOGGER: LocalLogger = LocalLogger;

struct Store {
    directory: PathBuf,
    version: &'static str,
    max_file_bytes: u64,
    recent: VecDeque<(EventKey, Instant)>,
}

#[derive(PartialEq, Eq)]
struct EventKey {
    level: String,
    component: String,
    event: String,
    source: Option<String>,
    line: Option<u32>,
}

#[derive(serde::Serialize)]
struct Event<'a> {
    timestamp_unix_ms: u128,
    version: &'a str,
    os: &'static str,
    arch: &'static str,
    level: &'a str,
    component: &'a str,
    event: &'a str,
    source: Option<&'a str>,
    line: Option<u32>,
}

/// Starts local logs and preserves the existing panic hook. Call once at startup.
pub fn init(version: &'static str) {
    let Some(directory) = directories::ProjectDirs::from("", "", "corvo")
        .map(|dirs| dirs.data_local_dir().join("logs"))
    else {
        return;
    };
    if STATE
        .set(Mutex::new(Store {
            directory,
            version,
            max_file_bytes: MAX_FILE_BYTES,
            recent: VecDeque::new(),
        }))
        .is_err()
    {
        return;
    }
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location();
        append(
            "ERROR",
            "runtime",
            "panic",
            location.map(|location| location.file()),
            location.map(|location| location.line()),
        );
        previous(info);
    }));
}

/// Records fixed event names. Do not pass user data or formatted error text.
#[track_caller]
pub fn record_error(component: &'static str, event: &'static str) {
    let location = std::panic::Location::caller();
    append(
        "ERROR",
        component,
        event,
        Some(location.file()),
        Some(location.line()),
    );
}

/// Gets the local log directory for manual sharing.
pub fn log_directory() -> Option<PathBuf> {
    STATE
        .get()?
        .lock()
        .ok()
        .map(|store| store.directory.clone())
}

fn append(level: &str, component: &str, event: &str, source: Option<&str>, line: Option<u32>) {
    let Some(state) = STATE.get() else {
        return;
    };
    let Ok(mut store) = state.try_lock() else {
        return;
    };
    let _ = store.append(level, component, event, source, line);
}

fn bounded(value: &str) -> &str {
    let mut end = value.len().min(MAX_FIELD_BYTES);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn source_name(source: &str) -> &str {
    source.rsplit(['/', '\\']).next().unwrap_or(source)
}

impl Store {
    fn append_log(&mut self, record: &log::Record<'_>) -> io::Result<()> {
        self.append(
            record.level().as_str(),
            record.module_path_static().unwrap_or("upstream"),
            "log_record",
            record.file_static(),
            record.line(),
        )
    }

    fn append(
        &mut self,
        level: &str,
        component: &str,
        event: &str,
        source: Option<&str>,
        line: Option<u32>,
    ) -> io::Result<()> {
        let key = EventKey {
            level: bounded(level).to_owned(),
            component: bounded(component).to_owned(),
            event: bounded(event).to_owned(),
            source: source.map(source_name).map(bounded).map(str::to_owned),
            line,
        };
        let now = Instant::now();
        let is_panic = component == "runtime" && event == "panic";
        if !is_panic
            && self.recent.iter().any(|(previous, recorded)| {
                previous == &key && now.duration_since(*recorded) < REPEAT_INTERVAL
            })
        {
            return Ok(());
        }
        let event = Event {
            timestamp_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            version: bounded(self.version),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            level,
            component: bounded(component),
            event: bounded(event),
            source: source.map(source_name).map(bounded),
            line,
        };
        let mut bytes = serde_json::to_vec(&event)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > self.max_file_bytes {
            return Ok(());
        }
        fs::create_dir_all(&self.directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))?;
        }
        let active = self.directory.join("corvo.jsonl");
        let current_size = match fs::metadata(&active) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error),
        };
        if current_size.saturating_add(bytes.len() as u64) > self.max_file_bytes {
            self.rotate(&active)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(active)?;
        file.write_all(&bytes)?;
        file.flush()?;
        if !is_panic {
            if self.recent.len() == MAX_RECENT_EVENTS {
                self.recent.pop_front();
            }
            self.recent.push_back((key, now));
        }
        Ok(())
    }

    fn rotate(&self, active: &Path) -> io::Result<()> {
        let oldest = self.directory.join("corvo.2.jsonl");
        let previous = self.directory.join("corvo.1.jsonl");
        remove_if_present(&oldest)?;
        rename_if_present(&previous, &oldest)?;
        rename_if_present(active, &previous)
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn rename_if_present(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

struct LocalLogger;

impl log::Log for LocalLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let Some(state) = STATE.get() else {
                return;
            };
            let Ok(mut store) = state.try_lock() else {
                return;
            };
            let _ = store.append_log(record);
        }
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "corvo-diagnostics-test-{}-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
            )))
        }

        fn store(&self, max_file_bytes: u64) -> Store {
            Store {
                directory: self.0.clone(),
                version: "test-version",
                max_file_bytes,
                recent: VecDeque::new(),
            }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn rotation_keeps_three_bounded_files_and_latest_event() {
        let directory = TestDirectory::new();
        let mut store = directory.store(250);
        for event in ["first", "second", "third", "fourth", "latest"] {
            store.append("ERROR", "test", event, None, None).unwrap();
        }
        let files: Vec<_> = fs::read_dir(&directory.0).unwrap().collect();
        assert_eq!(files.len(), 3);
        for file in files {
            assert!(file.unwrap().metadata().unwrap().len() <= 250);
        }
        let active = fs::read_to_string(directory.0.join("corvo.jsonl")).unwrap();
        assert!(active.contains("latest"));
        let archived = fs::read_to_string(directory.0.join("corvo.2.jsonl")).unwrap();
        assert!(!archived.contains("first"));
    }

    #[test]
    fn records_exclude_source_directories_and_cap_fields() {
        let directory = TestDirectory::new();
        directory
            .store(MAX_FILE_BYTES)
            .append(
                "ERROR",
                &"x".repeat(1000),
                "failed",
                Some("C:\\Users\\private-name\\app.rs"),
                Some(42),
            )
            .unwrap();
        let text = fs::read_to_string(directory.0.join("corvo.jsonl")).unwrap();
        let event: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(event["source"], "app.rs");
        assert_eq!(event["component"].as_str().unwrap().len(), MAX_FIELD_BYTES);
        assert!(!text.contains("private-name"));
        assert!(event.get("message").is_none());
        assert!(event.get("payload").is_none());
        assert_eq!(bounded("é".repeat(100).as_str()).len(), MAX_FIELD_BYTES);
        let record = log::Record::builder()
            .args(format_args!("private user message"))
            .level(log::Level::Error)
            .target("dynamic private target")
            .build();
        assert!(record.module_path_static().is_none());
        directory.store(MAX_FILE_BYTES).append_log(&record).unwrap();
        let text = fs::read_to_string(directory.0.join("corvo.jsonl")).unwrap();
        assert!(!text.contains("private user message"));
        assert!(!text.contains("dynamic private target"));
    }

    #[test]
    fn repeats_are_bounded_and_panic_events_remain_visible() {
        let directory = TestDirectory::new();
        let mut store = directory.store(MAX_FILE_BYTES);
        for _ in 0..100 {
            store
                .append("ERROR", "ipc", "pipe_failed", Some("ipc.rs"), Some(20))
                .unwrap();
        }
        let active = directory.0.join("corvo.jsonl");
        assert_eq!(fs::read_to_string(&active).unwrap().lines().count(), 1);
        store.recent.front_mut().unwrap().1 = Instant::now() - Duration::from_secs(31);
        store
            .append("ERROR", "ipc", "pipe_failed", Some("ipc.rs"), Some(20))
            .unwrap();
        for _ in 0..2 {
            store
                .append("ERROR", "runtime", "panic", Some("app.rs"), Some(1))
                .unwrap();
        }
        assert_eq!(fs::read_to_string(&active).unwrap().lines().count(), 4);
        for index in 0..50 {
            store
                .append("ERROR", "test", &format!("event_{index}"), None, None)
                .unwrap();
        }
        assert_eq!(store.recent.len(), MAX_RECENT_EVENTS);
    }

    #[test]
    fn oversized_records_do_not_create_a_file() {
        let directory = TestDirectory::new();
        directory
            .store(1)
            .append("ERROR", "test", "failed", None, None)
            .unwrap();
        assert!(!directory.0.exists());
    }

    #[test]
    fn write_failure_returns_without_panic() {
        let directory = TestDirectory::new();
        fs::write(&directory.0, b"blocks directory").unwrap();
        assert!(directory
            .store(MAX_FILE_BYTES)
            .append("ERROR", "test", "failed", None, None)
            .is_err());
        assert_eq!(fs::read(&directory.0).unwrap(), b"blocks directory");
    }
}
