use std::path::PathBuf;
use std::process::Command;

const FINDER_SCRIPT: &str = r#"
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
"#;

pub fn selected_files() -> Vec<PathBuf> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(FINDER_SCRIPT)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_posix_paths(&String::from_utf8_lossy(&output.stdout))
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
