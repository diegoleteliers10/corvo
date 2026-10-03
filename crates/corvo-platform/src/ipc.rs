//! Single-instance IPC. Unix domain socket on Linux and macOS, named pipe on Windows.

#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
const TOGGLE: &[u8] = b"toggle\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupDecision {
    StartResident,
    ToggleExisting,
    ExistingWithoutToggle,
}

#[cfg(unix)]
fn socket_path() -> PathBuf {
    // Suffix the UID: without it every local user competes for the same
    // path, and on macOS (no XDG_RUNTIME_DIR) that path is in the
    // shared /tmp. Another user's resident could then swallow toggles
    // or block binding outright.
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("corvo-{}.sock", unsafe { libc::getuid() }))
}

/// The pre-UID socket path. Residents built before the suffix live
/// there; they register the same global hotkey, which double-fires.
#[cfg(unix)]
fn legacy_socket_path() -> PathBuf {
    std::env::temp_dir().join("corvo.sock")
}

/// Warns when an older Corvo resident is still alive: both register
/// the same global hotkey and macOS delivers the event to each, so
/// toggles land twice and the launcher looks flaky.
#[cfg(unix)]
fn warn_about_legacy_resident() {
    let legacy = legacy_socket_path();
    if legacy == socket_path() {
        return;
    }
    if std::os::unix::net::UnixStream::connect(&legacy).is_ok() {
        crate::diagnostics::record_error("ipc", "legacy_resident_conflict");
        eprintln!(
            "corvo: an older Corvo resident is still running ({} answers). \
             Both register the same hotkey and toggles will misfire — \
             quit the older Corvo.",
            legacy.display()
        );
    }
}

/// Checks whether another process owns the resident slot and asks it to toggle.
#[cfg(unix)]
pub fn try_send_toggle() -> StartupDecision {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    // The current socket first; fall back to the legacy path so the
    // binary can still drive a pre-UID resident during upgrades.
    let mut decided = StartupDecision::StartResident;
    for path in [socket_path(), legacy_socket_path()] {
        if let Ok(mut stream) = UnixStream::connect(&path) {
            if stream.write_all(TOGGLE).is_ok() {
                return StartupDecision::ToggleExisting;
            }
        } else {
            decided = StartupDecision::StartResident;
        }
    }
    decided
}

/// Serves toggle requests on a background thread.
#[cfg(unix)]
pub fn serve(tx: smol::channel::Sender<()>) {
    use std::io::{ErrorKind, Read};
    use std::os::unix::net::UnixListener;

    warn_about_legacy_resident();
    let path = socket_path();
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            // The path is taken. When it answers, a resident is already
            // serving. When it does not, the file is stale, left behind
            // by a crashed resident, so take it over.
            if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                eprintln!("corvo: another resident owns {}", path.display());
                return;
            }
            let _ = std::fs::remove_file(&path);
            match UnixListener::bind(&path) {
                Ok(listener) => listener,
                Err(err) => {
                    crate::diagnostics::record_error("ipc", "socket_bind_failed");
                    eprintln!("corvo: cannot bind ipc socket at {}: {err}", path.display());
                    return;
                }
            }
        }
        Err(err) => {
            crate::diagnostics::record_error("ipc", "socket_bind_failed");
            eprintln!("corvo: cannot bind ipc socket at {}: {err}", path.display());
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 16];
            let Ok(n) = stream.read(&mut buf) else {
                continue;
            };
            if buf[..n].trim_ascii() == b"toggle" {
                let _ = tx.try_send(());
            }
        }
    });
}

#[cfg(windows)]
pub fn try_send_toggle() -> StartupDecision {
    use std::ffi::c_void;
    use std::io::Write;
    use std::ptr;

    const ERROR_ALREADY_EXISTS: u32 = 183;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_ABANDONED: u32 = 0x00000080;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(
            security_attributes: *mut c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> *mut c_void;
        fn GetLastError() -> u32;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    static INSTANCE_MUTEX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    if INSTANCE_MUTEX.get().is_some() {
        return StartupDecision::StartResident;
    }
    let mutex_name = "Local\\Corvo.SingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mutex = unsafe { CreateMutexW(ptr::null_mut(), 1, mutex_name.as_ptr()) };
    if mutex.is_null() {
        return StartupDecision::ExistingWithoutToggle;
    }
    let mutex_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let owns_mutex = !mutex_exists
        || matches!(
            unsafe { WaitForSingleObject(mutex, 0) },
            WAIT_OBJECT_0 | WAIT_ABANDONED
        );
    if owns_mutex {
        let _ = INSTANCE_MUTEX.set(mutex as usize);
        return StartupDecision::StartResident;
    }

    for attempt in 0..200 {
        match std::fs::OpenOptions::new()
            .write(true)
            .open(r"\\.\pipe\corvo")
        {
            Ok(mut pipe) => {
                if pipe.write_all(b"toggle\n").is_ok() {
                    unsafe {
                        let _ = CloseHandle(mutex);
                    }
                    return StartupDecision::ToggleExisting;
                }
                if attempt < 199 {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }
            Err(error) if matches!(error.raw_os_error(), Some(2 | 231)) && attempt < 199 => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
    unsafe {
        let _ = CloseHandle(mutex);
    }
    // The mutex owner can still be starting its named-pipe listener. Treat the
    // active mutex as authoritative so a concurrent launch never starts a copy.
    StartupDecision::ExistingWithoutToggle
}

#[cfg(windows)]
pub fn serve(tx: smol::channel::Sender<()>) {
    use std::ffi::c_void;
    use std::ptr;

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
    const PIPE_TYPE_BYTE: u32 = 0x00000000;
    const PIPE_READMODE_BYTE: u32 = 0x00000000;
    const PIPE_WAIT: u32 = 0x00000000;
    const ERROR_PIPE_CONNECTED: u32 = 535;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer_size: u32,
            in_buffer_size: u32,
            default_timeout: u32,
            security_attributes: *mut c_void,
        ) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        fn DisconnectNamedPipe(pipe: Handle) -> i32;
        fn ReadFile(
            file: Handle,
            buffer: *mut u8,
            bytes_to_read: u32,
            bytes_read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn PeekNamedPipe(
            pipe: Handle,
            buffer: *mut c_void,
            buffer_size: u32,
            bytes_read: *mut u32,
            bytes_available: *mut u32,
            bytes_left_this_message: *mut u32,
        ) -> i32;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    let pipe_name = r"\\.\pipe\corvo"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    std::thread::spawn(move || loop {
        let pipe = unsafe {
            CreateNamedPipeW(
                pipe_name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                255,
                0,
                64,
                0,
                ptr::null_mut(),
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            crate::diagnostics::record_error("ipc", "pipe_create_failed");
            std::thread::sleep(std::time::Duration::from_millis(250));
            continue;
        }

        let connected = unsafe { ConnectNamedPipe(pipe, ptr::null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected {
            let tx = tx.clone();
            let pipe = pipe as usize;
            std::thread::spawn(move || {
                let pipe = pipe as Handle;
                let mut buffer = [0u8; 64];
                let mut bytes_read = 0;
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
                let mut bytes_available = 0;
                let data_ready = loop {
                    let peeked = unsafe {
                        PeekNamedPipe(
                            pipe,
                            ptr::null_mut(),
                            0,
                            ptr::null_mut(),
                            &mut bytes_available,
                            ptr::null_mut(),
                        )
                    } != 0;
                    if !peeked || bytes_available > 0 || std::time::Instant::now() >= deadline {
                        break peeked && bytes_available > 0;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                };
                let read = data_ready
                    && unsafe {
                        ReadFile(
                            pipe,
                            buffer.as_mut_ptr(),
                            buffer.len() as u32,
                            &mut bytes_read,
                            ptr::null_mut(),
                        )
                    } != 0;
                if read && buffer[..bytes_read as usize].trim_ascii() == b"toggle" {
                    let _ = tx.try_send(());
                }
                unsafe {
                    let _ = DisconnectNamedPipe(pipe);
                    let _ = CloseHandle(pipe);
                }
            });
        } else {
            unsafe {
                let _ = CloseHandle(pipe);
            }
        }
    });
}
