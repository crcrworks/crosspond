use std::path::Path;
use std::process::Command;

use crosspond_tools::{ShellSandbox, unsandboxed_shell_command};

pub struct MacOsShellSandbox;

impl ShellSandbox for MacOsShellSandbox {
    fn is_enforcing(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn prepare_command(&self, shell_command: &str, scratch: &Path) -> Command {
        #[cfg(target_os = "macos")]
        {
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command
                .arg("-p")
                .arg(seatbelt_profile(scratch))
                .arg("/bin/sh")
                .arg("-c")
                .arg(shell_command)
                .current_dir(scratch);
            command
        }
        #[cfg(not(target_os = "macos"))]
        {
            unsandboxed_shell_command(shell_command, scratch)
        }
    }
}

#[cfg(target_os = "macos")]
fn scheme_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
fn seatbelt_profile(scratch: &Path) -> String {
    let root = scheme_path(scratch);
    let work = scheme_path(&scratch.join("work"));
    format!(
        r#"(version 1)
(allow default)
(deny network*)
(deny network-inbound)
(deny network-outbound)
(deny network-bind)
(deny file-write*)
(allow file-write-data (literal "/dev/null"))
(allow file-write* (subpath "{root}") (subpath "{work}") (subpath "/private/tmp") (subpath "/tmp"))
"#
    )
}
