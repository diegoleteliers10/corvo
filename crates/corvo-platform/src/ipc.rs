//! Single-instance IPC (SPEC §6). Unix domain socket on Linux and macOS.
//! Named pipe on Windows, phase 4.

#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
const TOGGLE: &[u8] = b"toggle\n";

#[cfg(unix)]
fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("corvo.sock")
}

/// Tries to toggle a resident instance. Returns `true` when one answered,
/// so the caller exits instead of starting a second resident.
#[cfg(unix)]
pub fn try_send_toggle() -> bool {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let Ok(mut stream) = UnixStream::connect(socket_path()) else {
        return false;
    };
    stream.write_all(TOGGLE).is_ok()
}

/// Serves toggle requests on a background thread.
#[cfg(unix)]
pub fn serve(tx: smol::channel::Sender<()>) {
    use std::io::{ErrorKind, Read};
    use std::os::unix::net::UnixListener;

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
                    eprintln!("corvo: cannot bind ipc socket at {}: {err}", path.display());
                    return;
                }
            }
        }
        Err(err) => {
            eprintln!("corvo: cannot bind ipc socket at {}: {err}", path.display());
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 16];
            let Ok(n) = stream.read(&mut buf) else { continue };
            if buf[..n].trim_ascii() == b"toggle" {
                // Unbounded channel, `try_send` never fails here.
                let _ = tx.try_send(());
            }
        }
    });
}

// TODO(named pipe server, phase 4): connect over `\\.\pipe\corvo`.
#[cfg(windows)]
pub fn try_send_toggle() -> bool {
    false
}

#[cfg(windows)]
pub fn serve(_tx: smol::channel::Sender<()>) {}
