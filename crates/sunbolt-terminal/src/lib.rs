use std::{
    env,
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use thiserror::Error;

/// Minimal terminal session states reserved for the terminal core boundary.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TerminalSessionState {
    Created,
}

/// Terminal viewport size in character cells.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

impl TerminalSize {
    /// Creates a terminal size in character cells.
    #[must_use]
    pub const fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }
}

impl From<TerminalSize> for PtySize {
    fn from(size: TerminalSize) -> Self {
        Self {
            rows: size.rows,
            cols: size.cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

/// Errors returned by local PTY operations.
#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("failed to open PTY: {0}")]
    OpenPty(#[source] anyhow::Error),
    #[error("failed to spawn shell: {0}")]
    Spawn(#[source] anyhow::Error),
    #[error("failed to clone PTY reader: {0}")]
    CloneReader(#[source] anyhow::Error),
    #[error("failed to take PTY writer: {0}")]
    TakeWriter(#[source] anyhow::Error),
    #[error("failed to write PTY input: {0}")]
    WriteInput(#[source] io::Error),
    #[error("failed to read PTY output: {0}")]
    ReadOutput(#[source] io::Error),
    #[error("failed to resize PTY: {0}")]
    Resize(#[source] anyhow::Error),
    #[error("failed to close PTY session: {0}")]
    Close(#[source] io::Error),
    #[error("PTY session is already closed")]
    Closed,
    #[error("default shell is not configured")]
    MissingDefaultShell,
    #[error("PTY {0} lock was poisoned")]
    LockPoisoned(&'static str),
}

/// Local PTY session for a shell running on the same host as Sunbolt.
pub struct LocalPtySession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
    reader: Mutex<Box<dyn Read + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    closed: AtomicBool,
}

impl LocalPtySession {
    /// Spawns the host user's default shell in a PTY.
    ///
    /// On Unix-like systems this uses the `SHELL` environment variable. On
    /// Windows it falls back to `COMSPEC`.
    ///
    /// # Errors
    ///
    /// Returns an error when no default shell is configured, or when the PTY,
    /// reader, writer, or shell process cannot be created.
    pub fn spawn_default_shell(size: TerminalSize) -> Result<Self, TerminalError> {
        let shell = default_shell().ok_or(TerminalError::MissingDefaultShell)?;
        Self::spawn_shell(shell, size)
    }

    /// Spawns a specific shell command in a PTY.
    ///
    /// # Errors
    ///
    /// Returns an error when the PTY cannot be opened, the command cannot be
    /// spawned, or the PTY reader/writer handles cannot be created.
    pub fn spawn_shell<S>(shell: S, size: TerminalSize) -> Result<Self, TerminalError>
    where
        S: Into<String>,
    {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(size.into())
            .map_err(TerminalError::OpenPty)?;

        let command = CommandBuilder::new(shell.into());
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(TerminalError::Spawn)?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(TerminalError::CloneReader)?;
        let writer = pair
            .master
            .take_writer()
            .map_err(TerminalError::TakeWriter)?;

        Ok(Self {
            master: Mutex::new(pair.master),
            child: Mutex::new(Some(child)),
            reader: Mutex::new(reader),
            writer: Mutex::new(writer),
            closed: AtomicBool::new(false),
        })
    }

    /// Writes input bytes to the PTY.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is closed, the writer lock is
    /// poisoned, or the PTY write/flush operation fails.
    pub fn write_input(&self, input: &[u8]) -> Result<(), TerminalError> {
        if self.is_closed() {
            return Err(TerminalError::Closed);
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| TerminalError::LockPoisoned("writer"))?;
        writer.write_all(input).map_err(TerminalError::WriteInput)?;
        writer.flush().map_err(TerminalError::WriteInput)
    }

    /// Reads output bytes from the PTY into the provided buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is closed, the reader lock is
    /// poisoned, or reading from the PTY fails.
    pub fn read_output(&self, output: &mut [u8]) -> Result<usize, TerminalError> {
        if self.is_closed() {
            return Err(TerminalError::Closed);
        }

        let mut reader = self
            .reader
            .lock()
            .map_err(|_| TerminalError::LockPoisoned("reader"))?;
        reader.read(output).map_err(TerminalError::ReadOutput)
    }

    /// Resizes the PTY viewport.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is closed or the PTY resize operation
    /// fails.
    pub fn resize(&self, size: TerminalSize) -> Result<(), TerminalError> {
        if self.is_closed() {
            return Err(TerminalError::Closed);
        }

        self.master
            .lock()
            .map_err(|_| TerminalError::LockPoisoned("master"))?
            .resize(size.into())
            .map_err(TerminalError::Resize)
    }

    /// Closes the PTY session by killing the child process.
    ///
    /// # Errors
    ///
    /// Returns an error when the child lock is poisoned or the child process
    /// cannot be killed. Calling this more than once succeeds.
    pub fn close(&self) -> Result<(), TerminalError> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let mut child = self
            .child
            .lock()
            .map_err(|_| TerminalError::LockPoisoned("child"))?;
        if let Some(mut child) = child.take() {
            child.kill().map_err(TerminalError::Close)?;
        }

        Ok(())
    }

    /// Returns true when `close` has been called.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

impl Drop for LocalPtySession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn default_shell() -> Option<String> {
    env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .or_else(|| env::var("COMSPEC").ok().filter(|shell| !shell.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::{
        default_shell, LocalPtySession, TerminalError, TerminalSessionState, TerminalSize,
    };
    use std::process::Command;
    #[cfg(unix)]
    use std::{
        fs, io,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::{Duration, Instant},
    };

    #[test]
    fn initial_state_is_created() {
        assert_eq!(TerminalSessionState::Created, TerminalSessionState::Created);
    }

    #[test]
    fn terminal_size_maps_to_pty_size() {
        let size = TerminalSize::new(120, 32);
        let pty_size: portable_pty::PtySize = size.into();

        assert_eq!(pty_size.cols, 120);
        assert_eq!(pty_size.rows, 32);
        assert_eq!(pty_size.pixel_width, 0);
        assert_eq!(pty_size.pixel_height, 0);
    }

    #[test]
    fn spawning_missing_shell_returns_error() {
        let result = LocalPtySession::spawn_shell(
            "/definitely/not/a/real/sunbolt/test/shell",
            TerminalSize::new(80, 24),
        );

        assert!(matches!(result, Err(TerminalError::Spawn(_))));
    }

    #[test]
    fn local_pty_can_resize_and_close() {
        let Some(shell) = test_shell() else {
            return;
        };

        let session = LocalPtySession::spawn_shell(shell, TerminalSize::new(80, 24))
            .expect("test shell should spawn in PTY");

        session
            .resize(TerminalSize::new(100, 30))
            .expect("PTY resize should succeed");
        session.close().expect("PTY close should succeed");
        assert!(session.is_closed());
        assert!(matches!(
            session.write_input(b"ignored"),
            Err(TerminalError::Closed)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn local_pty_can_write_input_and_read_output() {
        let script = TestScript::new(
            "sunbolt-pty-echo",
            "#!/bin/sh\nIFS= read -r line\nprintf 'sunbolt:%s\\n' \"$line\"\n",
        )
        .expect("test script should be created");

        let session = LocalPtySession::spawn_shell(script.path_string(), TerminalSize::new(80, 24))
            .expect("test script should spawn in PTY");

        session
            .write_input(b"input-check\n")
            .expect("PTY input should be written");

        let output = read_until(&session, "sunbolt:input-check", Duration::from_secs(3))
            .expect("expected PTY output should be read");

        assert!(output.contains("sunbolt:input-check"));
        session.close().expect("PTY close should succeed");
    }

    #[test]
    fn default_shell_is_present_when_environment_configures_one() {
        if let Some(shell) = default_shell() {
            assert!(!shell.is_empty());
        }
    }

    fn test_shell() -> Option<String> {
        if cfg!(windows) {
            return default_shell();
        }

        for candidate in ["/bin/sh", "/usr/bin/sh"] {
            if Command::new(candidate)
                .arg("-c")
                .arg("exit 0")
                .status()
                .is_ok()
            {
                return Some(candidate.to_owned());
            }
        }

        default_shell()
    }

    #[cfg(unix)]
    fn read_until(
        session: &LocalPtySession,
        expected: &str,
        timeout: Duration,
    ) -> io::Result<String> {
        let deadline = Instant::now() + timeout;
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 256];

        while Instant::now() < deadline {
            let read = session.read_output(&mut buffer).map_err(io::Error::other)?;
            bytes.extend_from_slice(&buffer[..read]);

            let output = String::from_utf8_lossy(&bytes);
            if output.contains(expected) {
                return Ok(output.into_owned());
            }
        }

        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "timed out waiting for PTY output",
        ))
    }

    #[cfg(unix)]
    struct TestScript {
        path: PathBuf,
    }

    #[cfg(unix)]
    impl TestScript {
        fn new(name: &str, contents: &str) -> io::Result<Self> {
            let path = std::env::temp_dir().join(format!("{}-{}", name, std::process::id()));
            fs::write(&path, contents)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            Ok(Self { path })
        }

        fn path_string(&self) -> String {
            self.path.to_string_lossy().into_owned()
        }
    }

    #[cfg(unix)]
    impl Drop for TestScript {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
