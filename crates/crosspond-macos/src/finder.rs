use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const FINDER_TIMEOUT: Duration = Duration::from_millis(800);

const FINDER_SCRIPT: &str = r#"
with timeout of 1 seconds
  if application "Finder" is running then
    tell application "Finder"
      if (count of selection) is 0 then return ""
      set posixList to {}
      repeat with theItem in selection
        try
          set end of posixList to POSIX path of (theItem as alias)
        end try
      end repeat
      set AppleScript's text item delimiters to linefeed
      return posixList as text
    end tell
  end if
end timeout
"#;

pub fn selected_files() -> Vec<PathBuf> {
    let mut child = match Command::new("osascript")
        .arg("-e")
        .arg(FINDER_SCRIPT)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Vec::new(),
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Vec::new();
    };
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    match rx.recv_timeout(FINDER_TIMEOUT) {
        Ok(buf) => {
            let _ = child.wait();
            parse_posix_paths(&String::from_utf8_lossy(&buf))
        }
        Err(_) => {
            eprintln!("crosspond: Finder selection timed out");
            let _ = child.kill();
            let _ = child.wait();
            Vec::new()
        }
    }
}

pub fn parse_posix_paths(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .map(|line| line.trim().trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_finder_posix_paths() {
        let paths = parse_posix_paths("/Users/me/Desktop/a.txt\n/Users/me/Downloads/b.csv\n");
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("a.txt"));
        assert!(paths[1].ends_with("b.csv"));
    }

    #[test]
    fn empty_finder_output_is_empty() {
        assert!(parse_posix_paths("\n  \n").is_empty());
    }
}
