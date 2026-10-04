use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use corvo_core::{
    Action, ActionGroup, Command, CommandAction, CommandError, ExecutionContext, Icon,
    SearchContext, SearchResult, search_match_score,
};
use corvo_platform::{ProcessIdentity, ProcessSnapshot};

const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

static PROCESS_CACHE: OnceLock<RwLock<Vec<ProcessSnapshot>>> = OnceLock::new();
static REFRESHER_STARTED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortOrder {
    Cpu,
    Memory,
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedQuery {
    text: String,
    sort: SortOrder,
}

fn parse_query(query: &str) -> ParsedQuery {
    let mut words = Vec::new();
    let mut sort = SortOrder::Cpu;

    for (index, word) in query.split_whitespace().enumerate() {
        if word.eq_ignore_ascii_case("sort:mem") {
            sort = SortOrder::Memory;
        } else if index == 0
            && matches!(
                word.to_ascii_lowercase().as_str(),
                "kill" | "process" | "processes"
            )
        {
            continue;
        } else {
            words.push(word);
        }
    }

    ParsedQuery {
        text: words.join(" "),
        sort,
    }
}

fn process_cache() -> &'static RwLock<Vec<ProcessSnapshot>> {
    PROCESS_CACHE.get_or_init(|| RwLock::new(Vec::new()))
}

fn start_refresher() {
    REFRESHER_STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            loop {
                if let Ok(processes) = corvo_platform::process_snapshot() {
                    if let Ok(mut cache) = process_cache().write() {
                        *cache = processes;
                    }
                }
                std::thread::sleep(REFRESH_INTERVAL);
            }
        });
    });
}

fn result_id(identity: &ProcessIdentity) -> String {
    format!("kill-process:{}:{}", identity.pid, identity.start_time)
}

fn parse_result_id(id: &str) -> Option<ProcessIdentity> {
    let tail = id.strip_prefix("kill-process:")?;
    let process = if let Some(port) = tail.strip_prefix("port:") {
        let (_, process) = port.split_once(':')?;
        process
    } else {
        tail
    };
    let (pid, start_time) = process.split_once(':')?;
    Some(ProcessIdentity {
        pid: pid.parse().ok()?,
        start_time: start_time.parse().ok()?,
    })
}

fn open_ports_result() -> SearchResult {
    SearchResult {
        id: "kill-process:open-ports".into(),
        title: "Kill Process Listening on".into(),
        subtitle: None,
        icon: Icon::Glyph("⌁"),
        score: 1000,
        accessory: Some("Port Manager".into()),
        section: None,
        accessories: Vec::new(),
    }
}

fn open_processes_result(filter: &str) -> SearchResult {
    SearchResult {
        id: if filter.is_empty() {
            "kill-process:open-processes".into()
        } else {
            format!("kill-process:open-processes:{filter}")
        },
        title: "Process List".into(),
        subtitle: None,
        icon: Icon::System,
        score: 1001,
        accessory: Some("Port Manager".into()),
        section: None,
        accessories: Vec::new(),
    }
}

fn root_menu_results() -> Vec<SearchResult> {
    vec![open_processes_result(""), open_ports_result()]
}

fn ports_unavailable_result(error: &str) -> SearchResult {
    SearchResult {
        id: "kill-process:ports-unavailable".into(),
        title: "Could not load listening ports".into(),
        subtitle: Some(error.into()),
        icon: Icon::System,
        score: 900,
        accessory: None,
        section: None,
        accessories: Vec::new(),
    }
}

async fn process_results_for(query: String, max_results: usize) -> Vec<SearchResult> {
    start_refresher();
    let cached = process_cache()
        .read()
        .map(|processes| processes.clone())
        .unwrap_or_default();
    let processes = if cached.is_empty() {
        smol::unblock(corvo_platform::process_snapshot)
            .await
            .unwrap_or_default()
    } else {
        cached
    };
    if let Ok(mut cache) = process_cache().write() {
        *cache = processes.clone();
    }
    build_results(&query, &processes, max_results)
}

fn port_results(
    query: &str,
    ports: &[corvo_platform::ListeningPortSnapshot],
    max_results: usize,
) -> Vec<SearchResult> {
    let needle = query.to_lowercase();
    ports
        .iter()
        .filter_map(|entry| {
            if entry.process.pid == std::process::id() {
                return None;
            }
            let port = entry.port.to_string();
            let pid = entry.process.pid.to_string();
            let score = if needle.is_empty() {
                700
            } else {
                search_match_score(&needle, &[&port, entry.process_name.as_str(), pid.as_str()])?
            };
            Some(SearchResult {
                id: format!(
                    "kill-process:port:{}:{}:{}",
                    entry.port, entry.process.pid, entry.process.start_time
                ),
                title: format!("Port {}", entry.port),
                subtitle: Some(format!(
                    "{} · PID {}",
                    entry.process_name, entry.process.pid
                )),
                icon: Icon::System,
                score,
                accessory: Some("TCP".into()),
                section: None,
                accessories: Vec::new(),
            })
        })
        .take(max_results)
        .collect()
}

fn memory_label(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{} MB", bytes / MIB)
    } else {
        format!("{} KB", bytes / 1024)
    }
}

fn build_results(
    query: &str,
    processes: &[ProcessSnapshot],
    max_results: usize,
) -> Vec<SearchResult> {
    let parsed = parse_query(query);
    let needle = parsed.text.to_lowercase();
    let mut ranked = processes
        .iter()
        .filter_map(|process| {
            if process.identity.pid == std::process::id() {
                return None;
            }
            let name = process.name.as_str();
            let score = if needle.is_empty() {
                0
            } else {
                search_match_score(&needle, &[name])?
            };
            Some((process, score))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|(left, left_score), (right, right_score)| {
        right_score.cmp(left_score).then_with(|| match parsed.sort {
            SortOrder::Cpu => right.cpu_percent.total_cmp(&left.cpu_percent),
            SortOrder::Memory => right.memory_bytes.cmp(&left.memory_bytes),
        })
    });

    ranked
        .into_iter()
        .take(max_results)
        .map(|(process, score)| SearchResult {
            id: result_id(&process.identity),
            title: process.name.clone(),
            subtitle: Some(format!(
                "PID {} · {}",
                process.identity.pid,
                memory_label(process.memory_bytes)
            )),
            icon: Icon::System,
            score: score * 100
                + match parsed.sort {
                    SortOrder::Cpu => (process.cpu_percent * 10.0).clamp(0.0, 99.0) as i32,
                    SortOrder::Memory => {
                        (process.memory_bytes / (1024 * 1024 * 100)).min(99) as i32
                    }
                },
            accessory: Some(format!("{:.1}% CPU", process.cpu_percent)),
            section: None,
            accessories: Vec::new(),
        })
        .collect()
}

#[derive(Default)]
pub struct KillProcessCommand;

corvo_core::register_command!(KillProcessCommand);

#[async_trait::async_trait]
impl Command for KillProcessCommand {
    fn id(&self) -> &'static str {
        "kill-process"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "process",
            "processes",
            "kill",
            "quit",
            "terminate",
            "cpu",
            "memory",
            "pid",
        ]
    }

    fn prefix(&self) -> Option<&'static str> {
        Some("kill")
    }

    async fn search(&self, query: &str, ctx: &SearchContext) -> Vec<SearchResult> {
        let trimmed_query = query.trim();
        if corvo_core::search::matches_command_prefix(trimmed_query, "port manager") {
            return root_menu_results();
        }
        if trimmed_query.eq_ignore_ascii_case("kill") {
            return vec![open_ports_result()];
        }
        let mut words = trimmed_query.split_whitespace();
        if words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("kill"))
        {
            let Some(mode) = words.next() else {
                return root_menu_results();
            };
            let filter = words.collect::<Vec<_>>().join(" ");
            if mode.eq_ignore_ascii_case("port") {
                if filter.is_empty() {
                    return vec![open_ports_result()];
                }
                let max_results = ctx.max_results;
                return smol::unblock(corvo_platform::listening_port_snapshot)
                    .await
                    .map(|ports| port_results(&filter, &ports, max_results))
                    .unwrap_or_else(|error| vec![ports_unavailable_result(&error.to_string())]);
            }
            if mode.eq_ignore_ascii_case("process") || mode.eq_ignore_ascii_case("processes") {
                return vec![open_processes_result(&filter)];
            }
            return root_menu_results();
        }
        if let Some(filter) = query.strip_prefix("kill-process-page:processes:") {
            return process_results_for(filter.to_string(), ctx.max_results).await;
        }
        if let Some(filter) = query.strip_prefix("kill-process-page:ports:") {
            let filter = filter.to_string();
            let max_results = ctx.max_results;
            return smol::unblock(corvo_platform::listening_port_snapshot)
                .await
                .map(|ports| port_results(&filter, &ports, max_results))
                .unwrap_or_else(|error| vec![ports_unavailable_result(&error.to_string())]);
        }
        Vec::new()
    }

    async fn execute(&self, id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        let identity = parse_result_id(id).ok_or(CommandError::NotFound)?;
        Ok(Action::TerminateProcess {
            pid: identity.pid,
            start_time: identity.start_time,
            force: false,
        })
    }

    fn actions(&self, id: &str) -> Vec<CommandAction> {
        let Some(identity) = parse_result_id(id) else {
            return Vec::new();
        };

        vec![
            CommandAction {
                id: "kill-process:terminate".into(),
                label: "Stop Process".into(),
                action: Action::TerminateProcess {
                    pid: identity.pid,
                    start_time: identity.start_time,
                    force: false,
                },
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::STOP),
                group: ActionGroup::Primary,
                hotkey: Some("enter"),
            },
            CommandAction {
                id: "kill-process:force-terminate".into(),
                label: "Force Quit Process".into(),
                action: Action::ConfirmProcessTermination {
                    pid: identity.pid,
                    start_time: identity.start_time,
                },
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::WARNING),
                group: ActionGroup::Destructive,
                hotkey: None,
            },
            CommandAction {
                id: "kill-process:copy-pid".into(),
                label: "Copy PID".into(),
                action: Action::Copy(identity.pid.to_string()),
                icon: Icon::Svg(corvo_core::phosphor_svgs::style::regular::COPY),
                group: ActionGroup::Standard,
                hotkey: Some("cmd+enter"),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, name: &str, cpu: f32, memory: u64, start_time: u64) -> ProcessSnapshot {
        ProcessSnapshot {
            identity: ProcessIdentity { pid, start_time },
            name: name.into(),
            cpu_percent: cpu,
            memory_bytes: memory,
        }
    }

    #[test]
    fn parses_memory_sort_modifier_and_keeps_other_terms() {
        assert_eq!(
            parse_query("chrome sort:mem helper"),
            ParsedQuery {
                text: "chrome helper".into(),
                sort: SortOrder::Memory
            }
        );
    }

    #[test]
    fn root_search_opens_extension_pages_and_does_not_list_processes() {
        let command = KillProcessCommand;
        let context = SearchContext::default();
        let results = smol::block_on(command.search("kill", &context));
        assert_eq!(
            results
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>(),
            ["kill-process:open-ports"]
        );
        let manager = smol::block_on(command.search("Port Manager", &context));
        assert_eq!(
            manager
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>(),
            ["kill-process:open-processes", "kill-process:open-ports"]
        );
        let process_list = smol::block_on(command.search("kill process node", &context));
        assert_eq!(process_list[0].id, "kill-process:open-processes:node");
        assert!(smol::block_on(command.search("skill", &context)).is_empty());
    }

    #[test]
    fn ranks_fuzzy_matches_then_sorts_by_memory_when_requested() {
        let processes = [
            process(1, "Google Chrome Helper", 9.0, 100, 20),
            process(2, "Google Chrome", 1.0, 300, 21),
            process(3, "Chromium", 1.0, 900, 22),
        ];

        let results = build_results("chrome sort:mem", &processes, 10);

        assert_eq!(results[0].id, "kill-process:2:21");
        assert_eq!(results[1].id, "kill-process:1:20");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn result_identity_contains_pid_and_start_time() {
        let identity = ProcessIdentity {
            pid: 41,
            start_time: 9001,
        };
        assert_eq!(parse_result_id(&result_id(&identity)), Some(identity));
        assert_eq!(parse_result_id("kill-process:41"), None);
        assert_eq!(parse_result_id("kill-process:41:bad"), None);
    }

    #[test]
    fn port_result_keeps_process_identity_for_termination() {
        let identity = ProcessIdentity {
            pid: 41,
            start_time: 9001,
        };
        assert_eq!(
            parse_result_id("kill-process:port:3000:41:9001"),
            Some(identity)
        );
        let ports = [corvo_platform::ListeningPortSnapshot {
            port: 3000,
            process: identity,
            process_name: "node".into(),
        }];
        let results = port_results("3000", &ports, 10);
        assert_eq!(results[0].id, "kill-process:port:3000:41:9001");
        assert_eq!(results[0].title, "Port 3000");
    }

    #[test]
    fn port_results_never_include_corvo() {
        let ports = [corvo_platform::ListeningPortSnapshot {
            port: 3000,
            process: ProcessIdentity {
                pid: std::process::id(),
                start_time: 1,
            },
            process_name: "corvo".into(),
        }];
        assert!(port_results("", &ports, 10).is_empty());
    }

    #[test]
    fn excludes_corvo_process_from_results() {
        let processes = [process(std::process::id(), "corvo", 100.0, 1024, 20)];
        assert!(build_results("", &processes, 10).is_empty());
    }

    #[test]
    fn force_quit_action_requires_a_confirmation_step() {
        let actions = KillProcessCommand.actions("kill-process:41:9001");
        assert!(matches!(
            &actions[1].action,
            Action::ConfirmProcessTermination {
                pid: 41,
                start_time: 9001
            }
        ));
    }
}
