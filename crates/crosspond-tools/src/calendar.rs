use std::sync::Arc;

use serde_json::{Value, json};

use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult, truncate_output};

/// Platform calendar reads. Implemented in `crosspond-macos` (EventKit).
pub trait CalendarBackend: Send + Sync {
    fn events(
        &self,
        start_iso: &str,
        end_iso: &str,
        calendar_name: Option<&str>,
    ) -> Result<String, ToolError>;
}

pub fn register_calendar_tools(registry: &mut ToolRegistry, backend: Arc<dyn CalendarBackend>) {
    registry.register(Arc::new(CalendarEvents {
        backend: Arc::clone(&backend),
    }));
}

fn optional_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn today_ymd() -> String {
    local_date_command("+%Y-%m-%d").unwrap_or_else(|| "1970-01-01".into())
}

fn day_after_ymd(ymd: &str) -> Result<String, ToolError> {
    let (year, month, day) = parse_ymd(ymd)?;
    let (y, m, d) = add_days(year, month, day, 1);
    Ok(format_ymd(y, m, d))
}

fn parse_ymd(raw: &str) -> Result<(i32, u32, u32), ToolError> {
    let trimmed = raw.trim();
    if trimmed.len() < 10 {
        return Err(ToolError::Failed(format!(
            "invalid date \"{raw}\"; use YYYY-MM-DD or RFC3339"
        )));
    }
    let year = trimmed[..4]
        .parse::<i32>()
        .map_err(|_| ToolError::Failed(format!("invalid year in \"{raw}\"")))?;
    let month = trimmed[5..7]
        .parse::<u32>()
        .map_err(|_| ToolError::Failed(format!("invalid month in \"{raw}\"")))?;
    let day = trimmed[8..10]
        .parse::<u32>()
        .map_err(|_| ToolError::Failed(format!("invalid day in \"{raw}\"")))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(ToolError::Failed(format!("invalid date \"{raw}\"")));
    }
    Ok((year, month, day))
}

fn format_ymd(year: i32, month: u32, day: u32) -> String {
    format!("{:04}-{:02}-{:02}", year, month, day)
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn add_days(year: i32, month: u32, day: u32, delta: i32) -> (i32, u32, u32) {
    let mut y = year;
    let mut m = month;
    let mut d = day as i32 + delta;
    while d > days_in_month(y, m) as i32 {
        d -= days_in_month(y, m) as i32;
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    while d < 1 {
        m -= 1;
        if m < 1 {
            m = 12;
            y -= 1;
        }
        d += days_in_month(y, m) as i32;
    }
    (y, m, d as u32)
}

fn local_date_command(format: &str) -> Option<String> {
    let output = std::process::Command::new("date")
        .arg(format)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn date_part(iso: &str) -> String {
    let trimmed = iso.trim();
    if trimmed.len() >= 10
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(7) == Some(&b'-')
    {
        trimmed[..10].to_string()
    } else {
        today_ymd()
    }
}

struct CalendarEvents {
    backend: Arc<dyn CalendarBackend>,
}

impl Tool for CalendarEvents {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "calendar_events".into(),
            description: "Read calendar events via EventKit. Prefer this over opening Calendar.app for schedule questions. Returns title, start, end, location, and notes.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "start": {
                        "type": "string",
                        "description": "Start of range as YYYY-MM-DD or RFC3339. Defaults to today (local)."
                    },
                    "end": {
                        "type": "string",
                        "description": "End of range (exclusive day boundary) as YYYY-MM-DD or RFC3339. Defaults to the day after start."
                    },
                    "calendar": {
                        "type": "string",
                        "description": "Optional calendar name filter"
                    }
                }
            }),
        }
    }

    fn execute(&self, _context: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let start = optional_string(&input, "start").unwrap_or_else(today_ymd);
        let end = match optional_string(&input, "end") {
            Some(end) => end,
            None => day_after_ymd(&date_part(&start))?,
        };
        let calendar = optional_string(&input, "calendar");
        let text = self.backend.events(&start, &end, calendar.as_deref())?;
        Ok(ToolResult {
            text: truncate_output(text),
            created_file: None,
            image: None,
        })
    }
}

#[cfg(test)]
pub(crate) struct MockCalendar;

#[cfg(test)]
impl CalendarBackend for MockCalendar {
    fn events(
        &self,
        start_iso: &str,
        end_iso: &str,
        calendar_name: Option<&str>,
    ) -> Result<String, ToolError> {
        let cal = calendar_name.unwrap_or("Work");
        Ok(format!("- {start_iso} → {end_iso} Standup [{cal}]"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;
    use uuid::Uuid;

    fn temp_workspace() -> Workspace {
        let root = std::env::temp_dir().join(format!("crosspond-cal-{}", Uuid::new_v4()));
        Workspace::create(root).unwrap()
    }

    #[test]
    fn calendar_events_with_mock() {
        let workspace = temp_workspace();
        let mut registry = ToolRegistry::new();
        register_calendar_tools(&mut registry, Arc::new(MockCalendar));
        let result = registry
            .execute(
                "calendar_events",
                &ToolContext::new(workspace.clone()),
                json!({"calendar": "Work"}),
            )
            .unwrap();
        assert!(result.text.contains("Standup"));
        assert!(result.text.contains("Work"));
        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn day_after_handles_month_boundary() {
        assert_eq!(day_after_ymd("2026-01-31").unwrap(), "2026-02-01");
    }
}
