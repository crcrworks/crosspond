//! Live `BrowserBackend` over a JSON transport to the Chrome extension.
//!
//! Snapshot rendering and ref tables live here so the MV3 extension can stay a
//! thin CDP relay.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::browser::{
    BrowserBackend, BrowserTransport, EXTENSION_DISCONNECTED, HttpAuthChallenge, host_from_url,
    http_auth_required_message, normalize_host,
};
use crate::browser_snapshot::{BoundRef, render_cdp_ax_tree};
use crate::tool::ToolError;

static EPOCH_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct Session {
    tab_id: Option<i64>,
    epoch: String,
    refs: HashMap<String, BoundRef>,
    url: String,
    title: String,
    http_auth: Option<HttpAuthChallenge>,
}

pub struct ExtensionBrowser {
    transport: Arc<dyn BrowserTransport>,
    session: Mutex<Session>,
}

impl ExtensionBrowser {
    pub fn new(transport: Arc<dyn BrowserTransport>) -> Self {
        Self {
            transport,
            session: Mutex::new(Session::default()),
        }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn require_connected(&self) -> Result<(), ToolError> {
        if self.transport.is_connected() {
            Ok(())
        } else {
            Err(ToolError::Failed(EXTENSION_DISCONNECTED.into()))
        }
    }

    fn request(&self, body: Value) -> Result<Value, ToolError> {
        self.require_connected()?;
        let response = self.transport.call(body)?;
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    fn cdp(&self, tab_id: i64, method: &str, params: Value) -> Result<Value, ToolError> {
        self.request(json!({
            "op": "cdp",
            "tabId": tab_id,
            "method": method,
            "params": params,
        }))
    }

    fn ensure_tab(&self) -> Result<i64, ToolError> {
        if let Some(id) = self.session().tab_id {
            return Ok(id);
        }
        let listed = self.request(json!({ "op": "list_tabs" }))?;
        let tab = pick_active_tab(&listed).ok_or_else(|| {
            ToolError::Failed("no Chromium tab is open. Call browser_new_tab first.".into())
        })?;
        let id = tab_id(&tab).ok_or_else(|| ToolError::Failed("tab id missing".into()))?;
        {
            let mut session = self.session();
            session.tab_id = Some(id);
            session.title = string_field(&tab, "title");
            session.url = string_field(&tab, "url");
        }
        let _ = self.request(json!({ "op": "attach", "tabId": id }));
        Ok(id)
    }

    fn lookup_ref(&self, element_ref: &str) -> Result<BoundRef, ToolError> {
        self.session()
            .refs
            .get(element_ref)
            .cloned()
            .ok_or_else(|| {
                ToolError::Failed("stale or unknown ref. Call browser_snapshot again.".into())
            })
    }

    fn capture_snapshot(&self) -> Result<String, ToolError> {
        let tab_id = self.ensure_tab()?;
        let _ = self.cdp(tab_id, "Accessibility.enable", json!({}))?;
        let tree = self.cdp(
            tab_id,
            "Accessibility.getFullAXTree",
            json!({ "depth": 20 }),
        )?;
        let (title, url) = self.tab_identity(tab_id)?;
        let epoch = next_epoch();
        let rendered =
            render_cdp_ax_tree(&title, &url, &epoch, &tree).map_err(ToolError::Failed)?;
        {
            let mut session = self.session();
            session.epoch = rendered.epoch.clone();
            session.refs = rendered
                .refs
                .iter()
                .cloned()
                .map(|bound| (bound.id.clone(), bound))
                .collect();
            session.title = title;
            session.url = url;
            session.tab_id = Some(tab_id);
        }
        Ok(rendered.text)
    }

    fn tab_identity(&self, tab_id: i64) -> Result<(String, String), ToolError> {
        let listed = self.request(json!({ "op": "list_tabs" }))?;
        if let Some(tab) = tabs_array(&listed)
            .into_iter()
            .find(|tab| self::tab_id(tab) == Some(tab_id))
        {
            Ok((string_field(&tab, "title"), string_field(&tab, "url")))
        } else {
            let session = self.session();
            Ok((session.title.clone(), session.url.clone()))
        }
    }

    fn with_action_snapshot(&self, line: String) -> Result<String, ToolError> {
        let snap = self.capture_snapshot()?;
        Ok(format!("{line}\n\n{snap}"))
    }

    fn click_backend(&self, tab_id: i64, backend_dom_node_id: i64) -> Result<(), ToolError> {
        let _ = self.cdp(
            tab_id,
            "DOM.scrollIntoViewIfNeeded",
            json!({ "backendNodeId": backend_dom_node_id }),
        );
        let quads = self.cdp(
            tab_id,
            "DOM.getContentQuads",
            json!({ "backendNodeId": backend_dom_node_id }),
        )?;
        let (x, y) = center_of_quads(&quads)?;
        self.cdp(
            tab_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
        )?;
        self.cdp(
            tab_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1
            }),
        )?;
        Ok(())
    }

    fn focus_backend(&self, tab_id: i64, backend_dom_node_id: i64) -> Result<(), ToolError> {
        self.cdp(
            tab_id,
            "DOM.focus",
            json!({ "backendNodeId": backend_dom_node_id }),
        )?;
        Ok(())
    }

    fn insert_text(&self, tab_id: i64, text: &str) -> Result<(), ToolError> {
        self.cdp(tab_id, "Input.insertText", json!({ "text": text }))?;
        Ok(())
    }

    fn select_all(&self, tab_id: i64) -> Result<(), ToolError> {
        dispatch_key(self, tab_id, "a", 4)
    }

    fn challenge_from_result(&self, result: &Value) -> Option<HttpAuthChallenge> {
        if result.get("http_auth_required").and_then(Value::as_bool) != Some(true)
            && result.get("pending").and_then(Value::as_bool) != Some(true)
        {
            return None;
        }
        let url = result
            .get("url")
            .and_then(Value::as_str)
            .or_else(|| result.get("origin").and_then(Value::as_str))
            .unwrap_or("");
        let host = host_from_url(url)
            .or_else(|| {
                result
                    .get("host")
                    .and_then(Value::as_str)
                    .map(normalize_host)
                    .filter(|host| !host.is_empty())
            })
            .unwrap_or_else(|| "this site".into());
        Some(HttpAuthChallenge {
            host,
            scheme: result
                .get("scheme")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            realm: result
                .get("realm")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }

    fn take_http_auth_message(&self, result: &Value) -> Option<String> {
        let challenge = self.challenge_from_result(result)?;
        {
            let mut session = self.session();
            session.http_auth = Some(challenge.clone());
        }
        Some(http_auth_required_message(
            &challenge.host,
            &challenge.scheme,
            &challenge.realm,
        ))
    }

    fn apply_navigation_result(&self, result: &Value, line: String) -> Result<String, ToolError> {
        {
            let mut session = self.session();
            if let Some(id) = result.get("tabId").and_then(Value::as_i64) {
                session.tab_id = Some(id);
            }
            session.refs.clear();
            session.epoch.clear();
            if let Some(url) = result.get("url").and_then(Value::as_str) {
                session.url = url.to_string();
            }
            if let Some(title) = result.get("title").and_then(Value::as_str) {
                session.title = title.to_string();
            }
        }
        if let Some(message) = self.take_http_auth_message(result) {
            return Ok(message);
        }
        self.session().http_auth = None;
        self.with_action_snapshot(line)
    }
}

impl BrowserBackend for ExtensionBrowser {
    fn connected(&self) -> bool {
        self.transport.is_connected()
    }

    fn current_host(&self) -> Option<String> {
        let url = self.session().url.clone();
        if !url.is_empty() {
            return host_from_url(&url);
        }
        if !self.transport.is_connected() {
            return None;
        }
        let listed = self.request(json!({ "op": "list_tabs" })).ok()?;
        let tab = pick_active_tab(&listed)?;
        host_from_url(&string_field(&tab, "url"))
    }

    fn tabs(&self) -> Result<String, ToolError> {
        let listed = self.request(json!({ "op": "list_tabs" }))?;
        let tabs = tabs_array(&listed);
        if tabs.is_empty() {
            return Ok("No open Chromium tabs.".into());
        }
        let mut lines = Vec::new();
        for (index, tab) in tabs.iter().enumerate() {
            let title = string_field(tab, "title");
            let url = string_field(tab, "url");
            let mut line = format!("{}. {title} — {url}", index + 1);
            if tab.get("active").and_then(Value::as_bool) == Some(true) {
                line.push_str(" (active)");
            }
            if tab.get("attached").and_then(Value::as_bool) == Some(true) {
                line.push_str(" (debugger)");
            }
            lines.push(line);
        }
        if let Some(tab) = pick_active_tab(&listed)
            && let Some(id) = tab_id(&tab)
        {
            let mut session = self.session();
            session.tab_id = Some(id);
            session.title = string_field(&tab, "title");
            session.url = string_field(&tab, "url");
        }
        Ok(lines.join("\n"))
    }

    fn snapshot(&self) -> Result<String, ToolError> {
        self.capture_snapshot()
    }

    fn text(&self) -> Result<String, ToolError> {
        let tab_id = self.ensure_tab()?;
        let value = self.cdp(
            tab_id,
            "Runtime.evaluate",
            json!({
                "expression": "document.body ? document.body.innerText : ''",
                "returnByValue": true
            }),
        )?;
        Ok(js_string(&value))
    }

    fn navigate(&self, action: &str, url: Option<&str>) -> Result<String, ToolError> {
        let tab_id = self.ensure_tab()?;
        let mut body = json!({ "op": "navigate", "tabId": tab_id, "action": action });
        if let Some(url) = url {
            body["url"] = json!(url);
        }
        let result = self.request(body)?;
        if let Some(url) = url
            && result.get("url").and_then(Value::as_str).is_none()
        {
            self.session().url = url.to_string();
        }
        self.apply_navigation_result(&result, format!("Navigated {action}"))
    }

    fn click(&self, element_ref: &str) -> Result<String, ToolError> {
        let bound = self.lookup_ref(element_ref)?;
        let tab_id = self.ensure_tab()?;
        self.click_backend(tab_id, bound.backend_dom_node_id)?;
        self.with_action_snapshot(format!("Clicked {element_ref}."))
    }

    fn type_text(&self, element_ref: &str, text: &str) -> Result<String, ToolError> {
        let bound = self.lookup_ref(element_ref)?;
        let tab_id = self.ensure_tab()?;
        self.focus_backend(tab_id, bound.backend_dom_node_id)?;
        self.insert_text(tab_id, text)?;
        self.with_action_snapshot(format!("Typed into {element_ref}."))
    }

    fn fill(&self, element_ref: &str, text: &str) -> Result<String, ToolError> {
        let bound = self.lookup_ref(element_ref)?;
        let tab_id = self.ensure_tab()?;
        self.focus_backend(tab_id, bound.backend_dom_node_id)?;
        self.select_all(tab_id)?;
        self.insert_text(tab_id, text)?;
        self.with_action_snapshot(format!("Filled {element_ref}."))
    }

    fn press_key(&self, key: &str) -> Result<String, ToolError> {
        let tab_id = self.ensure_tab()?;
        press_key_combo(self, tab_id, key)?;
        self.with_action_snapshot(format!("Pressed {key}."))
    }

    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        element_ref: Option<&str>,
    ) -> Result<String, ToolError> {
        let tab_id = self.ensure_tab()?;
        let mut x = 0.0;
        let mut y = 0.0;
        if let Some(element_ref) = element_ref {
            let bound = self.lookup_ref(element_ref)?;
            let _ = self.cdp(
                tab_id,
                "DOM.scrollIntoViewIfNeeded",
                json!({ "backendNodeId": bound.backend_dom_node_id }),
            );
            if let Ok(quads) = self.cdp(
                tab_id,
                "DOM.getContentQuads",
                json!({ "backendNodeId": bound.backend_dom_node_id }),
            ) && let Ok((cx, cy)) = center_of_quads(&quads)
            {
                x = cx;
                y = cy;
            }
        }
        let delta = 80.0 * f64::from(amount.max(1));
        let (delta_x, delta_y) = match direction {
            "up" => (0.0, -delta),
            "down" => (0.0, delta),
            "left" => (-delta, 0.0),
            "right" => (delta, 0.0),
            _ => (0.0, delta),
        };
        self.cdp(
            tab_id,
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseWheel",
                "x": x,
                "y": y,
                "deltaX": delta_x,
                "deltaY": delta_y
            }),
        )?;
        self.with_action_snapshot(format!("Scrolled {direction} {amount}."))
    }

    fn select_option(&self, element_ref: &str, value: &str) -> Result<String, ToolError> {
        let bound = self.lookup_ref(element_ref)?;
        let tab_id = self.ensure_tab()?;
        let resolved = self.cdp(
            tab_id,
            "DOM.resolveNode",
            json!({ "backendNodeId": bound.backend_dom_node_id }),
        )?;
        let object_id = resolved
            .pointer("/object/objectId")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("could not resolve select node".into()))?;
        self.cdp(
            tab_id,
            "Runtime.callFunctionOn",
            json!({
                "objectId": object_id,
                "functionDeclaration": "function(v){ if(!this) return false; this.value=v; this.dispatchEvent(new Event('input',{bubbles:true})); this.dispatchEvent(new Event('change',{bubbles:true})); return true; }",
                "arguments": [{ "value": value }],
                "returnByValue": true
            }),
        )?;
        self.with_action_snapshot(format!("Selected an option in {element_ref}."))
    }

    fn new_tab(&self, url: Option<&str>) -> Result<String, ToolError> {
        let mut body = json!({ "op": "new_tab" });
        if let Some(url) = url {
            body["url"] = json!(url);
        }
        let result = self.request(body)?;
        if result.get("tabId").and_then(Value::as_i64).is_none() {
            return Err(ToolError::Failed("new tab did not return a tab id".into()));
        }
        self.apply_navigation_result(
            &result,
            format!("Opened tab {}", url.unwrap_or("about:blank")),
        )
    }

    fn pending_http_auth(&self) -> Option<HttpAuthChallenge> {
        if let Some(challenge) = self.session().http_auth.clone() {
            return Some(challenge);
        }
        let tab_id = self.session().tab_id?;
        let result = self
            .request(json!({ "op": "pending_http_auth", "tabId": tab_id }))
            .ok()?;
        self.take_http_auth_message(&result)?;
        self.session().http_auth.clone()
    }

    fn continue_http_auth(&self, username: &str, password: &str) -> Result<String, ToolError> {
        let tab_id = self.ensure_tab()?;
        let result = self.request(json!({
            "op": "continue_http_auth",
            "tabId": tab_id,
            "username": username,
            "password": password,
        }))?;
        if let Some(message) = self.take_http_auth_message(&result) {
            return Ok(message);
        }
        self.session().http_auth = None;
        self.with_action_snapshot("Filled HTTP authentication. Values were not returned.".into())
    }

    fn describe_ref(&self, element_ref: &str) -> Option<String> {
        self.session().refs.get(element_ref).map(|bound| {
            let _redact_value = bound.secret;
            if bound.name.is_empty() {
                bound.role.clone()
            } else {
                bound.name.clone()
            }
        })
    }
}

fn next_epoch() -> String {
    let n = EPOCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{n:x}")
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn tab_id(value: &Value) -> Option<i64> {
    value
        .get("id")
        .and_then(Value::as_i64)
        .or_else(|| value.get("tabId").and_then(Value::as_i64))
}

fn tabs_array(listed: &Value) -> Vec<Value> {
    listed
        .get("tabs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn pick_active_tab(listed: &Value) -> Option<Value> {
    let tabs = tabs_array(listed);
    tabs.iter()
        .find(|tab| tab.get("active").and_then(Value::as_bool) == Some(true))
        .cloned()
        .or_else(|| tabs.first().cloned())
}

fn center_of_quads(value: &Value) -> Result<(f64, f64), ToolError> {
    let quads = value
        .get("quads")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Failed("no content quads for ref".into()))?;
    let first = quads
        .first()
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Failed("no content quads for ref".into()))?;
    let nums: Vec<f64> = first.iter().filter_map(Value::as_f64).collect();
    if nums.len() < 8 {
        return Err(ToolError::Failed("no content quads for ref".into()));
    }
    let x = (nums[0] + nums[2] + nums[4] + nums[6]) / 4.0;
    let y = (nums[1] + nums[3] + nums[5] + nums[7]) / 4.0;
    Ok((x, y))
}

fn js_string(value: &Value) -> String {
    value
        .pointer("/result/value")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn dispatch_key(
    browser: &ExtensionBrowser,
    tab_id: i64,
    key: &str,
    modifiers: i64,
) -> Result<(), ToolError> {
    let (code, vk) = key_code(key);
    for event_type in ["keyDown", "keyUp"] {
        browser.cdp(
            tab_id,
            "Input.dispatchKeyEvent",
            json!({
                "type": event_type,
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": vk,
                "nativeVirtualKeyCode": vk,
                "modifiers": modifiers
            }),
        )?;
    }
    Ok(())
}

fn press_key_combo(browser: &ExtensionBrowser, tab_id: i64, spec: &str) -> Result<(), ToolError> {
    let mut modifiers = 0i64;
    let mut key = String::from("Enter");
    let mut saw_key = false;
    for part in spec.split('+') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "meta" | "cmd" | "command" => modifiers |= 4,
            "ctrl" | "control" => modifiers |= 2,
            "alt" | "option" => modifiers |= 1,
            "shift" => modifiers |= 8,
            other => {
                key = canonicalize_key(other);
                saw_key = true;
            }
        }
    }
    if !saw_key {
        key = canonicalize_key(spec.trim());
    }
    dispatch_key(browser, tab_id, &key, modifiers)
}

fn canonicalize_key(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => "Enter".into(),
        "tab" => "Tab".into(),
        "esc" | "escape" => "Escape".into(),
        "backspace" => "Backspace".into(),
        "space" => " ".into(),
        "arrowdown" | "down" => "ArrowDown".into(),
        "arrowup" | "up" => "ArrowUp".into(),
        "arrowleft" | "left" => "ArrowLeft".into(),
        "arrowright" | "right" => "ArrowRight".into(),
        other if other.len() == 1 => other.to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => other.to_string(),
            }
        }
    }
}

fn key_code(key: &str) -> (&'static str, i64) {
    match key {
        "Enter" => ("Enter", 13),
        "Tab" => ("Tab", 9),
        "Escape" => ("Escape", 27),
        "Backspace" => ("Backspace", 8),
        " " => ("Space", 32),
        "ArrowDown" => ("ArrowDown", 40),
        "ArrowUp" => ("ArrowUp", 38),
        "ArrowLeft" => ("ArrowLeft", 37),
        "ArrowRight" => ("ArrowRight", 39),
        "a" | "A" => ("KeyA", 65),
        _ => ("Unidentified", 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::BrowserBackend;
    use serde_json::json;
    use std::sync::Mutex;

    struct FakeTransport {
        calls: Mutex<Vec<String>>,
        http_auth_pending: Mutex<bool>,
    }

    impl FakeTransport {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                http_auth_pending: Mutex::new(false),
            }
        }
    }

    impl BrowserTransport for FakeTransport {
        fn is_connected(&self) -> bool {
            true
        }

        fn call(&self, request: Value) -> Result<Value, ToolError> {
            let op = request
                .get("op")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            self.calls.lock().expect("calls").push(op.clone());
            let result = match op.as_str() {
                "list_tabs" => json!({
                    "tabs": [{
                        "id": 7,
                        "title": "Example",
                        "url": "https://example.com/",
                        "active": true
                    }]
                }),
                "attach" => json!({
                    "tabId": 7,
                    "title": "Example",
                    "url": "https://example.com/"
                }),
                "navigate" | "new_tab" => {
                    let url = request.get("url").and_then(Value::as_str).unwrap_or("");
                    if url.contains("/inner/") {
                        *self.http_auth_pending.lock().expect("auth") = true;
                        json!({
                            "tabId": 7,
                            "http_auth_required": true,
                            "url": url,
                            "scheme": "digest",
                            "realm": "lab-share"
                        })
                    } else {
                        *self.http_auth_pending.lock().expect("auth") = false;
                        json!({
                            "tabId": 7,
                            "title": "Example",
                            "url": "https://example.com/"
                        })
                    }
                }
                "pending_http_auth" => {
                    if *self.http_auth_pending.lock().expect("auth") {
                        json!({
                            "pending": true,
                            "url": "https://files.example.invalid/inner/",
                            "scheme": "digest",
                            "realm": "lab-share"
                        })
                    } else {
                        json!({ "pending": false })
                    }
                }
                "continue_http_auth" => {
                    *self.http_auth_pending.lock().expect("auth") = false;
                    json!({
                        "tabId": 7,
                        "title": "Share",
                        "url": "https://files.example.invalid/inner/"
                    })
                }
                "cdp" => {
                    let method = request
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    self.calls.lock().expect("calls").push(method.clone());
                    match method.as_str() {
                        "Accessibility.getFullAXTree" => json!({
                            "nodes": [
                                {
                                    "nodeId": "1",
                                    "role": { "value": "WebArea" },
                                    "name": { "value": "Example" },
                                    "childIds": ["2"]
                                },
                                {
                                    "nodeId": "2",
                                    "role": { "value": "button" },
                                    "name": { "value": "Continue" },
                                    "backendDOMNodeId": 12,
                                    "childIds": []
                                }
                            ]
                        }),
                        "DOM.getContentQuads" => json!({
                            "quads": [[0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0]]
                        }),
                        "DOM.resolveNode" => json!({ "object": { "objectId": "1" } }),
                        "Runtime.evaluate" => json!({
                            "result": { "type": "string", "value": "Example Domain" }
                        }),
                        _ => json!({}),
                    }
                }
                _ => json!({}),
            };
            Ok(json!({ "ok": true, "result": result }))
        }
    }

    #[test]
    fn snapshot_then_click_uses_cdp_mouse_events() {
        let transport = Arc::new(FakeTransport::new());
        let browser = ExtensionBrowser::new(Arc::clone(&transport) as Arc<dyn BrowserTransport>);
        let snap = browser.snapshot().unwrap();
        assert!(snap.contains("button \"Continue\""));
        assert!(snap.contains("-e1]"));
        let element_ref = snap
            .split('[')
            .nth(1)
            .and_then(|part| part.split(']').next())
            .unwrap()
            .to_string();
        let clicked = browser.click(&element_ref).unwrap();
        assert!(clicked.contains("Clicked"));
        assert!(clicked.contains("Continue"));
        let calls = transport.calls.lock().unwrap();
        assert!(calls.iter().any(|call| call == "Input.dispatchMouseEvent"));
        assert!(!calls.iter().any(|call| call.contains("hunter")));
    }

    #[test]
    fn fill_does_not_echo_typed_text() {
        let transport = Arc::new(FakeTransport::new());
        let browser = ExtensionBrowser::new(transport);
        let snap = browser.snapshot().unwrap();
        let element_ref = snap
            .split('[')
            .nth(1)
            .and_then(|part| part.split(']').next())
            .unwrap()
            .to_string();
        let filled = browser.fill(&element_ref, "hunter2").unwrap();
        assert!(!filled.contains("hunter2"));
        assert!(filled.contains("Filled"));
    }

    #[test]
    fn plaintext_uses_runtime_evaluate() {
        let transport = Arc::new(FakeTransport::new());
        let browser = ExtensionBrowser::new(Arc::clone(&transport) as Arc<dyn BrowserTransport>);
        assert_eq!(browser.text().unwrap(), "Example Domain");
        assert!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call == "Runtime.evaluate")
        );
    }

    #[test]
    fn navigate_http_auth_does_not_echo_secrets() {
        let transport = Arc::new(FakeTransport::new());
        let browser = ExtensionBrowser::new(Arc::clone(&transport) as Arc<dyn BrowserTransport>);
        let text = browser
            .navigate(
                "goto",
                Some("https://files.example.invalid/inner/lab-share/"),
            )
            .unwrap();
        assert!(text.contains("digest authentication required"));
        assert!(text.contains("fill_credential"));
        assert!(!text.contains("hunter2"));
        assert!(!text.contains("labuser"));
        let challenge = browser.pending_http_auth().expect("pending");
        assert_eq!(challenge.host, "files.example.invalid");
        let filled = browser.continue_http_auth("labuser", "hunter2").unwrap();
        assert!(filled.contains("Filled HTTP authentication"));
        assert!(!filled.contains("hunter2"));
        assert!(!filled.contains("labuser"));
        assert!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call == "continue_http_auth")
        );
        assert!(
            !transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call.contains("hunter2") || call.contains("labuser"))
        );
        assert!(browser.pending_http_auth().is_none());
    }
}
