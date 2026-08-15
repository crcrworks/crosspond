//! Host-owned macOS TCC prompts. cua-driver in embedded/direct mode never prompts.

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

pub(crate) fn screen_recording_granted() -> bool {
    // SAFETY: this TCC helper takes no pointers and only queries permission.
    unsafe { CGPreflightScreenCaptureAccess() }
}

pub(crate) fn ensure_screen_capture() -> Result<(), crosspond_tools::ToolError> {
    if screen_recording_granted() {
        return Ok(());
    }
    // SAFETY: this TCC helper takes no pointers and only requests permission.
    let _ = unsafe { CGRequestScreenCaptureAccess() };
    if screen_recording_granted() {
        Ok(())
    } else {
        Err(crosspond_tools::ToolError::Failed(
            "Screen Recording is off. Enable Crosspond in System Settings → Privacy & Security → Screen Recording, then try again.".into(),
        ))
    }
}
