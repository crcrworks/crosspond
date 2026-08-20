//! Host-provided shell confinement. This crate must not depend on macOS APIs.

use std::path::Path;
use std::process::Command;

/// Wraps `sh -c` so the host can confine the process (Seatbelt, etc.).
pub trait ShellSandbox: Send + Sync {
    /// True when Auto may run `run_command` without an Allow card.
    fn is_enforcing(&self) -> bool;

    /// Build the process that will run `shell_command` with cwd `scratch`.
    fn prepare_command(&self, shell_command: &str, scratch: &Path) -> Command;
}

/// No OS sandbox; the caller still isolates the environment.
pub struct UnsandboxedShell;

impl ShellSandbox for UnsandboxedShell {
    fn is_enforcing(&self) -> bool {
        false
    }

    fn prepare_command(&self, shell_command: &str, scratch: &Path) -> Command {
        unsandboxed_shell_command(shell_command, scratch)
    }
}

pub fn unsandboxed_shell() -> UnsandboxedShell {
    UnsandboxedShell
}

pub fn unsandboxed_shell_command(shell_command: &str, scratch: &Path) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(shell_command).current_dir(scratch);
    command
}
