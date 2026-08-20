//! Chrome native-messaging bridge wired as a `BrowserBackend` transport.

use std::sync::Arc;

use crosspond_chrome_host::{
    BrowserBridge, default_socket_path, install_native_host_manifests, resolve_host_binary,
    spawn_bridge_server,
};
use crosspond_tools::{BrowserBackend, BrowserTransport, ExtensionBrowser, ToolError};
use serde_json::Value;

pub struct BridgeTransport(pub Arc<BrowserBridge>);

impl BrowserTransport for BridgeTransport {
    fn is_connected(&self) -> bool {
        self.0.is_connected()
    }

    fn call(&self, request: Value) -> Result<Value, ToolError> {
        self.0.call_json(request).map_err(ToolError::Failed)
    }
}

pub fn start_browser_backend() -> (Arc<BrowserBridge>, Arc<dyn BrowserBackend>) {
    let bridge = BrowserBridge::new();
    let _ = spawn_bridge_server(Arc::clone(&bridge), default_socket_path());
    if let Ok(exe) = std::env::current_exe() {
        if let Some(host) = resolve_host_binary(&exe) {
            if let Err(err) = install_native_host_manifests(&host) {
                eprintln!("crosspond: chrome native host manifest: {err}");
            }
        } else {
            eprintln!(
                "crosspond: crosspond-chrome-host binary not found; build the workspace so Chrome can launch it"
            );
        }
    }
    let backend: Arc<dyn BrowserBackend> = Arc::new(ExtensionBrowser::new(Arc::new(
        BridgeTransport(Arc::clone(&bridge)),
    )));
    (bridge, backend)
}
