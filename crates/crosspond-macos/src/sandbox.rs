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

fn scheme_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

/// Auto-safe Seatbelt: scratch read/write, system binaries/libraries, no network.
///
/// `(allow default)` keeps process/mach working; filesystem reads are deny-then-allow
/// so user home and other task temps stay out of Auto `run_command`.
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
(allow file-write* (subpath "{root}") (subpath "{work}"))
(deny file-read*)
(allow file-read-metadata (literal "/"))
(allow file-read*
    (subpath "/usr")
    (subpath "/bin")
    (subpath "/sbin")
    (subpath "/System")
    (subpath "/Library")
    (subpath "/dev")
    (subpath "/private/var/db/dyld")
    (subpath "/private/var/db/timezone")
    (subpath "/private/etc")
    (subpath "/etc")
    (subpath "{root}")
    (subpath "{work}")
)
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn profile_confines_reads_to_system_and_scratch() {
        let scratch = Path::new("/tmp/crosspond-scratch-profile");
        let profile = seatbelt_profile(scratch);
        let deny_reads = profile
            .find("(deny file-read*)")
            .expect("profile must deny file-read");
        if let Some(allow_default) = profile.find("(allow default)") {
            assert!(
                deny_reads > allow_default,
                "file-read deny must override allow default:\n{profile}"
            );
        }
        assert!(profile.contains("(deny file-read*)"), "{profile}");
        assert!(profile.contains("(deny file-write*)"), "{profile}");
        assert!(profile.contains("(deny network*)"), "{profile}");
        assert!(
            profile.contains("(subpath \"/tmp/crosspond-scratch-profile\")"),
            "{profile}"
        );
        assert!(
            profile.contains("(subpath \"/tmp/crosspond-scratch-profile/work\")"),
            "{profile}"
        );
        for system in ["/usr", "/bin", "/sbin", "/System", "/Library", "/dev"] {
            assert!(
                profile.contains(&format!("(subpath \"{system}\")")),
                "missing {system} in {profile}"
            );
        }
        assert!(
            !profile.contains("(subpath \"/Users\")"),
            "must not allow all of /Users:\n{profile}"
        );
        assert!(
            !profile.contains("(subpath \"/private/var/folders\")"),
            "must not allow all process temps:\n{profile}"
        );
        assert!(
            !profile.contains("(subpath \"/private/tmp\")"),
            "writes/reads must not include shared /tmp:\n{profile}"
        );
    }

    #[test]
    fn linux_sandbox_is_not_auto_safe() {
        #[cfg(not(target_os = "macos"))]
        {
            assert!(!MacOsShellSandbox.is_enforcing());
        }
        #[cfg(target_os = "macos")]
        {
            assert!(MacOsShellSandbox.is_enforcing());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_blocks_reads_outside_scratch() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("crosspond-sb-{stamp}"));
        let scratch = root.join("scratch");
        let work = scratch.join("work");
        let secret_dir = root.join("home-secret");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&secret_dir).unwrap();
        let secret = secret_dir.join("id_rsa");
        std::fs::write(&secret, "FAKE-SSH-PRIVATE-KEY").unwrap();
        std::fs::write(scratch.join("inside.txt"), "scratch-ok").unwrap();

        let sandbox = MacOsShellSandbox;
        let outside = sandbox
            .prepare_command(&format!("cat '{}'", secret.display()), &scratch)
            .output()
            .expect("sandbox-exec");
        let outside_text = [
            String::from_utf8_lossy(&outside.stdout),
            String::from_utf8_lossy(&outside.stderr),
        ]
        .join("\n");
        assert!(
            !outside_text.contains("FAKE-SSH-PRIVATE-KEY"),
            "sandboxed cat leaked outside scratch: {outside_text}"
        );
        assert!(
            !outside.status.success(),
            "read outside scratch must fail: {outside_text}"
        );

        let inside = sandbox
            .prepare_command("cat inside.txt", &scratch)
            .output()
            .expect("sandbox-exec");
        assert!(
            inside.status.success(),
            "scratch read failed: {}",
            String::from_utf8_lossy(&inside.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&inside.stdout).trim(), "scratch-ok");

        let echo = sandbox
            .prepare_command("/bin/echo seatbelt-bin", &scratch)
            .output()
            .expect("sandbox-exec");
        assert!(
            echo.status.success(),
            "system binary failed: {}",
            String::from_utf8_lossy(&echo.stderr)
        );
        assert!(String::from_utf8_lossy(&echo.stdout).contains("seatbelt-bin"));

        let write_ok = sandbox
            .prepare_command(
                "printf written > work/out.txt && cat work/out.txt",
                &scratch,
            )
            .output()
            .expect("sandbox-exec");
        assert!(
            write_ok.status.success(),
            "scratch write failed: {}",
            String::from_utf8_lossy(&write_ok.stderr)
        );

        let net = sandbox
            .prepare_command(
                "/usr/bin/curl -s --max-time 2 https://example.invalid/",
                &scratch,
            )
            .output()
            .expect("sandbox-exec");
        let net_text = [
            String::from_utf8_lossy(&net.stdout),
            String::from_utf8_lossy(&net.stderr),
        ]
        .join("\n");
        assert!(!net.status.success(), "network must be denied: {net_text}");

        let _ = std::fs::remove_dir_all(root);
    }
}
