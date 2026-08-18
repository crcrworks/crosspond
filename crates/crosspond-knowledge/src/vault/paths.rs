use std::path::{Component, Path, PathBuf};

use super::VaultError;
use crate::model::NoteKind;

const RESERVED_ROOT_FILES: &[&str] = &["Index.md", "Log.md", "Home.md"];

pub fn join_inside(root: &Path, relative: &Path) -> Result<PathBuf, VaultError> {
    let root = root
        .canonicalize()
        .map_err(|err| VaultError::Io(err.to_string()))?;
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(VaultError::PathEscape);
    }
    let mut current = root.clone();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => {
                if name.to_string_lossy().starts_with('.') {
                    return Err(VaultError::PathEscape);
                }
                current.push(name);
                if current.exists() {
                    let canonical = current
                        .canonicalize()
                        .map_err(|err| VaultError::Io(err.to_string()))?;
                    if !is_inside(&root, &canonical) {
                        return Err(VaultError::PathEscape);
                    }
                    current = canonical;
                } else if let Some(parent) = current.parent().filter(|parent| parent.exists()) {
                    let parent = parent
                        .canonicalize()
                        .map_err(|err| VaultError::Io(err.to_string()))?;
                    if !is_inside(&root, &parent) {
                        return Err(VaultError::PathEscape);
                    }
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(VaultError::PathEscape);
            }
        }
    }
    Ok(current)
}

pub fn is_inside(root: &Path, path: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let mut current = path;
    loop {
        if current == root {
            return true;
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => return false,
        }
    }
}

pub fn is_reserved_relative(relative: &Path) -> bool {
    let text = relative.to_string_lossy();
    if text.starts_with("_system/") || text == "_system" {
        return true;
    }
    RESERVED_ROOT_FILES
        .iter()
        .any(|name| relative == Path::new(name))
}

pub fn is_indexable_markdown(root: &Path, path: &Path) -> bool {
    if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
        return false;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    if is_reserved_relative(relative) {
        return false;
    }
    relative.components().all(|component| match component {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            !name.starts_with('.') && name != "_system"
        }
        Component::CurDir => true,
        _ => false,
    })
}

/// Characters Obsidian (and Windows) reject in filenames, plus wikilink syntax.
const ILLEGAL_FILENAME_CHARS: &[char] = &[
    '/', '\\', ':', '#', '|', '[', ']', '^', '*', '"', '<', '>', '?',
];

pub fn sanitize_filename(title: &str) -> Result<String, VaultError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(VaultError::Io("title is required".into()));
    }
    let mut name = String::new();
    for ch in trimmed.chars() {
        match ch {
            '\0' => name.push('-'),
            ch if ILLEGAL_FILENAME_CHARS.contains(&ch) => name.push('-'),
            ch if ch.is_control() => {}
            ch => name.push(ch),
        }
    }
    let name = name.trim().trim_end_matches('.');
    if name.is_empty() || name == "." || name == ".." {
        return Err(VaultError::Io("title is not a usable filename".into()));
    }
    Ok(name.to_string())
}

pub fn sanitize_relative_path(relative: &Path) -> Result<PathBuf, VaultError> {
    let mut out = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                if name.starts_with('.') {
                    return Err(VaultError::PathEscape);
                }
                let sanitized = if let Some(stem) = name.strip_suffix(".md") {
                    format!("{}.md", sanitize_filename(stem)?)
                } else {
                    sanitize_filename(&name)?
                };
                out.push(sanitized);
            }
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(VaultError::PathEscape);
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(VaultError::PathEscape);
    }
    Ok(out)
}

pub fn default_relative_path(
    kind: NoteKind,
    title: &str,
    now: &time::OffsetDateTime,
) -> Result<PathBuf, VaultError> {
    let file = format!("{}.md", sanitize_filename(title)?);
    if kind == NoteKind::Activity {
        let year = now.year();
        let month = u8::from(now.month());
        return Ok(PathBuf::from(format!("history/{year}/{month:02}/{file}")));
    }
    Ok(PathBuf::from(kind.default_dir()).join(file))
}

/// Obsidian resolves `[[wikilinks]]` by filename, not YAML `title`.
pub fn format_wikilink(title: &str, path: &Path) -> String {
    let target = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| wikilink_target_from_title(title));
    wikilink_with_display(&target, title)
}

pub fn format_wikilink_for_title(title: &str) -> String {
    wikilink_with_display(&wikilink_target_from_title(title), title)
}

fn wikilink_target_from_title(title: &str) -> String {
    sanitize_filename(title).unwrap_or_else(|_| "Untitled".into())
}

fn wikilink_with_display(target: &str, title: &str) -> String {
    let title = title.trim();
    if title == target || !wikilink_alias_ok(title) {
        format!("[[{target}]]")
    } else {
        format!("[[{target}|{title}]]")
    }
}

fn wikilink_alias_ok(title: &str) -> bool {
    !title.is_empty()
        && !title.contains('|')
        && !title.contains('[')
        && !title.contains(']')
        && !title.contains('\n')
        && !title.contains('\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_replaces_obsidian_illegal_chars() {
        assert_eq!(
            sanitize_filename("cordiverse/paper: Title").unwrap(),
            "cordiverse-paper- Title"
        );
        assert_eq!(sanitize_filename("Issue #12").unwrap(), "Issue -12");
        assert_eq!(sanitize_filename("Lab VPN").unwrap(), "Lab VPN");
    }

    #[test]
    fn wikilink_uses_filename_and_keeps_title_as_alias() {
        let path = Path::new("sources/cordiverse-paper- Title.md");
        assert_eq!(
            format_wikilink("cordiverse/paper: Title", path),
            "[[cordiverse-paper- Title|cordiverse/paper: Title]]"
        );
        assert_eq!(
            format_wikilink("Lab VPN", Path::new("resources/Lab VPN.md")),
            "[[Lab VPN]]"
        );
        assert_eq!(
            format_wikilink_for_title("cordiverse/paper: Title"),
            "[[cordiverse-paper- Title|cordiverse/paper: Title]]"
        );
    }
}
