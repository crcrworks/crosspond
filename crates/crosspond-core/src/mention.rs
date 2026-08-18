use serde::{Deserialize, Serialize};

/// User-attached composer mention. Payloads stay in Rust; the WebView only
/// sends kinds plus vault titles/ids and app names.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mention {
    Vault {
        #[serde(default)]
        note_id: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },
    Save,
    Later,
    Screen,
    App {
        name: String,
    },
    Files,
    Calendar,
    Web,
}

impl Mention {
    pub fn is_later(&self) -> bool {
        matches!(self, Self::Later)
    }

    pub fn is_screen(&self) -> bool {
        matches!(self, Self::Screen)
    }

    pub fn is_save(&self) -> bool {
        matches!(self, Self::Save)
    }

    pub fn vault_note_id(&self) -> Option<&str> {
        match self {
            Self::Vault {
                note_id: Some(id), ..
            } if !id.is_empty() => Some(id.as_str()),
            _ => None,
        }
    }

    pub fn app_name(&self) -> Option<&str> {
        match self {
            Self::App { name } if !name.trim().is_empty() => Some(name.trim()),
            _ => None,
        }
    }

    pub fn display_token(&self) -> String {
        match self {
            Self::Vault {
                title: Some(title), ..
            } if !title.trim().is_empty() => format!("@vault {title}"),
            Self::Vault { .. } => "@vault".into(),
            Self::Save => "@save".into(),
            Self::Later => "@later".into(),
            Self::Screen => "@screen".into(),
            Self::App { name } if !name.trim().is_empty() => format!("@app {name}"),
            Self::App { .. } => "@app".into(),
            Self::Files => "@files".into(),
            Self::Calendar => "@calendar".into(),
            Self::Web => "@web".into(),
        }
    }
}

pub fn display_prompt(prompt: &str, mentions: &[Mention]) -> String {
    let mut parts: Vec<String> = mentions.iter().map(Mention::display_token).collect();
    let trimmed = prompt.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }
    parts.join(" ")
}

pub fn vault_pin_ids(mentions: &[Mention]) -> Vec<String> {
    mentions
        .iter()
        .filter_map(Mention::vault_note_id)
        .map(str::to_string)
        .collect()
}

/// Routing block injected into the system prompt. Never includes screenshot
/// bytes, selected text, file paths, or note bodies.
pub fn mention_routing(mentions: &[Mention]) -> String {
    if mentions.is_empty() {
        return String::new();
    }
    let mut lines = vec!["User mentions for this turn (explicit; honor them):".to_string()];
    for mention in mentions {
        match mention {
            Mention::Vault {
                note_id: Some(id),
                title,
            } if !id.is_empty() => {
                let title = title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("note");
                lines.push(format!(
                    "- Pinned vault note {title} (id={id}). knowledge_read it before computer tools."
                ));
            }
            Mention::Vault { .. } => {
                lines.push(
                    "- Consult the Knowledge Vault (knowledge_search / knowledge_read) before acting."
                        .into(),
                );
            }
            Mention::Save => {
                lines.push(
                    "- Save this into the Knowledge Vault with knowledge_ingest (validated plan only; no secrets). Do not only answer in chat."
                        .into(),
                );
            }
            Mention::Later => {
                lines.push(
                    "- Save the current page, selection, PDF, or local document as an unread Source via knowledge_read_later."
                        .into(),
                );
            }
            Mention::Screen => {
                lines.push(
                    "- A screenshot of the user's frontmost window is attached. Look at that image before acting. Do not skip it."
                        .into(),
                );
            }
            Mention::App { name } if !name.trim().is_empty() => {
                lines.push(format!(
                    "- Target app is {}. Pass app=\"{}\" on snapshot, screenshot, and UI tools (or open_app / focus_app first).",
                    name.trim(),
                    name.trim()
                ));
            }
            Mention::App { .. } => {
                lines.push(
                    "- The user named a target app. Call list_apps / open_app and pass app= on computer tools."
                        .into(),
                );
            }
            Mention::Files => {
                lines.push(
                    "- Use the Finder selection already staged under input/ (read_file those paths). If none were staged, say so."
                        .into(),
                );
            }
            Mention::Calendar => {
                lines.push(
                    "- Use calendar_events (EventKit) for this request. Do not web_search personal plans and do not open Calendar.app unless asked to change the UI."
                        .into(),
                );
            }
            Mention::Web => {
                lines.push(
                    "- Use web_search / fetch_url. Do not answer from memory or the vault alone. Never put selected text, calendar details, passwords, or private file contents into a web_search query."
                        .into(),
                );
            }
        }
    }
    lines.join("\n")
}

pub fn model_user_text(prompt: &str, mentions: &[Mention]) -> String {
    let mut body = String::new();
    let routing = mention_routing(mentions);
    if !routing.is_empty() {
        body.push_str(&routing);
        body.push_str("\n\n");
    }
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        if mentions.iter().any(Mention::is_screen) {
            body.push_str("Look at the attached screen and continue.");
        } else if mentions.iter().any(Mention::is_later) {
            body.push_str("Save this for later.");
        } else if mentions.iter().any(Mention::is_save) {
            body.push_str("Save the current context to the vault.");
        } else {
            body.push_str("Follow the attached mentions.");
        }
    } else {
        body.push_str(trimmed);
    }
    body
}

/// Running-app lines from `list_apps` (`Name (bundle) — running pid N`).
pub fn parse_running_app_names(list_apps_text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in list_apps_text.lines() {
        if !line.contains("running pid") {
            continue;
        }
        let Some(name) = line.split(" (").next() else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.eq_ignore_ascii_case("Crosspond") {
            continue;
        }
        if !names.iter().any(|existing: &String| existing == name) {
            names.push(name.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_prompt_joins_tokens_and_text() {
        assert_eq!(
            display_prompt(
                "このダイアログ進めて",
                &[
                    Mention::Screen,
                    Mention::Vault {
                        note_id: Some("cp_vpn".into()),
                        title: Some("Lab VPN".into()),
                    },
                ]
            ),
            "@screen @vault Lab VPN このダイアログ進めて"
        );
        assert_eq!(display_prompt("   ", &[Mention::Screen]), "@screen");
        assert_eq!(display_prompt("hello", &[]), "hello");
    }

    #[test]
    fn mention_routing_omits_bodies_and_paths() {
        let text = mention_routing(&[
            Mention::Vault {
                note_id: Some("cp_vpn".into()),
                title: Some("Lab VPN".into()),
            },
            Mention::Screen,
            Mention::Save,
        ]);
        assert!(text.contains("id=cp_vpn"));
        assert!(text.contains("Lab VPN"));
        assert!(text.contains("screenshot"));
        assert!(text.contains("knowledge_ingest"));
        assert!(!text.contains('/'));
        assert!(!text.contains("secret-token"));
        assert!(!text.contains("password"));
    }

    #[test]
    fn parse_running_app_names_keeps_running_only() {
        let listed = "Safari (com.apple.Safari) — running pid 12, frontmost\nNotes (com.apple.Notes) — not running\nMail (com.apple.mail) — running pid 44\n";
        assert_eq!(
            parse_running_app_names(listed),
            vec!["Safari".to_string(), "Mail".to_string()]
        );
        assert!(
            parse_running_app_names("Crosspond (com.crosspond.app) — running pid 9\n").is_empty()
        );
    }

    #[test]
    fn serde_roundtrip_matches_ui_payload() {
        let raw = r#"[{"kind":"screen"},{"kind":"vault","note_id":"cp_1","title":"Lab VPN"},{"kind":"app","name":"Safari"}]"#;
        let mentions: Vec<Mention> = serde_json::from_str(raw).unwrap();
        assert_eq!(
            mentions,
            vec![
                Mention::Screen,
                Mention::Vault {
                    note_id: Some("cp_1".into()),
                    title: Some("Lab VPN".into()),
                },
                Mention::App {
                    name: "Safari".into()
                },
            ]
        );
        assert_eq!(vault_pin_ids(&mentions), vec!["cp_1"]);
    }
}
