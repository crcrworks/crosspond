use crosspond_tools::{InputBackend, ToolError};

#[cfg(target_os = "macos")]
use crate::cua::CuaHost;

pub struct MacOsInput {
    #[cfg(target_os = "macos")]
    host: CuaHost,
}

impl MacOsInput {
    #[cfg(target_os = "macos")]
    pub(crate) fn with_host(host: CuaHost) -> Self {
        Self { host }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl InputBackend for MacOsInput {
    fn type_text(&self, text: &str, node_id: Option<&str>) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (text, node_id);
            return Err(ToolError::Failed(
                "ui_type is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            self.host.type_text(text, node_id)
        }
    }

    fn hotkey(&self, keys: &[String]) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = keys;
            return Err(ToolError::Failed(
                "ui_hotkey is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            self.host.hotkey(keys)
        }
    }

    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        by: &str,
        node_id: Option<&str>,
        x: Option<u32>,
        y: Option<u32>,
    ) -> Result<String, ToolError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (direction, amount, by, node_id, x, y);
            return Err(ToolError::Failed(
                "ui_scroll is only available on macOS".into(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            self.host.scroll(direction, amount, by, node_id, x, y)
        }
    }

    fn live_target_app(&self) -> Option<String> {
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
        #[cfg(target_os = "macos")]
        {
            self.host.live_target_app()
        }
    }
}
