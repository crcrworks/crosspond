use serde::Serialize;

/// Permission status for Settings. Do not prompt at first launch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PermissionSnapshot {
    pub accessibility: bool,
    pub screen_recording: bool,
    pub calendars: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Accessibility,
    ScreenRecording,
    Calendars,
}

impl PermissionSnapshot {
    pub fn current() -> Self {
        Self {
            accessibility: accessibility_granted(),
            screen_recording: screen_recording_granted(),
            calendars: calendars_granted(),
        }
    }
}

impl PermissionKind {
    pub fn settings_url(self) -> &'static str {
        match self {
            Self::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Self::ScreenRecording => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            Self::Calendars => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars"
            }
        }
    }
}

fn accessibility_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::ax::is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn screen_recording_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::tcc::screen_recording_granted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn calendars_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2_event_kit::{EKEntityType, EKEventStore};
        // SAFETY: class method; does not prompt and takes no pointers.
        let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
        status == objc2_event_kit::EKAuthorizationStatus::FullAccess
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
