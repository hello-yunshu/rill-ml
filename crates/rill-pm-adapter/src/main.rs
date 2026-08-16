//! `rill-pm-adapter` daemon entrypoint.
//!
//! Runs the `pm-rill-shadow` v1 decision host as a Unix domain socket
//! service that OpenWrt Performance Manager talks to. The adapter is
//! advisory-only: it never applies OpenWrt / UCI / sysctl / ethtool changes.
//!
//! Command-line surface intentionally matches the PM procd glue:
//! `--socket`, `--state-dir`, `--max-message`, `--timeout-ms`.
//! `--version` prints the adapter crate version (Gate 2 metadata smoke).
//!
//! The daemon is Unix-only by design (Unix domain socket service for
//! OpenWrt/Linux). On non-Unix hosts (e.g. the Windows CI matrix) the binary
//! compiles to a stub that reports the platform is unsupported; this keeps
//! `cargo test --workspace --all-targets` green on every CI runner while the
//! shipped release assets are the Unix/musl binaries PM actually consumes.

/// Unix-domain-socket daemon implementation. Compiled only on Unix-like
/// platforms; a non-Unix build produces a stub entrypoint instead.
#[cfg(unix)]
mod unix {
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use clap::Parser;

    use rill_pm_adapter::{
        AdapterConfig, AdapterState, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_TIMEOUT_MS, Frame,
        read_frame, write_frame,
    };

    /// Set by the SIGTERM / SIGINT handler to request a graceful shutdown of
    /// the accept loop. The adapter is a long-running daemon: it must not exit
    /// when a single client disconnects (EOF), only when the service manager
    /// asks it to stop.
    static SHUTDOWN: AtomicBool = AtomicBool::new(false);

    /// Async-signal-safe handler: only flips a flag (no allocation, no locks).
    extern "C" fn handle_signal(_sig: libc::c_int) {
        SHUTDOWN.store(true, Ordering::SeqCst);
    }

    /// Install SIGTERM / SIGINT handlers so the daemon can stop cleanly.
    fn install_signal_handlers() {
        // SAFETY: signal() is reentrant-safe for installing a handler that only
        // writes an atomic flag. The handler body called from the signal context
        // performs no allocation or locking.
        unsafe {
            libc::signal(
                libc::SIGTERM,
                handle_signal as *const () as libc::sighandler_t,
            );
            libc::signal(libc::SIGINT, handle_signal as *const () as libc::sighandler_t);
        }
    }

    /// CLI arguments (kept intentionally small and stable).
    #[derive(Debug, Parser)]
    #[command(
        name = "rill-pm-adapter",
        version,
        about = "pm-rill-shadow v1 decision adapter for OpenWrt Performance Manager (advisory-only)"
    )]
    struct Args {
        /// Unix domain socket path to listen on.
        #[arg(long, default_value = "/run/performance-manager/rill.sock")]
        socket: PathBuf,
        /// Directory used for bounded persisted state (JSON, temp+rename).
        #[arg(long, default_value = "/etc/performance-manager/rill")]
        state_dir: PathBuf,
        /// Maximum size in bytes of one newline-delimited JSON frame.
        #[arg(long, default_value_t = DEFAULT_MAX_MESSAGE_BYTES)]
        max_message: usize,
        /// Per-request processing timeout in milliseconds.
        #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
        timeout_ms: u64,
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Daemon entrypoint. Never returns.
    pub fn entry() -> ! {
        let args = Args::parse();
        if let Err(error) = run(args) {
            eprintln!("rill-pm-adapter: fatal: {error}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
        let config = AdapterConfig {
            socket: args.socket,
            state_dir: args.state_dir,
            max_message: args.max_message.max(1),
            timeout_ms: args.timeout_ms.max(1),
        };

        let state = Arc::new(std::sync::Mutex::new(AdapterState::from_disk(
            config.clone(),
            now_ms(),
        )?));

        // State directory must exist before the socket is placed (the state dir is
        // owned by the PM package; the adapter only reads/writes inside it).
        std::fs::create_dir_all(&config.state_dir)?;

        // Remove a stale socket file left by an unclean exit, then bind.
        if config.socket.exists() {
            std::fs::remove_file(&config.socket)?;
        }
        let listener = UnixListener::bind(&config.socket)?;
        listener.set_nonblocking(true)?;
        install_signal_handlers();

        loop {
            if SHUTDOWN.load(Ordering::SeqCst) {
                // Graceful stop requested (SIGTERM / SIGINT).
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || {
                        if let Err(error) = serve_connection(stream, state) {
                            eprintln!("rill-pm-adapter: connection error: {error}");
                        }
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(error) => {
                    eprintln!("rill-pm-adapter: accept error: {error}");
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        Ok(())
    }

    /// Serve one NDJSON connection until EOF or an oversized frame. An oversized
    /// frame fails closed: the connection is closed and nothing is parsed.
    fn serve_connection(
        mut stream: UnixStream,
        state: Arc<std::sync::Mutex<AdapterState>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let max_message = {
            let state = state.lock().expect("adapter state poisoned");
            state.max_message()
        };
        loop {
            match read_frame(&mut stream, max_message) {
                // On some platforms (e.g. macOS) an accepted UnixStream inherits
                // the listener's non-blocking flag, so a writer that has dropped
                // the connection surfaces as EAGAIN rather than a clean EOF.
                // Treat WouldBlock exactly like EOF: the peer is gone.
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
                Ok(Frame::Eof) => break,
                Ok(Frame::TooLarge) => {
                    // Fail closed: never parse an oversized frame.
                    break;
                }
                Ok(Frame::Line(raw)) => {
                    let mut state = state.lock().expect("adapter state poisoned");
                    let response = rill_pm_adapter::handle_request(&mut state, &raw, now_ms());
                    write_frame(&mut stream, &response)?;
                }
            }
        }
        Ok(())
    }
}

fn main() {
    #[cfg(unix)]
    {
        unix::entry();
    }
    #[cfg(not(unix))]
    {
        eprintln!(
            "rill-pm-adapter: this build supports Unix-like systems only \
             (OpenWrt / Linux / macOS / BSD); the current platform is not supported."
        );
        std::process::exit(1);
    }
}
