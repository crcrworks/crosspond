use crosspond_tools::{AppBackend, ToolError};

#[cfg(target_os = "macos")]
use crate::cua::CuaHost;

pub struct MacOsApps {
    #[cfg(target_os = "macos")]
    host: CuaHost,
}

impl MacOsApps {
    #[cfg(target_os = "macos")]
    pub(crate) fn with_host(host: CuaHost) -> Self {
        Self { host }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl AppBackend for MacOsApps {
    fn list_apps(&self) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(ToolError::Failed(
                "list_apps is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            self.host.list_apps()
        }
    }

    fn open_app(&self, name: Option<&str>, bundle_id: Option<&str>) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (name, bundle_id);
            return Err(ToolError::Failed(
                "open_app is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            self.host.open_app(name, bundle_id)
        }
    }

    fn focus_app(&self, name: Option<&str>, bundle_id: Option<&str>) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (name, bundle_id);
            return Err(ToolError::Failed(
                "focus_app is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            self.host.focus_app(name, bundle_id)
        }
    }

    fn resolve_running_app(&self, app: &str) -> Result<(i32, String), ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = app;
            return Err(ToolError::Failed(
                "resolve_running_app is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            self.host.resolve_running_app(app)
        }
    }
}
