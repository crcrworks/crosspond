use crosspond_tools::{AccessibilityBackend, ToolError};

#[cfg(target_os = "macos")]
use crate::cua::CuaHost;

pub struct MacOsAccessibility {
    #[cfg(target_os = "macos")]
    host: CuaHost,
}

impl MacOsAccessibility {
    #[cfg(target_os = "macos")]
    pub(crate) fn with_host(host: CuaHost) -> Self {
        Self { host }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl AccessibilityBackend for MacOsAccessibility {
    fn snapshot(&self, pid: Option<i32>, app_name: Option<&str>) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (pid, app_name);
            return Err(ToolError::Failed(
                "Accessibility is only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            self.host.snapshot(pid, app_name)
        }
    }

    fn press(&self, node_id: &str) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = node_id;
            return Err(ToolError::Failed(
                "Accessibility is only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            self.host.press(node_id)
        }
    }

    fn set_value(&self, node_id: &str, value: &str) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (node_id, value);
            return Err(ToolError::Failed(
                "Accessibility is only available on macOS".into(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            self.host.set_value(node_id, value)
        }
    }

    fn describe_node(&self, node_id: &str) -> Option<String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = node_id;
            None
        }
        #[cfg(target_os = "macos")]
        {
            self.host.describe_node(node_id)
        }
    }

    fn is_secure_node(&self, node_id: &str) -> bool {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = node_id;
            false
        }
        #[cfg(target_os = "macos")]
        {
            self.host.is_secure_node(node_id)
        }
    }
}
