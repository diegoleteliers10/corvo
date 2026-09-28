use std::sync::{Mutex, OnceLock};

#[cfg(not(target_os = "windows"))]
use sysinfo::Signal;
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::{PlatformError, PlatformResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessSnapshot {
    pub identity: ProcessIdentity,
    pub name: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListeningPortSnapshot {
    pub port: u16,
    pub process: ProcessIdentity,
    pub process_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminationMode {
    Graceful,
    Force,
}

static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();

fn system() -> &'static Mutex<System> {
    SYSTEM.get_or_init(|| Mutex::new(System::new_all()))
}

pub fn process_snapshot() -> PlatformResult<Vec<ProcessSnapshot>> {
    let mut system = system()
        .lock()
        .map_err(|_| PlatformError::Os("process cache is unavailable".into()))?;
    system.refresh_processes(ProcessesToUpdate::All);

    Ok(system
        .processes()
        .iter()
        .map(|(pid, process)| ProcessSnapshot {
            identity: ProcessIdentity {
                pid: pid.as_u32(),
                start_time: process.start_time(),
            },
            name: process.name().to_string_lossy().into_owned(),
            cpu_percent: process.cpu_usage(),
            memory_bytes: process.memory(),
        })
        .collect())
}

pub fn listening_port_snapshot() -> PlatformResult<Vec<ListeningPortSnapshot>> {
    #[cfg(target_os = "windows")]
    {
        return windows_listening_port_snapshot();
    }

    #[cfg(target_os = "linux")]
    {
        return linux_listening_port_snapshot();
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("lsof")
            .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
            .output()
            .map_err(|error| PlatformError::Os(format!("could not start lsof: {error}")))?;
        if !output.status.success() {
            if output.status.code() == Some(1) && output.stdout.is_empty() {
                return Ok(Vec::new());
            }
            return Err(PlatformError::Os(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        let processes = process_snapshot()?
            .into_iter()
            .map(|process| (process.identity.pid, (process.identity, process.name)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut pid = None;
        let mut process_name = String::new();
        let mut ports = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Some(field) = line.chars().next() else {
                continue;
            };
            let value = &line[field.len_utf8()..];
            match field {
                'p' => {
                    pid = value.parse::<u32>().ok();
                    process_name.clear();
                }
                'c' => process_name = value.to_string(),
                'n' => {
                    let Some(port) = value.rsplit(':').next().and_then(|s| s.parse::<u16>().ok())
                    else {
                        continue;
                    };
                    let Some((process, snapshot_name)) = pid.and_then(|pid| processes.get(&pid))
                    else {
                        continue;
                    };
                    ports.push(ListeningPortSnapshot {
                        port,
                        process: *process,
                        process_name: if process_name.is_empty() {
                            snapshot_name.clone()
                        } else {
                            process_name.clone()
                        },
                    });
                }
                _ => {}
            }
        }
        ports.sort_by(|left, right| {
            left.port
                .cmp(&right.port)
                .then(left.process.pid.cmp(&right.process.pid))
        });
        ports.dedup_by(|left, right| left.port == right.port && left.process == right.process);
        Ok(ports)
    }
}

#[cfg(target_os = "windows")]
fn windows_listening_port_snapshot() -> PlatformResult<Vec<ListeningPortSnapshot>> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-NetTCPConnection -State Listen -ErrorAction Stop | ForEach-Object { '{0}|{1}' -f $_.LocalPort, $_.OwningProcess }",
        ])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|error| PlatformError::Os(format!("could not query listening ports: {error}")))?;
    if !output.status.success() {
        return Err(PlatformError::Os(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let processes = process_snapshot()?
        .into_iter()
        .map(|process| (process.identity.pid, (process.identity, process.name)))
        .collect::<std::collections::HashMap<_, _>>();
    let mut ports = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (port, pid) = line.trim().split_once('|')?;
            let port = port.trim().parse::<u16>().ok()?;
            let pid = pid.trim().parse::<u32>().ok()?;
            let (identity, process_name) = processes.get(&pid)?;
            Some(ListeningPortSnapshot {
                port,
                process: *identity,
                process_name: process_name.clone(),
            })
        })
        .collect::<Vec<_>>();
    ports.sort_by(|left, right| {
        left.port
            .cmp(&right.port)
            .then(left.process.pid.cmp(&right.process.pid))
    });
    ports.dedup_by(|left, right| left.port == right.port && left.process == right.process);
    Ok(ports)
}

#[cfg(target_os = "linux")]
fn linux_listening_port_snapshot() -> PlatformResult<Vec<ListeningPortSnapshot>> {
    use std::collections::{HashMap, HashSet};

    let mut socket_ports = HashMap::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let contents = match std::fs::read_to_string(table) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(PlatformError::Os(format!(
                    "could not read {table}: {error}"
                )));
            }
        };
        for line in contents.lines().skip(1) {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.get(3) != Some(&"0A") {
                continue;
            }
            let Some(local_address) = fields.get(1) else {
                continue;
            };
            let Some(port) = local_address
                .rsplit(':')
                .next()
                .and_then(|value| u16::from_str_radix(value, 16).ok())
            else {
                continue;
            };
            let Some(inode) = fields.get(9).and_then(|value| value.parse::<u64>().ok()) else {
                continue;
            };
            socket_ports.insert(inode, port);
        }
    }
    if socket_ports.is_empty() {
        return Ok(Vec::new());
    }

    let process_by_pid = process_snapshot()?
        .into_iter()
        .map(|process| (process.identity.pid, process))
        .collect::<HashMap<_, _>>();
    let mut matched_sockets = HashSet::new();
    let mut ports = Vec::new();
    let entries = std::fs::read_dir("/proc")
        .map_err(|error| PlatformError::Os(format!("could not scan process files: {error}")))?;
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(process) = process_by_pid.get(&pid) else {
            continue;
        };
        let Ok(descriptors) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten() {
            let Ok(target) = std::fs::read_link(descriptor.path()) else {
                continue;
            };
            let Some(inode) = target
                .to_str()
                .and_then(|target| target.strip_prefix("socket:["))
                .and_then(|target| target.strip_suffix(']'))
                .and_then(|inode| inode.parse::<u64>().ok())
            else {
                continue;
            };
            let Some(port) = socket_ports.get(&inode).copied() else {
                continue;
            };
            if matched_sockets.insert(inode) {
                ports.push(ListeningPortSnapshot {
                    port,
                    process: process.identity,
                    process_name: process.name.clone(),
                });
            }
        }
        if matched_sockets.len() == socket_ports.len() {
            break;
        }
    }
    ports.sort_by(|left, right| {
        left.port
            .cmp(&right.port)
            .then(left.process.pid.cmp(&right.process.pid))
    });
    ports.dedup_by(|left, right| left.port == right.port && left.process == right.process);
    Ok(ports)
}

pub fn terminate_process(identity: ProcessIdentity, mode: TerminationMode) -> PlatformResult<()> {
    let mut system = system()
        .lock()
        .map_err(|_| PlatformError::Os("process cache is unavailable".into()))?;
    system.refresh_processes(ProcessesToUpdate::All);
    let pid = Pid::from_u32(identity.pid);
    let Some(process) = system.process(pid) else {
        return Err(PlatformError::Os("process no longer exists".into()));
    };
    if process.start_time() != identity.start_time {
        return Err(PlatformError::Os("process identity changed".into()));
    }

    #[cfg(target_os = "windows")]
    let outcome = {
        let _ = mode;
        Some(process.kill())
    };
    #[cfg(not(target_os = "windows"))]
    let outcome = {
        let signal = match mode {
            TerminationMode::Graceful => Signal::Term,
            TerminationMode::Force => Signal::Kill,
        };
        process.kill_with(signal)
    };
    match outcome {
        Some(true) => Ok(()),
        Some(false) => Err(PlatformError::Os(
            "operating system rejected process termination".into(),
        )),
        None => Err(PlatformError::Unsupported(
            "process termination signal".into(),
        )),
    }
}
