use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crosspond_tools::ShellSandbox;
#[cfg(not(target_os = "macos"))]
use crosspond_tools::unsandboxed_shell_command;

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

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

/// Seatbelt matches the kernel path. Scratch often lives under a symlink
/// (`/var` → `/private/var`, `/tmp` → `/private/tmp`) or the Data firmlink
/// (`/Users` → `/System/Volumes/Data/Users`). Allow both spellings.
fn scratch_path_aliases(scratch: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    push_unique(&mut paths, scratch.to_path_buf());
    if let Ok(canon) = scratch.canonicalize() {
        push_unique(&mut paths, canon);
    }
    const PAIRS: &[(&str, &str)] = &[
        ("/tmp", "/private/tmp"),
        ("/var", "/private/var"),
        ("/etc", "/private/etc"),
        ("/Users", "/System/Volumes/Data/Users"),
    ];
    let seed = paths.clone();
    for path in seed {
        let text = path.to_string_lossy();
        for (from, to) in PAIRS {
            for (prefix, mapped) in [(from, to), (to, from)] {
                if let Some(rest) = text.strip_prefix(prefix)
                    && (rest.is_empty() || rest.starts_with('/'))
                {
                    push_unique(&mut paths, PathBuf::from(format!("{mapped}{rest}")));
                }
            }
        }
    }
    paths
}

fn ancestor_metadata_rules(roots: &[PathBuf]) -> String {
    let mut ancestors = BTreeSet::new();
    for root in roots {
        let mut current = root.clone();
        loop {
            let Some(parent) = current.parent() else {
                break;
            };
            if parent.as_os_str().is_empty() {
                break;
            }
            ancestors.insert(scheme_path(parent));
            if parent == Path::new("/") {
                break;
            }
            current = parent.to_path_buf();
        }
    }
    ancestors
        .iter()
        .map(|path| format!("(allow file-read-metadata (literal \"{path}\"))\n"))
        .collect()
}

fn scratch_subpath_filters(roots: &[PathBuf]) -> String {
    let mut out = String::new();
    for root in roots {
        let root_s = scheme_path(root);
        let work_s = scheme_path(&root.join("work"));
        out.push_str(&format!(
            "    (subpath \"{root_s}\")\n    (subpath \"{work_s}\")\n"
        ));
    }
    out
}

/// Auto-safe Seatbelt: scratch read/write, system binaries/libraries, no network,
/// and no clipboard / Keychain / Apple Events.
///
/// `(allow default)` keeps process/mach working; filesystem reads are deny-then-allow
/// so user home and other task temps stay out of Auto `run_command`. Clipboard,
/// Keychain, and Apple Event Mach names are denied after that. Code-signing trust
/// (`trustd` / `ocspd`) is left allowed so signed binaries can start. Clipboard
/// denies use the `com.apple.pasteboard*` prefix (`com.apple.pasteboard.1` on
/// current macOS), not only the legacy `com.apple.pboard` name.
///
/// macOS 26 dyld aborts with SIGABRT if it cannot `file-read*` the root inode
/// (`literal "/"`, not `subpath "/"`) or map executables.
fn seatbelt_profile(scratch: &Path) -> String {
    let roots = scratch_path_aliases(scratch);
    let subpaths = scratch_subpath_filters(&roots);
    let metadata = ancestor_metadata_rules(&roots);
    format!(
        r#"(version 1)
(allow default)
(deny network*)
(deny network-inbound)
(deny network-outbound)
(deny network-bind)
(deny file-write*)
(allow file-write-data
    (literal "/dev/null")
    (literal "/dev/dtracehelper")
)
(allow file-write*
{subpaths})
(deny file-read*)
(allow file-read* (literal "/"))
(allow file-read-metadata (vnode-type DIRECTORY))
(allow file-map-executable)
{metadata}(allow file-read*
    (subpath "/usr")
    (subpath "/bin")
    (subpath "/sbin")
    (subpath "/System")
    (subpath "/Library")
    (subpath "/dev")
    (subpath "/private/var/db")
    (subpath "/var/db")
    (subpath "/private/var/select")
    (subpath "/private/etc")
    (subpath "/etc")
{subpaths})
(deny appleevent-send)
(deny mach-lookup (global-name "com.apple.appleeventsd"))
(deny mach-lookup (global-name "com.apple.coreservices.appleevents"))
(deny mach-lookup (global-name "com.apple.ae.listener.register"))
(deny mach-lookup (global-name-prefix "com.apple.pasteboard"))
(deny mach-lookup (global-name-prefix "com.apple.pboard"))
(deny mach-lookup (global-name-prefix "com.apple.pbs"))
(deny mach-lookup (global-name-prefix "com.apple.coreservices.uasharedpasteboard"))
(deny mach-lookup (global-name-prefix "com.apple.coreservices.uauseractivitypasteboard"))
(deny mach-lookup (local-name "com.apple.CFPasteboardClient"))
(deny mach-lookup (xpc-service-name-prefix "com.apple.pasteboard"))
(deny mach-lookup (xpc-service-name-prefix "com.apple.pbs"))
(deny mach-lookup (global-name-prefix "com.apple.securityd"))
(deny mach-lookup (global-name-prefix "com.apple.secd"))
(deny mach-lookup (global-name "com.apple.SecurityServer"))
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn has_exact_subpath(profile: &str, path: &str) -> bool {
        profile.contains(&format!("(subpath \"{path}\")"))
    }

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
        assert!(profile.contains("(deny appleevent-send)"), "{profile}");
        assert!(
            profile.contains("(global-name-prefix \"com.apple.pasteboard\")"),
            "{profile}"
        );
        assert!(
            profile.contains("(global-name-prefix \"com.apple.securityd\")"),
            "{profile}"
        );
        assert!(
            !profile.contains("com.apple.trustd"),
            "denying trustd prevents signed binaries from starting:\n{profile}"
        );
        assert!(
            !profile.contains("com.apple.ocspd"),
            "denying ocspd can prevent process launch:\n{profile}"
        );
        assert!(
            has_exact_subpath(&profile, "/tmp/crosspond-scratch-profile"),
            "{profile}"
        );
        assert!(
            has_exact_subpath(&profile, "/tmp/crosspond-scratch-profile/work"),
            "{profile}"
        );
        assert!(
            has_exact_subpath(&profile, "/private/tmp/crosspond-scratch-profile"),
            "scratch under /tmp must also allow the /private/tmp path:\n{profile}"
        );
        assert!(
            profile.contains("(allow file-read-metadata (literal \"/tmp\"))"),
            "{profile}"
        );
        assert!(
            profile.contains("(allow file-read* (literal \"/\"))"),
            "macOS 26 dyld needs the root inode, not subpath /:\n{profile}"
        );
        assert!(
            !has_exact_subpath(&profile, "/"),
            "must not allow the whole filesystem via subpath /:\n{profile}"
        );
        assert!(profile.contains("(allow file-map-executable)"), "{profile}");
        assert!(
            profile.contains("(allow file-read-metadata (vnode-type DIRECTORY))"),
            "{profile}"
        );
        assert!(
            profile.contains("(allow file-read-metadata (literal \"/\"))"),
            "{profile}"
        );
        for system in ["/usr", "/bin", "/sbin", "/System", "/Library", "/dev"] {
            assert!(
                has_exact_subpath(&profile, system),
                "missing {system} in {profile}"
            );
        }
        assert!(
            !has_exact_subpath(&profile, "/Users"),
            "must not allow all of /Users:\n{profile}"
        );
        assert!(
            !has_exact_subpath(&profile, "/private/var/folders"),
            "must not allow all process temps:\n{profile}"
        );
        assert!(
            !has_exact_subpath(&profile, "/private/tmp"),
            "writes/reads must not include shared /tmp:\n{profile}"
        );
    }

    #[test]
    fn profile_allows_macos_symlink_and_firmlink_aliases() {
        let var_scratch = Path::new("/var/folders/zz/tmp/crosspond-scratch");
        let var_profile = seatbelt_profile(var_scratch);
        assert!(
            has_exact_subpath(&var_profile, "/var/folders/zz/tmp/crosspond-scratch"),
            "{var_profile}"
        );
        assert!(
            has_exact_subpath(
                &var_profile,
                "/private/var/folders/zz/tmp/crosspond-scratch"
            ),
            "{var_profile}"
        );
        assert!(
            var_profile.contains("(allow file-read-metadata (literal \"/private/var/folders\"))"),
            "{var_profile}"
        );
        assert!(
            !has_exact_subpath(&var_profile, "/private/var/folders"),
            "must not allow all of /private/var/folders:\n{var_profile}"
        );

        let home_scratch = Path::new("/Users/me/.crosspond/scratch/task");
        let home_profile = seatbelt_profile(home_scratch);
        assert!(
            has_exact_subpath(&home_profile, "/Users/me/.crosspond/scratch/task"),
            "{home_profile}"
        );
        assert!(
            has_exact_subpath(
                &home_profile,
                "/System/Volumes/Data/Users/me/.crosspond/scratch/task"
            ),
            "{home_profile}"
        );
        assert!(
            !has_exact_subpath(&home_profile, "/Users"),
            "must not allow all of /Users:\n{home_profile}"
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
    fn describe_output(output: &std::process::Output) -> String {
        format!(
            "status={} stdout={:?} stderr={:?}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
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
        let inside_file = scratch.join("inside.txt");
        std::fs::write(&inside_file, "scratch-ok").unwrap();

        let sandbox = MacOsShellSandbox;
        let echo = sandbox
            .prepare_command("/bin/echo seatbelt-bin", &scratch)
            .output()
            .expect("sandbox-exec");
        assert!(
            echo.status.success(),
            "system binary must run under the profile: {}",
            describe_output(&echo)
        );
        assert!(
            String::from_utf8_lossy(&echo.stdout).contains("seatbelt-bin"),
            "echo stdout missing: {}",
            describe_output(&echo)
        );

        let outside = sandbox
            .prepare_command(&format!("/bin/cat '{}'", secret.display()), &scratch)
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

        let inside_abs = sandbox
            .prepare_command(&format!("/bin/cat '{}'", inside_file.display()), &scratch)
            .output()
            .expect("sandbox-exec");
        assert!(
            inside_abs.status.success(),
            "scratch read (absolute) failed: {}",
            describe_output(&inside_abs)
        );
        assert_eq!(
            String::from_utf8_lossy(&inside_abs.stdout).trim(),
            "scratch-ok"
        );

        let inside_rel = sandbox
            .prepare_command("/bin/cat inside.txt", &scratch)
            .output()
            .expect("sandbox-exec");
        assert!(
            inside_rel.status.success(),
            "scratch read (relative) failed: {}",
            describe_output(&inside_rel)
        );
        assert_eq!(
            String::from_utf8_lossy(&inside_rel.stdout).trim(),
            "scratch-ok"
        );

        let write_ok = sandbox
            .prepare_command(
                "printf written > work/out.txt && /bin/cat work/out.txt",
                &scratch,
            )
            .output()
            .expect("sandbox-exec");
        assert!(
            write_ok.status.success(),
            "scratch write failed: {}",
            describe_output(&write_ok)
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

        let write_out = sandbox
            .prepare_command(
                &format!("printf leaked > '{}'", secret_dir.join("out.txt").display()),
                &scratch,
            )
            .output()
            .expect("sandbox-exec");
        assert!(
            !write_out.status.success() || !secret_dir.join("out.txt").exists(),
            "write outside scratch must fail"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    fn sandbox_output(command: &str, scratch: &Path) -> (bool, String) {
        let output = MacOsShellSandbox
            .prepare_command(command, scratch)
            .output()
            .expect("sandbox-exec");
        let text = [
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ]
        .join("\n");
        (output.status.success(), text)
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_blocks_clipboard_and_apple_events() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("crosspond-sb-ipc-{stamp}"));
        let scratch = root.join("scratch");
        std::fs::create_dir_all(scratch.join("work")).unwrap();

        let (echo_ok, echo_text) = sandbox_output("/bin/echo seatbelt-alive", &scratch);
        assert!(
            echo_ok,
            "benign command must run under IPC denies: {echo_text}"
        );
        assert!(
            echo_text.contains("seatbelt-alive"),
            "echo stdout missing: {echo_text}"
        );

        let secret = format!("CROSSPOND-CLIP-{stamp}");
        let _ = Command::new("/usr/bin/pbcopy")
            .current_dir(&scratch)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(secret.as_bytes())?;
                }
                child.wait()
            });

        let (paste_ok, paste_text) = sandbox_output("/usr/bin/pbpaste", &scratch);
        assert!(
            !paste_text.contains(&secret),
            "sandboxed pbpaste leaked clipboard: {paste_text}"
        );
        assert!(
            !paste_ok || paste_text.trim().is_empty(),
            "pbpaste must not return user clipboard: {paste_text}"
        );

        let (ae_ok, ae_text) = sandbox_output(
            r#"/usr/bin/osascript -e 'tell application "Finder" to get POSIX path of (path to home folder)'"#,
            &scratch,
        );
        assert!(
            !ae_text.contains("/Users/"),
            "sandboxed osascript leaked home: {ae_text}"
        );
        assert!(!ae_ok, "Apple Events must be denied: {ae_text}");

        let (sec_ok, sec_text) = sandbox_output(
            "/usr/bin/security find-generic-password -s com.crosspond.app.test -a missing",
            &scratch,
        );
        assert!(!sec_ok, "Keychain lookup must be denied: {sec_text}");
        assert!(
            !sec_text.to_ascii_lowercase().contains("password"),
            "security must not echo secrets: {sec_text}"
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
