use crosspond_tools::{Screenshot, ScreenshotBackend, ToolError};

#[cfg(target_os = "macos")]
use crate::cua::CuaHost;

pub struct MacOsScreenshot {
    #[cfg(target_os = "macos")]
    host: CuaHost,
}

impl MacOsScreenshot {
    #[cfg(target_os = "macos")]
    pub(crate) fn with_host(host: CuaHost) -> Self {
        Self { host }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl ScreenshotBackend for MacOsScreenshot {
    fn capture(&self, pid: Option<i32>, app_name: Option<&str>) -> Result<Screenshot, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (pid, app_name);
            return Err(ToolError::Failed(
                "Screenshots are only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            self.host.capture(pid, app_name)
        }
    }

    fn click(&self, x: u32, y: u32) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (x, y);
            return Err(ToolError::Failed(
                "Clicks are only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            self.host.click_pixels(x, y)
        }
    }

    fn recapture(&self) -> Result<Screenshot, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(ToolError::Failed(
                "Screenshots are only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            self.host.capture_live()
        }
    }
}
