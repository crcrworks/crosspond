//! Host-owned macOS TCC prompts. cua-driver in embedded/direct mode never prompts.

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

pub(crate) fn ensure_screen_capture() -> Result<(), crosspond_tools::ToolError> {
    // SAFETY: these TCC helpers take no pointers and only query/request permission.
    let trusted = unsafe { CGPreflightScreenCaptureAccess() };
    if trusted {
        return Ok(());
    }
    let _ = unsafe { CGRequestScreenCaptureAccess() };
    let trusted = unsafe { CGPreflightScreenCaptureAccess() };
    if trusted {
        Ok(())
    } else {
        Err(crosspond_tools::ToolError::Failed(
            "Screen Recording is off. Enable Crosspond in System Settings → Privacy & Security → Screen Recording, then try again.".into(),
        ))
    }
}
