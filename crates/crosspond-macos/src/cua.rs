//! cua-driver MCP host for computer-use.
//!
//! Crosspond spawns `cua-driver mcp` as a child (`--direct` when available,
//! otherwise `--no-daemon-relaunch`) so Accessibility and Screen Recording stay
//! attributed to Crosspond.app. Coordinate conversion, window targeting, and
//! background clicks are cua-driver's job.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use crosspond_tools::{
    AxOutlineNode, MAX_AX_DEPTH, MAX_AX_NODES, Screenshot, ToolError, render_ax_outline,
    truncate_ax_text,
};
use serde_json::{Value, json};

use crate::context::CROSSPOND_BUNDLE_ID;

const RPC_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Clone)]
pub(crate) struct CuaHost {
    inner: Arc<CuaHostInner>,
}

struct CuaHostInner {
    session: Mutex<Option<McpSession>>,
    live: Mutex<Option<LiveState>>,
}

struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

#[derive(Clone)]
struct LiveState {
    pid: i32,
    app_name: String,
    window_id: u32,
    image_w: u32,
    image_h: u32,
    nodes: HashMap<u32, LiveNode>,
}

#[derive(Clone)]
struct LiveNode {
    token: Option<String>,
    label: String,
    secure: bool,
}

struct WindowRecord {
    id: u32,
    pid: i32,
    app_name: String,
    on_screen: bool,
    area: f64,
}

struct ParsedSnapshot {
    app_name: String,
    window_id: u32,
    truncated: bool,
    elements: Vec<CuaElement>,
    image: Option<CapturedImage>,
}

struct CuaElement {
    index: u32,
    token: Option<String>,
    role: String,
    label: String,
    value: Option<String>,
    parent_index: Option<u32>,
    focused: bool,
    enabled: Option<bool>,
}

#[derive(Clone)]
struct CapturedImage {
    bytes: Vec<u8>,
    media_type: String,
    width: u32,
    height: u32,
}

impl CuaHost {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(CuaHostInner {
                session: Mutex::new(None),
                live: Mutex::new(None),
            }),
        }
    }

    pub(crate) fn snapshot(
        &self,
        pid: Option<i32>,
        app_name: Option<&str>,
    ) -> Result<String, ToolError> {
        if !crate::ax::is_trusted() {
            return Err(not_trusted());
        }
        let (pid, name) = resolve_target(pid, app_name)?;
        let parsed = self.window_state(pid, &name, false)?;
        self.store_and_render(pid, parsed)
    }

    pub(crate) fn capture(
        &self,
        pid: Option<i32>,
        app_name: Option<&str>,
    ) -> Result<Screenshot, ToolError> {
        if !crate::ax::is_trusted() {
            return Err(not_trusted());
        }
        crate::tcc::ensure_screen_capture()?;
        let (pid, name) = resolve_target(pid, app_name)?;
        let parsed = self.window_state(pid, &name, true)?;
        let image = parsed
            .image
            .clone()
            .ok_or_else(|| ToolError::Failed("cua-driver returned no screenshot".into()))?;
        let app_name = parsed.app_name.clone();
        self.store_and_render(pid, parsed)?;
        Ok(Screenshot {
            bytes: image.bytes,
            media_type: image.media_type,
            width: image.width,
            height: image.height,
            app_name,
        })
    }

    pub(crate) fn click_pixels(&self, x: u32, y: u32) -> Result<String, ToolError> {
        crate::tcc::ensure_screen_capture()?;
        let live = self.live_state()?;
        if live.image_w == 0 || live.image_h == 0 {
            return Err(ToolError::Failed(
                "no screenshot yet. Call take_screenshot first.".into(),
            ));
        }
        if x >= live.image_w || y >= live.image_h {
            return Err(ToolError::Failed(format!(
                "click ({x}, {y}) is outside the screenshot ({}×{})",
                live.image_w, live.image_h
            )));
        }
        let arguments = json!({
            "pid": live.pid,
            "window_id": live.window_id,
            "x": x,
            "y": y,
            "delivery_mode": "background"
        });
        let result = self.call("click", arguments)?;
        tool_error(&result)?;
        Ok(format!("Clicked ({x}, {y}) in {}.", live.app_name))
    }

    pub(crate) fn press(&self, node_id: &str) -> Result<String, ToolError> {
        let id = parse_node_id(node_id)?;
        let live = self.live_state()?;
        let node = live.nodes.get(&id).cloned().ok_or_else(stale_node)?;
        self.click_element(&live, id, node.token.as_deref())?;
        std::thread::sleep(Duration::from_millis(50));
        let parsed = self.window_state(live.pid, &live.app_name, false)?;
        let tree = self.store_and_render(live.pid, parsed)?;
        Ok(format!("Pressed {}.\n\n{tree}", node.label))
    }

    pub(crate) fn set_value(&self, node_id: &str, value: &str) -> Result<String, ToolError> {
        let id = parse_node_id(node_id)?;
        let live = self.live_state()?;
        let node = live.nodes.get(&id).cloned().ok_or_else(stale_node)?;
        if node.secure {
            return Err(ToolError::Failed(
                "won't set a password field from the snapshot".into(),
            ));
        }
        let mut arguments = json!({
            "pid": live.pid,
            "window_id": live.window_id,
            "element_index": id,
            "value": value
        });
        if let Some(token) = &node.token {
            arguments["element_token"] = json!(token);
        }
        let result = self.call("set_value", arguments)?;
        tool_error(&result)?;
        std::thread::sleep(Duration::from_millis(50));
        let parsed = self.window_state(live.pid, &live.app_name, false)?;
        let tree = self.store_and_render(live.pid, parsed)?;
        Ok(format!("Set {}.\n\n{tree}", node.label))
    }

    pub(crate) fn describe_node(&self, node_id: &str) -> Option<String> {
        let id = node_id.parse().ok()?;
        let live = self.inner.live.lock().ok()?;
        live.as_ref()
            .and_then(|state| state.nodes.get(&id))
            .map(|node| node.label.clone())
    }

    pub(crate) fn is_secure_node(&self, node_id: &str) -> bool {
        let Ok(id) = node_id.parse() else {
            return false;
        };
        let Ok(live) = self.inner.live.lock() else {
            return false;
        };
        live.as_ref()
            .and_then(|state| state.nodes.get(&id))
            .is_some_and(|node| node.secure)
    }

    pub(crate) fn list_apps(&self) -> Result<String, ToolError> {
        let result = self.call("list_apps", json!({}))?;
        tool_error(&result)?;
        Ok(format_app_list(&result))
    }

    pub(crate) fn open_app(
        &self,
        name: Option<&str>,
        bundle_id: Option<&str>,
    ) -> Result<String, ToolError> {
        let mut arguments = json!({});
        if let Some(bundle_id) = bundle_id.filter(|value| !value.is_empty()) {
            arguments["bundle_id"] = json!(bundle_id);
        } else if let Some(name) = name.filter(|value| !value.is_empty()) {
            arguments["name"] = json!(name);
        } else {
            return Err(ToolError::Failed(
                "open_app requires name or bundle_id".into(),
            ));
        }
        let result = self.call("launch_app", arguments)?;
        tool_error(&result)?;
        let structured = structured_content(&result);
        let pid = json_i32(structured, "pid").unwrap_or(0);
        let app_name = json_string(structured, "name").unwrap_or_else(|| "app".into());
        let bundle = json_string(structured, "bundle_id").unwrap_or_default();
        if pid > 0 && pid != std::process::id() as i32 {
            Ok(format!(
                "Opened {app_name} ({bundle}, pid {pid}). Pass app=\"{app_name}\" (or the bundle id) on the next snapshot/screenshot."
            ))
        } else {
            Ok(format!("Opened {app_name} ({bundle})."))
        }
    }

    pub(crate) fn focus_app(
        &self,
        name: Option<&str>,
        bundle_id: Option<&str>,
    ) -> Result<String, ToolError> {
        let query = bundle_id
            .filter(|value| !value.is_empty())
            .or(name.filter(|value| !value.is_empty()))
            .ok_or_else(|| ToolError::Failed("focus_app requires name or bundle_id".into()))?;
        let (pid, app_name) = self.resolve_running_app(query)?;
        let result = self.call("bring_to_front", json!({ "pid": pid }))?;
        tool_error(&result)?;
        Ok(format!("Brought {app_name} (pid {pid}) to the front."))
    }

    pub(crate) fn resolve_running_app(&self, app: &str) -> Result<(i32, String), ToolError> {
        let app = app.trim();
        if app.is_empty() {
            return Err(ToolError::Failed("app is required".into()));
        }
        let result = self.call("list_apps", json!({}))?;
        tool_error(&result)?;
        let apps = structured_content(&result)
            .get("apps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let needle = app.to_ascii_lowercase();
        let mut exact: Option<(i32, String)> = None;
        let mut fuzzy: Option<(i32, String)> = None;
        for entry in apps {
            let running = entry
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !running {
                continue;
            }
            let pid = json_i32(&entry, "pid").unwrap_or(0);
            if pid <= 0 || pid == std::process::id() as i32 {
                continue;
            }
            let name = json_string(&entry, "name").unwrap_or_default();
            let bundle = json_string(&entry, "bundle_id").unwrap_or_default();
            if bundle.eq_ignore_ascii_case(CROSSPOND_BUNDLE_ID) {
                continue;
            }
            let name_l = name.to_ascii_lowercase();
            let bundle_l = bundle.to_ascii_lowercase();
            if name_l == needle || bundle_l == needle {
                exact = Some((pid, name));
                break;
            }
            if fuzzy.is_none()
                && (name_l.contains(&needle)
                    || bundle_l.contains(&needle)
                    || needle.contains(&name_l))
            {
                fuzzy = Some((pid, name));
            }
        }
        exact.or(fuzzy).ok_or_else(|| {
            ToolError::Failed(format!(
                "no running app matching \"{app}\". Call list_apps or open_app first."
            ))
        })
    }

    pub(crate) fn type_text(&self, text: &str, node_id: Option<&str>) -> Result<String, ToolError> {
        let live = self.live_state()?;
        let mut arguments = json!({
            "pid": live.pid,
            "window_id": live.window_id,
            "text": text,
            "delivery_mode": "background"
        });
        if let Some(node_id) = node_id {
            let id = parse_node_id(node_id)?;
            let node = live.nodes.get(&id).cloned().ok_or_else(stale_node)?;
            arguments["element_index"] = json!(id);
            if let Some(token) = &node.token {
                arguments["element_token"] = json!(token);
            }
        }
        let result = self.call("type_text", arguments)?;
        tool_error(&result)?;
        Ok(format!("Typed into {}.", live.app_name))
    }

    pub(crate) fn hotkey(&self, keys: &[String]) -> Result<String, ToolError> {
        if keys.len() < 2 {
            return Err(ToolError::Failed(
                "hotkey requires at least one modifier and one key".into(),
            ));
        }
        let live = self.live_state()?;
        let arguments = json!({
            "pid": live.pid,
            "window_id": live.window_id,
            "keys": keys,
            "delivery_mode": "background"
        });
        let result = self.call("hotkey", arguments)?;
        tool_error(&result)?;
        Ok(format!(
            "Sent hotkey {} in {}.",
            keys.join("+"),
            live.app_name
        ))
    }

    pub(crate) fn scroll(
        &self,
        direction: &str,
        amount: u32,
        by: &str,
        node_id: Option<&str>,
        x: Option<u32>,
        y: Option<u32>,
    ) -> Result<String, ToolError> {
        let live = self.live_state()?;
        let mut arguments = json!({
            "pid": live.pid,
            "window_id": live.window_id,
            "direction": direction,
            "amount": amount,
            "by": by,
            "delivery_mode": "background"
        });
        if let Some(node_id) = node_id {
            let id = parse_node_id(node_id)?;
            let node = live.nodes.get(&id).cloned().ok_or_else(stale_node)?;
            arguments["element_index"] = json!(id);
            if let Some(token) = &node.token {
                arguments["element_token"] = json!(token);
            }
        } else if let (Some(x), Some(y)) = (x, y) {
            arguments["x"] = json!(x);
            arguments["y"] = json!(y);
        }
        let result = self.call("scroll", arguments)?;
        tool_error(&result)?;
        Ok(format!("Scrolled {direction} in {}.", live.app_name))
    }

    fn click_element(
        &self,
        live: &LiveState,
        index: u32,
        token: Option<&str>,
    ) -> Result<(), ToolError> {
        let mut arguments = json!({
            "pid": live.pid,
            "window_id": live.window_id,
            "element_index": index,
            "delivery_mode": "background"
        });
        if let Some(token) = token {
            arguments["element_token"] = json!(token);
        }
        let result = self.call("click", arguments)?;
        tool_error(&result)
    }

    fn window_state(
        &self,
        pid: i32,
        app_name: &str,
        include_screenshot: bool,
    ) -> Result<ParsedSnapshot, ToolError> {
        let window = self.largest_window(pid, app_name)?;
        let arguments = json!({
            "pid": pid,
            "window_id": window.id,
            "include_screenshot": include_screenshot,
            "max_elements": MAX_AX_NODES,
            "max_depth": MAX_AX_DEPTH
        });
        let result = self.call("get_window_state", arguments)?;
        tool_error(&result)?;
        let mut parsed = parse_snapshot(&result, window.id, &window.app_name)?;
        if parsed.app_name.is_empty() {
            parsed.app_name = window.app_name;
        }
        Ok(parsed)
    }

    fn largest_window(&self, pid: i32, app_name: &str) -> Result<WindowRecord, ToolError> {
        let mut windows = self.list_windows(pid, true)?;
        if windows.is_empty() {
            windows = self.list_windows(pid, false)?;
        }
        windows
            .into_iter()
            .filter(|window| window.pid == pid)
            .max_by(|a, b| {
                a.on_screen.cmp(&b.on_screen).then(
                    a.area
                        .partial_cmp(&b.area)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            })
            .ok_or_else(|| {
                ToolError::Failed(format!("could not find an on-screen window for {app_name}"))
            })
    }

    fn list_windows(&self, pid: i32, on_screen_only: bool) -> Result<Vec<WindowRecord>, ToolError> {
        let result = self.call(
            "list_windows",
            json!({
                "pid": pid,
                "on_screen_only": on_screen_only
            }),
        )?;
        tool_error(&result)?;
        Ok(parse_windows(&result))
    }

    fn store_and_render(&self, pid: i32, parsed: ParsedSnapshot) -> Result<String, ToolError> {
        let (roots, nodes) = outline_from_elements(&parsed.elements);
        let text = render_ax_outline(&parsed.app_name, &roots, parsed.truncated);
        let mut live = self
            .inner
            .live
            .lock()
            .map_err(|_| ToolError::Failed("cua-driver state is unavailable".into()))?;
        let previous = live.take();
        let (image_w, image_h) = match &parsed.image {
            Some(image) => (image.width, image.height),
            None => previous
                .filter(|state| state.pid == pid && state.window_id == parsed.window_id)
                .map(|state| (state.image_w, state.image_h))
                .unwrap_or((0, 0)),
        };
        *live = Some(LiveState {
            pid,
            app_name: parsed.app_name,
            window_id: parsed.window_id,
            image_w,
            image_h,
            nodes,
        });
        Ok(text)
    }

    fn live_state(&self) -> Result<LiveState, ToolError> {
        let live = self
            .inner
            .live
            .lock()
            .map_err(|_| ToolError::Failed("cua-driver state is unavailable".into()))?;
        live.clone().ok_or_else(|| {
            ToolError::Failed(
                "no snapshot yet. Call get_accessibility_snapshot or take_screenshot first.".into(),
            )
        })
    }

    fn call(&self, name: &str, arguments: Value) -> Result<Value, ToolError> {
        let mut slot = self
            .inner
            .session
            .lock()
            .map_err(|_| ToolError::Failed("cua-driver state is unavailable".into()))?;
        if slot.as_mut().is_some_and(|session| !session.alive()) {
            *slot = None;
        }
        if slot.is_none() {
            *slot = Some(McpSession::spawn()?);
        }
        let session = slot
            .as_mut()
            .ok_or_else(|| ToolError::Failed("cua-driver failed to start".into()))?;
        match session.call(name, arguments) {
            Ok(value) => Ok(value),
            Err(error) => {
                *slot = None;
                Err(error)
            }
        }
    }
}

impl Drop for CuaHostInner {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.session.lock() {
            *slot = None;
        }
    }
}

impl McpSession {
    fn spawn() -> Result<Self, ToolError> {
        let binary = find_driver()?;
        let mut child = Command::new(&binary)
            .args(mcp_args(&binary))
            .env("CUA_DRIVER_EMBEDDED", "1")
            .env("CUA_DRIVER_HOST_BUNDLE_ID", CROSSPOND_BUNDLE_ID)
            .env("CUA_DRIVER_PERMISSION_MODE", "unrestricted")
            .env("CUA_DRIVER_DANGEROUSLY_BYPASS_APPROVALS", "1")
            .env("CUA_DRIVER_RS_MCP_NO_RELAUNCH", "1")
            .env("CUA_DRIVER_RS_PERMISSIONS_GATE", "0")
            .env("CUA_DRIVER_RS_TELEMETRY_ENABLED", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                ToolError::Failed(format!(
                    "could not start cua-driver at {}: {error}",
                    binary.display()
                ))
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::Failed("cua-driver stdin is unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::Failed("cua-driver stdout is unavailable".into()))?;
        let stdout = BufReader::new(stdout);
        set_read_timeout(stdout.get_ref(), RPC_TIMEOUT);
        let mut session = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        session.initialize()?;
        Ok(session)
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn initialize(&mut self) -> Result<(), ToolError> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "crosspond", "version": "0.0.1"}
            }
        }))?;
        let _ = self.read_response(id)?;
        self.write(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))?;
        Ok(())
    }

    fn call(&mut self, name: &str, arguments: Value) -> Result<Value, ToolError> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }))?;
        let message = self.read_response(id)?;
        if let Some(error) = message.get("error") {
            let text = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("cua-driver RPC error");
            return Err(ToolError::Failed(text.to_string()));
        }
        Ok(message.get("result").cloned().unwrap_or(Value::Null))
    }

    fn write(&mut self, message: &Value) -> Result<(), ToolError> {
        let mut line = serde_json::to_vec(message)
            .map_err(|_| ToolError::Failed("could not encode cua-driver request".into()))?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .and_then(|_| self.stdin.flush())
            .map_err(|_| ToolError::Failed("could not write to cua-driver".into()))
    }

    fn read_response(&mut self, id: u64) -> Result<Value, ToolError> {
        loop {
            let message = read_mcp_message(&mut self.stdout).map_err(rpc_io_error)?;
            if message_id_eq(&message, id) {
                return Ok(message);
            }
        }
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn find_driver() -> Result<PathBuf, ToolError> {
    if let Ok(path) = std::env::var("CUA_DRIVER_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Ok(output) = Command::new("/usr/bin/which").arg("cua-driver").output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() && Path::new(&path).is_file() {
            return Ok(PathBuf::from(path));
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    for candidate in [
        "/opt/homebrew/bin/cua-driver",
        "/usr/local/bin/cua-driver",
        "/opt/cua/bin/cua-driver",
        "/Applications/CuaDriver.app/Contents/MacOS/cua-driver",
    ] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    if !home.is_empty() {
        for suffix in [".local/bin/cua-driver", ".cua/bin/cua-driver"] {
            let path = PathBuf::from(&home).join(suffix);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(ToolError::Failed(
        "cua-driver is not installed. Install it from https://cua.ai/cua-driver (or set CUA_DRIVER_BIN), then try again.".into(),
    ))
}

fn mcp_args(binary: &Path) -> Vec<String> {
    let help = Command::new(binary)
        .args(["mcp", "--help"])
        .output()
        .ok()
        .map(|output| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .unwrap_or_default();
    mcp_args_from_help(&help)
}

fn mcp_args_from_help(help: &str) -> Vec<String> {
    let mut args = vec!["mcp".to_string()];
    if flag_listed(help, "--direct") {
        args.push("--direct".into());
        if flag_listed(help, "--embedded") {
            args.push("--embedded".into());
            if flag_listed(help, "--host-bundle-id") {
                args.push("--host-bundle-id".into());
                args.push(CROSSPOND_BUNDLE_ID.into());
            }
        }
        // cua-driver 0.20+ rejects serve-only authorization flags on `mcp`
        // (exit 64). Unrestricted mode is set via CUA_DRIVER_* env vars.
    } else if flag_listed(help, "--no-daemon-relaunch") {
        args.push("--no-daemon-relaunch".into());
    }
    if flag_listed(help, "--no-overlay") {
        args.push("--no-overlay".into());
    }
    args
}

fn flag_listed(help: &str, flag: &str) -> bool {
    help.split_whitespace()
        .any(|token| token.trim_end_matches([',', '.']) == flag)
}

fn message_id_eq(message: &Value, id: u64) -> bool {
    match message.get("id") {
        Some(Value::Number(number)) => number.as_u64() == Some(id),
        Some(Value::String(text)) => text.parse::<u64>().ok() == Some(id),
        _ => false,
    }
}

fn set_read_timeout(stdout: &ChildStdout, timeout: Duration) {
    use std::os::fd::AsRawFd;
    let fd = stdout.as_raw_fd();
    let tv = libc::timeval {
        tv_sec: timeout.as_secs() as libc::time_t,
        tv_usec: timeout.subsec_micros() as libc::suseconds_t,
    };
    // SAFETY: `fd` is the live stdout pipe of the cua-driver child.
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            std::ptr::addr_of!(tv).cast(),
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );
    }
}

fn read_mcp_message(stdout: &mut BufReader<ChildStdout>) -> std::io::Result<Value> {
    let mut header = String::new();
    stdout.read_line(&mut header)?;
    if header.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "cua-driver closed stdout",
        ));
    }
    if let Some(rest) = header.strip_prefix("Content-Length:") {
        let len = rest
            .trim()
            .parse::<usize>()
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        loop {
            header.clear();
            stdout.read_line(&mut header)?;
            if header.trim().is_empty() {
                break;
            }
        }
        let mut body = vec![0; len];
        stdout.read_exact(&mut body)?;
        return serde_json::from_slice(&body)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error));
    }
    serde_json::from_str(header.trim())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn rpc_io_error(error: std::io::Error) -> ToolError {
    if error.kind() == std::io::ErrorKind::TimedOut
        || error.raw_os_error() == Some(libc::EAGAIN)
        || error.raw_os_error() == Some(libc::EWOULDBLOCK)
    {
        return ToolError::Failed("cua-driver timed out".into());
    }
    ToolError::Failed(format!("could not read cua-driver: {error}"))
}

fn tool_error(result: &Value) -> Result<(), ToolError> {
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find_map(|part| {
                (part.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| part.get("text").and_then(Value::as_str))
                    .flatten()
            })
            .unwrap_or("cua-driver tool error");
        return Err(ToolError::Failed(text.to_string()));
    }
    Ok(())
}

fn parse_windows(result: &Value) -> Vec<WindowRecord> {
    structured_content(result)
        .get("windows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_window)
        .collect()
}

fn parse_window(value: &Value) -> Option<WindowRecord> {
    let id = json_u32(value, "window_id")?;
    let pid = json_i32(value, "pid").unwrap_or(0);
    let app_name = json_string(value, "app_name").unwrap_or_default();
    let on_screen = value
        .get("is_on_screen")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let bounds = value.get("bounds").unwrap_or(value);
    let width = json_f64(bounds, "width").unwrap_or(0.0);
    let height = json_f64(bounds, "height").unwrap_or(0.0);
    Some(WindowRecord {
        id,
        pid,
        app_name,
        on_screen,
        area: width * height,
    })
}

fn parse_snapshot(
    result: &Value,
    window_id: u32,
    fallback_name: &str,
) -> Result<ParsedSnapshot, ToolError> {
    let structured = structured_content(result).clone();
    let truncated = structured
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let elements = structured
        .get("elements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_element)
        .collect();
    let image = extract_image(result)?;
    Ok(ParsedSnapshot {
        app_name: json_string(&structured, "app_name").unwrap_or_else(|| fallback_name.to_string()),
        window_id: json_u32(&structured, "window_id").unwrap_or(window_id),
        truncated,
        elements,
        image,
    })
}

fn parse_element(value: &Value) -> Option<CuaElement> {
    let index = json_u32(value, "element_index")?;
    let role = json_string(value, "role").unwrap_or_else(|| "AXUnknown".into());
    let label = json_string(value, "label")
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| role.clone());
    Some(CuaElement {
        index,
        token: json_string(value, "element_token"),
        role,
        label,
        value: json_string(value, "value"),
        parent_index: json_u32(value, "parent_index"),
        focused: value
            .get("focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        enabled: value.get("enabled").and_then(Value::as_bool),
    })
}

fn extract_image(result: &Value) -> Result<Option<CapturedImage>, ToolError> {
    let Some(part) = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|part| part.get("type").and_then(Value::as_str) == Some("image"))
    else {
        return Ok(None);
    };
    let Some(data) = part.get("data").and_then(Value::as_str) else {
        return Ok(None);
    };
    let media_type = part
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or("image/png");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| ToolError::Failed("cua-driver screenshot was not valid base64".into()))?;
    let (width, height) = png_size(&bytes)
        .ok_or_else(|| ToolError::Failed("cua-driver screenshot had no pixel size".into()))?;
    Ok(Some(CapturedImage {
        bytes,
        media_type: media_type.to_string(),
        width,
        height,
    }))
}

fn structured_content(result: &Value) -> &Value {
    match result.get("structuredContent") {
        Some(value) if !value.is_null() => value,
        _ => result,
    }
}

fn outline_from_elements(elements: &[CuaElement]) -> (Vec<AxOutlineNode>, HashMap<u32, LiveNode>) {
    let mut live = HashMap::new();
    let mut nodes = HashMap::new();
    for element in elements {
        if skip_ax_role(&element.role) {
            continue;
        }
        let secure = is_secure_role(&element.role);
        let title = (!element.label.is_empty() && element.label != element.role)
            .then(|| truncate_ax_text(&element.label));
        let value = if secure {
            Some("••••".into())
        } else {
            element
                .value
                .as_deref()
                .filter(|value| !value.is_empty())
                .map(truncate_ax_text)
        };
        live.insert(
            element.index,
            LiveNode {
                token: element.token.clone(),
                label: title
                    .clone()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| element.role.clone()),
                secure,
            },
        );
        nodes.insert(
            element.index,
            AxOutlineNode {
                id: element.index,
                role: element.role.clone(),
                title,
                value,
                enabled: element.enabled,
                focused: element.focused,
                truncated_children: false,
                children: Vec::new(),
            },
        );
    }
    let mut children: HashMap<Option<u32>, Vec<u32>> = HashMap::new();
    for element in elements {
        if !nodes.contains_key(&element.index) {
            continue;
        }
        let parent = element
            .parent_index
            .filter(|parent| nodes.contains_key(parent));
        children.entry(parent).or_default().push(element.index);
    }
    fn attach(
        id: u32,
        nodes: &mut HashMap<u32, AxOutlineNode>,
        children: &HashMap<Option<u32>, Vec<u32>>,
    ) -> Option<AxOutlineNode> {
        let mut node = nodes.remove(&id)?;
        if let Some(child_ids) = children.get(&Some(id)) {
            for child_id in child_ids {
                if let Some(child) = attach(*child_id, nodes, children) {
                    node.children.push(child);
                }
            }
        }
        Some(node)
    }
    let roots = children
        .get(&None)
        .into_iter()
        .flatten()
        .filter_map(|id| attach(*id, &mut nodes, &children))
        .collect();
    (roots, live)
}

fn skip_ax_role(role: &str) -> bool {
    matches!(
        role,
        "AXMenuBar"
            | "AXMenu"
            | "AXMenuBarItem"
            | "AXMenuItem"
            | "AXScrollBar"
            | "AXDockItem"
            | "AXCloseButton"
            | "AXMinimizeButton"
            | "AXZoomButton"
            | "AXFullScreenButton"
            | "AXGrowArea"
    )
}

fn is_secure_role(role: &str) -> bool {
    role.contains("Secure") || role == "AXSecureTextField"
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() >= 24 && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some((width, height));
    }
    None
}

fn json_u32(value: &Value, key: &str) -> Option<u32> {
    match value.get(key)? {
        Value::Number(number) => {
            number
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .or_else(|| {
                    number
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.0)
                        .map(|n| n.round() as u32)
                })
        }
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn json_i32(value: &Value, key: &str) -> Option<i32> {
    match value.get(key)? {
        Value::Number(number) => {
            number
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .or_else(|| {
                    number
                        .as_f64()
                        .filter(|n| n.is_finite())
                        .map(|n| n.round() as i32)
                })
        }
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn json_f64(value: &Value, key: &str) -> Option<f64> {
    match value.get(key)? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse_node_id(node_id: &str) -> Result<u32, ToolError> {
    node_id
        .parse()
        .map_err(|_| ToolError::Failed("node_id must be a number".into()))
}

fn resolve_target(pid: Option<i32>, app_name: Option<&str>) -> Result<(i32, String), ToolError> {
    if let Some(pid) = pid
        && pid > 0
        && pid != std::process::id() as i32
    {
        let name = app_name
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("pid {pid}"));
        return Ok((pid, name));
    }
    let Some(app) = crate::context::frontmost_app() else {
        return Err(no_target());
    };
    if app.bundle_id == CROSSPOND_BUNDLE_ID || app.pid == std::process::id() as i32 {
        return Err(no_target());
    }
    Ok((app.pid, app.name))
}

fn no_target() -> ToolError {
    ToolError::Failed(
        "no target app. Call open_app or pass app= on the tool, or open another app and press Option+Space.".into(),
    )
}

fn format_app_list(result: &Value) -> String {
    let apps = structured_content(result)
        .get("apps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let name = json_string(entry, "name")?;
            let bundle = json_string(entry, "bundle_id").unwrap_or_default();
            if bundle.eq_ignore_ascii_case(CROSSPOND_BUNDLE_ID) {
                return None;
            }
            let running = entry
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let pid = json_i32(entry, "pid").unwrap_or(0);
            let active = entry
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut line = format!("{name} ({bundle})");
            if running && pid > 0 {
                line.push_str(&format!(" — running pid {pid}"));
                if active {
                    line.push_str(", frontmost");
                }
            } else {
                line.push_str(" — not running");
            }
            Some(line)
        })
        .collect::<Vec<_>>();
    if apps.is_empty() {
        "(no apps)".into()
    } else {
        apps.join("\n")
    }
}

fn not_trusted() -> ToolError {
    ToolError::Failed(
        "Accessibility is off. Enable Crosspond in System Settings → Privacy & Security → Accessibility, then try again.".into(),
    )
}

fn stale_node() -> ToolError {
    ToolError::Failed("stale or unknown node id. Call get_accessibility_snapshot again.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_png_size() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        png.extend_from_slice(&800u32.to_be_bytes());
        png.extend_from_slice(&600u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((800, 600)));
    }

    #[test]
    fn parses_window_area() {
        let window = parse_window(&json!({
            "window_id": 9,
            "pid": 42,
            "app_name": "Helium",
            "is_on_screen": true,
            "bounds": {"x": 0, "y": 0, "width": 100, "height": 20}
        }))
        .unwrap();
        assert_eq!(window.id, 9);
        assert_eq!(window.area, 2000.0);
    }

    #[test]
    fn outline_keeps_cua_element_ids() {
        let elements = vec![
            CuaElement {
                index: 4,
                token: Some("tok-4".into()),
                role: "AXButton".into(),
                label: "Continue".into(),
                value: None,
                parent_index: None,
                focused: false,
                enabled: Some(true),
            },
            CuaElement {
                index: 1,
                token: None,
                role: "AXCloseButton".into(),
                label: "Close".into(),
                value: None,
                parent_index: None,
                focused: false,
                enabled: None,
            },
        ];
        let (roots, live) = outline_from_elements(&elements);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, 4);
        assert_eq!(live.get(&4).unwrap().token.as_deref(), Some("tok-4"));
        assert!(!live.contains_key(&1));
    }

    #[test]
    fn mcp_direct_is_optional() {
        assert!(!flag_listed(
            "Usage: cua-driver mcp --no-daemon-relaunch",
            "--direct"
        ));
        assert!(flag_listed(
            "  --direct    Own the runtime\n  --embedded",
            "--direct"
        ));
    }

    #[test]
    fn mcp_args_prefer_direct_without_serve_authorization_flags() {
        let help = "\
mcp options:
  --direct
  --embedded
  --host-bundle-id <id>
agent authorization (serve only):
  --dangerously-bypass-approvals
  --no-overlay";
        let args = mcp_args_from_help(help);
        assert_eq!(
            args,
            vec![
                "mcp",
                "--direct",
                "--embedded",
                "--host-bundle-id",
                CROSSPOND_BUNDLE_ID,
                "--no-overlay",
            ]
        );
        assert!(
            !args
                .iter()
                .any(|arg| arg == "--dangerously-bypass-approvals")
        );
    }

    #[test]
    fn mcp_args_fall_back_to_no_daemon_relaunch() {
        assert_eq!(
            mcp_args_from_help("Usage: cua-driver mcp --no-daemon-relaunch"),
            vec!["mcp", "--no-daemon-relaunch"]
        );
    }
}
