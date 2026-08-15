//! EventKit calendar reads.

use crosspond_tools::{CalendarBackend, ToolError};

pub struct MacOsCalendar;

impl CalendarBackend for MacOsCalendar {
    fn events(
        &self,
        start_iso: &str,
        end_iso: &str,
        calendar_name: Option<&str>,
    ) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (start_iso, end_iso, calendar_name);
            return Err(ToolError::Failed(
                "calendar_events is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            events_macos(start_iso, end_iso, calendar_name)
        }
    }
}

#[cfg(target_os = "macos")]
fn events_macos(
    start_iso: &str,
    end_iso: &str,
    calendar_name: Option<&str>,
) -> Result<String, ToolError> {
    use objc2_event_kit::{EKEvent, EKEventStore};

    // SAFETY: EKEventStore is the documented Calendar entry point.
    let store = unsafe { EKEventStore::new() };
    ensure_access(&store)?;

    let start = parse_to_nsdate(start_iso)?;
    let end = parse_to_nsdate(end_iso)?;
    if end.timeIntervalSince1970() <= start.timeIntervalSince1970() {
        return Err(ToolError::Failed("end must be after start".into()));
    }

    let predicate =
        unsafe { store.predicateForEventsWithStartDate_endDate_calendars(&start, &end, None) };
    let events = unsafe { store.eventsMatchingPredicate(&predicate) };

    let needle = calendar_name
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty());

    let mut lines = Vec::new();
    let count = events.count();
    for index in 0..count {
        let event: objc2::rc::Retained<EKEvent> = events.objectAtIndex(index);
        let calendar = unsafe { event.calendar() }
            .map(|cal| unsafe { cal.title() }.to_string())
            .unwrap_or_else(|| "Calendar".into());
        if let Some(needle) = &needle {
            let lower = calendar.to_ascii_lowercase();
            if lower != *needle && !lower.contains(needle) {
                continue;
            }
        }
        lines.push(format_event(&event, &calendar));
    }
    lines.sort();
    if lines.is_empty() {
        Ok("(no events in that range)".into())
    } else if lines.len() > 100 {
        lines.truncate(100);
        lines.push("… truncated after 100 events".into());
        Ok(lines.join("\n"))
    } else {
        Ok(lines.join("\n"))
    }
}

#[cfg(target_os = "macos")]
fn ensure_access(store: &objc2_event_kit::EKEventStore) -> Result<(), ToolError> {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};
    use objc2_foundation::NSError;

    let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
    match status {
        EKAuthorizationStatus::FullAccess => Ok(()),
        EKAuthorizationStatus::Denied | EKAuthorizationStatus::Restricted => Err(denied()),
        _ => {
            let (tx, rx) = mpsc::channel();
            let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
                let _ = tx.send(granted.as_bool());
            });
            // SAFETY: block stays alive until recv returns; EventKit calls it once.
            unsafe {
                store.requestFullAccessToEventsWithCompletion(
                    std::ptr::from_ref(&*block).cast_mut(),
                );
            }
            match rx.recv_timeout(Duration::from_secs(120)) {
                Ok(true) => Ok(()),
                Ok(false) => Err(denied()),
                Err(_) => Err(ToolError::Failed(
                    "Calendar access prompt timed out. Enable Crosspond in System Settings → Privacy & Security → Calendars, then try again.".into(),
                )),
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn denied() -> ToolError {
    ToolError::Failed(
        "Calendar access is off. Enable Crosspond in System Settings → Privacy & Security → Calendars, then try again.".into(),
    )
}

#[cfg(target_os = "macos")]
fn parse_to_nsdate(raw: &str) -> Result<objc2::rc::Retained<objc2_foundation::NSDate>, ToolError> {
    use objc2_foundation::{NSISO8601DateFormatter, NSString};

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ToolError::Failed("date is required".into()));
    }

    let candidate = if trimmed.len() == 10
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(7) == Some(&b'-')
    {
        format!("{trimmed}T00:00:00Z")
    } else {
        trimmed.to_string()
    };

    let formatter = NSISO8601DateFormatter::new();
    let ns = NSString::from_str(&candidate);
    if let Some(date) = formatter.dateFromString(&ns) {
        return Ok(date);
    }

    let with_z = if !candidate.ends_with('Z') && !candidate.contains('+') {
        format!("{candidate}Z")
    } else {
        candidate
    };
    let ns = NSString::from_str(&with_z);
    if let Some(date) = formatter.dateFromString(&ns) {
        return Ok(date);
    }

    Err(ToolError::Failed(format!(
        "invalid date \"{raw}\"; use YYYY-MM-DD or RFC3339"
    )))
}

#[cfg(target_os = "macos")]
fn format_event(event: &objc2_event_kit::EKEvent, calendar: &str) -> String {
    let title = unsafe { event.title() }.to_string();
    let title = if title.is_empty() {
        "(untitled)".into()
    } else {
        title
    };
    let start = format_nsdate(unsafe { event.startDate() });
    let end = format_nsdate(unsafe { event.endDate() });
    let all_day = unsafe { event.isAllDay() };
    let location = unsafe { event.location() }
        .map(|loc| loc.to_string())
        .filter(|loc| !loc.is_empty());
    // Notes go to the model only — never log them in Crosspond.
    let notes = unsafe { event.notes() }
        .map(|notes| notes.to_string())
        .filter(|notes| !notes.is_empty());
    let mut line = if all_day {
        format!("- {start} (all day) {title} [{calendar}]")
    } else {
        format!("- {start} → {end} {title} [{calendar}]")
    };
    if let Some(location) = location {
        line.push_str(&format!(" @ {location}"));
    }
    if let Some(notes) = notes {
        let short: String = notes.chars().take(200).collect();
        if notes.chars().count() > 200 {
            line.push_str(&format!(" — {short}…"));
        } else {
            line.push_str(&format!(" — {short}"));
        }
    }
    line
}

#[cfg(target_os = "macos")]
fn format_nsdate(date: objc2::rc::Retained<objc2_foundation::NSDate>) -> String {
    let secs = date.timeIntervalSince1970();
    let Ok(duration) = std::time::Duration::try_from_secs_f64(secs.max(0.0)) else {
        return "(invalid time)".into();
    };
    let time = std::time::UNIX_EPOCH + duration;
    let secs_u = time
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    // SAFETY: localtime_r writes into our stack tm.
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let ptr = unsafe { libc::localtime_r(&secs_u, tm.as_mut_ptr()) };
    if ptr.is_null() {
        return "(invalid time)".into();
    }
    let tm = unsafe { tm.assume_init() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min
    )
}
