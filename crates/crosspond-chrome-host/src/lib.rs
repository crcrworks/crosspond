//! Native-messaging framing, unix-socket bridge, and Chromium host manifests.
//!
//! Chrome launches the `crosspond-chrome-host` binary. That process copies
//! length-prefixed JSON between stdin/stdout and `~/.crosspond/chrome-bridge.sock`.
//! This crate must not depend on Tauri or `crosspond-core`.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Value, json};

/// Native messaging host name registered with Chromium browsers.
pub const NATIVE_HOST_NAME: &str = "com.crosspond.chrome";

/// Stable unpacked extension id (public key in `extension/chrome/manifest.json`).
pub const EXTENSION_ID: &str = "cjgcgokbkhcedpojinajphgehbliaile";

/// Chrome limits native-host → extension messages to 1 MiB.
pub const MAX_NATIVE_MESSAGE_BYTES: usize = 1024 * 1024;

const CONNECT_RETRY: Duration = Duration::from_millis(250);

/// `~/.crosspond/chrome-bridge.sock`.
pub fn default_socket_path() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(home)
        .join(".crosspond")
        .join("chrome-bridge.sock")
}

pub fn socket_dir() -> PathBuf {
    default_socket_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Length-prefixed native-messaging / bridge frame.
pub fn write_message<W: Write>(writer: &mut W, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_NATIVE_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native message exceeds 1 MiB",
        ));
    }
    let len = u32::try_from(bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "native message exceeds 1 MiB"))?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

pub fn read_message<R: Read>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_NATIVE_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native message exceeds 1 MiB",
        ));
    }
    let mut buf = vec![0; len];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn copy_framed<R, W>(reader: &mut R, writer: &mut W) -> io::Result<()>
where
    R: Read,
    W: Write,
{
    loop {
        let payload = match read_message(reader) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(err) => return Err(err),
        };
        write_message(writer, &payload)?;
    }
}

/// Absolute path candidates for the native host binary next to this process.
pub fn host_binary_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = current_exe.parent() {
        out.push(dir.join("crosspond-chrome-host"));
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out.push(
        manifest_dir
            .join("../..")
            .join("target/debug/crosspond-chrome-host"),
    );
    out.push(
        manifest_dir
            .join("../..")
            .join("target/release/crosspond-chrome-host"),
    );
    out
}

pub fn resolve_host_binary(current_exe: &Path) -> Option<PathBuf> {
    host_binary_candidates(current_exe)
        .into_iter()
        .find(|path| path.is_file())
}

pub fn native_host_manifest_json(host_path: &Path) -> Value {
    json!({
        "name": NATIVE_HOST_NAME,
        "description": "Crosspond Chrome bridge",
        "path": host_path.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")]
    })
}

pub fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
}

/// Staged copy Chrome launches: `~/.crosspond/bin/crosspond-chrome-host`.
pub fn installed_host_binary_path() -> PathBuf {
    installed_host_binary_path_for(&home_dir())
}

pub fn installed_host_binary_path_for(home: &Path) -> PathBuf {
    home.join(".crosspond")
        .join("bin")
        .join("crosspond-chrome-host")
}

/// Chromium native-messaging host directories on this OS.
pub fn native_host_dirs() -> Vec<PathBuf> {
    native_host_dirs_for(&home_dir())
}

pub fn native_host_dirs_for(home: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let support = home.join("Library/Application Support");
        vec![
            support.join("Google/Chrome/NativeMessagingHosts"),
            support.join("Google/Chrome Canary/NativeMessagingHosts"),
            support.join("Google/Chrome Beta/NativeMessagingHosts"),
            support.join("Google/Chrome Dev/NativeMessagingHosts"),
            support.join("Google/Chrome for Testing/NativeMessagingHosts"),
            support.join("Chromium/NativeMessagingHosts"),
            support.join("Helium/NativeMessagingHosts"),
            support.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            support.join("Microsoft Edge/NativeMessagingHosts"),
            support.join("Arc/User Data/NativeMessagingHosts"),
            support.join("Arc/NativeMessagingHosts"),
            support.join("company.thebrowser.Browser/NativeMessagingHosts"),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        let config = home.join(".config");
        vec![
            config.join("google-chrome/NativeMessagingHosts"),
            config.join("google-chrome-unstable/NativeMessagingHosts"),
            config.join("google-chrome-for-testing/NativeMessagingHosts"),
            config.join("chromium/NativeMessagingHosts"),
            config.join("helium/NativeMessagingHosts"),
            config.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            config.join("microsoft-edge/NativeMessagingHosts"),
        ]
    }
}

fn stage_host_binary(source: &Path, dest: &Path) -> Result<(), String> {
    let Some(dir) = dest.parent() else {
        return Err(format!("invalid host path {}", dest.display()));
    };
    fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    if dest.exists()
        && dest
            .canonicalize()
            .ok()
            .as_deref()
            .is_some_and(|canonical| canonical == source)
    {
        return Ok(());
    }
    fs::copy(source, dest).map_err(|err| format!("{}: {err}", dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dest, fs::Permissions::from_mode(0o700))
            .map_err(|err| format!("{}: {err}", dest.display()))?;
    }
    Ok(())
}

pub fn install_native_host_manifests(host_path: &Path) -> Result<Vec<PathBuf>, String> {
    install_native_host_manifests_in(&home_dir(), host_path)
}

/// Copy the host binary to `~/.crosspond/bin` and register it with Chromium.
pub fn install_native_host_manifests_in(
    home: &Path,
    host_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    let source = host_path
        .canonicalize()
        .map_err(|err| format!("{}: {err}", host_path.display()))?;
    let staged = installed_host_binary_path_for(home);
    stage_host_binary(&source, &staged)?;
    let staged = staged
        .canonicalize()
        .map_err(|err| format!("{}: {err}", staged.display()))?;
    let body = serde_json::to_vec_pretty(&native_host_manifest_json(&staged))
        .map_err(|err| err.to_string())?;
    let mut written = Vec::new();
    let mut last_err = None;
    for dir in native_host_dirs_for(home) {
        if let Err(err) = fs::create_dir_all(&dir) {
            last_err = Some(format!("{}: {err}", dir.display()));
            continue;
        }
        let path = dir.join(format!("{NATIVE_HOST_NAME}.json"));
        if let Err(err) = fs::write(&path, &body) {
            last_err = Some(format!("{}: {err}", path.display()));
            continue;
        }
        written.push(path);
    }
    if written.is_empty() {
        return Err(last_err.unwrap_or_else(|| "no native-host directories".into()));
    }
    Ok(written)
}

/// Dev checkout or bundled resources: `extension/chrome`.
pub fn extension_dir_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = current_exe.parent() {
        out.push(dir.join("chrome-extension"));
        out.push(dir.join("../Resources/chrome-extension"));
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out.push(manifest_dir.join("../..").join("extension/chrome"));
    out
}

pub fn resolve_extension_dir(current_exe: &Path) -> Option<PathBuf> {
    extension_dir_candidates(current_exe)
        .into_iter()
        .find(|path| path.join("manifest.json").is_file())
}

/// Bidirectional JSON bridge used by `BrowserBackend`.
pub struct BrowserBridge {
    inner: Mutex<Option<BridgeStream>>,
    connected: AtomicBool,
    next_id: AtomicU64,
}

struct BridgeStream {
    #[cfg(unix)]
    stream: std::os::unix::net::UnixStream,
}

impl BrowserBridge {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(None),
            connected: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    pub fn call_json(&self, mut request: Value) -> Result<Value, String> {
        if request.get("id").is_none() {
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            request["id"] = json!(id);
        }
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let payload =
            serde_json::to_vec(&request).map_err(|err| format!("browser request: {err}"))?;
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "browser bridge lock".to_string())?;
        let Some(conn) = guard.as_mut() else {
            return Err(disconnected_message().into());
        };
        #[cfg(unix)]
        {
            write_message(&mut conn.stream, &payload).map_err(|err| err.to_string())?;
            loop {
                let bytes = match read_message(&mut conn.stream) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        *guard = None;
                        self.connected.store(false, Ordering::SeqCst);
                        return Err(format!("browser extension disconnected: {err}"));
                    }
                };
                let response: Value =
                    serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
                if response.get("id") == Some(&id) {
                    if response.get("ok").and_then(Value::as_bool) == Some(false) {
                        let err = response
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("browser extension error");
                        return Err(err.to_string());
                    }
                    return Ok(response);
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = conn;
            Err("browser bridge requires unix sockets".into())
        }
    }

    fn attach_stream(&self, #[cfg(unix)] stream: std::os::unix::net::UnixStream) {
        #[cfg(unix)]
        {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(25)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(25)));
            if let Ok(mut guard) = self.inner.lock() {
                *guard = Some(BridgeStream { stream });
                self.connected.store(true, Ordering::SeqCst);
            }
        }
    }
}

pub fn disconnected_message() -> &'static str {
    "Chrome extension is not connected. In Settings, load the unpacked Crosspond extension (chrome://extensions → Developer mode → Load unpacked → the extension/chrome folder). Until then, browser_* tools cannot run; do not fall back to Accessibility or screenshots for Chromium pages."
}

/// Listen for native-host connections. Replaces the active stream on each accept.
pub fn spawn_bridge_server(bridge: Arc<BrowserBridge>, socket_path: PathBuf) -> JoinHandle<()> {
    thread::Builder::new()
        .name("crosspond-chrome-bridge".into())
        .spawn(move || {
            if let Err(err) = run_bridge_server(bridge, socket_path) {
                eprintln!("crosspond: chrome bridge: {err}");
            }
        })
        .expect("chrome bridge thread")
}

#[cfg(unix)]
fn run_bridge_server(bridge: Arc<BrowserBridge>, socket_path: PathBuf) -> io::Result<()> {
    use std::os::unix::net::UnixListener;

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    let _ = fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600));
    }
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => bridge.attach_stream(stream),
            Err(err) => {
                eprintln!("crosspond: chrome bridge accept: {err}");
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn run_bridge_server(_bridge: Arc<BrowserBridge>, _socket_path: PathBuf) -> io::Result<()> {
    Ok(())
}

/// Native-host process: copy framed JSON between Chrome stdio and the unix socket.
pub fn run_native_host(socket_path: PathBuf) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;
        loop {
            match UnixStream::connect(&socket_path) {
                Ok(stream) => {
                    if let Err(err) = pipe_stdio(stream)
                        && err.kind() != io::ErrorKind::UnexpectedEof
                    {
                        eprintln!("crosspond-chrome-host: {err}");
                    }
                }
                Err(_) => thread::sleep(CONNECT_RETRY),
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = socket_path;
        Err(io::Error::other("native host requires unix"))
    }
}

#[cfg(unix)]
fn pipe_stdio(stream: std::os::unix::net::UnixStream) -> io::Result<()> {
    let mut to_app = stream.try_clone()?;
    let mut from_app = stream;
    let incoming = thread::spawn(move || {
        let mut stdin = io::stdin();
        copy_framed(&mut stdin, &mut to_app)
    });
    let outgoing = thread::spawn(move || {
        let mut stdout = io::stdout();
        copy_framed(&mut from_app, &mut stdout)
    });
    let first = incoming
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("stdin pipe panicked")));
    let second = outgoing
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("stdout pipe panicked")));
    first.and(second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_roundtrip() {
        let mut buf = Vec::new();
        write_message(&mut buf, br#"{"ok":true}"#).unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let got = read_message(&mut cursor).unwrap();
        assert_eq!(got, br#"{"ok":true}"#);
    }

    #[test]
    fn framing_rejects_oversize_length() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(MAX_NATIVE_MESSAGE_BYTES as u32 + 1).to_le_bytes());
        buf.extend_from_slice(&[0]);
        let mut cursor = std::io::Cursor::new(buf);
        let err = read_message(&mut cursor).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn native_host_manifest_pins_extension_id() {
        let json = native_host_manifest_json(Path::new("/tmp/crosspond-chrome-host"));
        assert_eq!(json["name"], NATIVE_HOST_NAME);
        assert_eq!(json["type"], "stdio");
        let origins = json["allowed_origins"].as_array().unwrap();
        assert_eq!(origins[0], format!("chrome-extension://{EXTENSION_ID}/"));
        assert_eq!(json["path"], "/tmp/crosspond-chrome-host");
    }

    #[test]
    fn native_host_dirs_are_under_home() {
        let dirs = native_host_dirs();
        assert!(!dirs.is_empty());
        assert!(
            dirs.iter()
                .any(|path| path.ends_with("NativeMessagingHosts"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_pair_copies_one_frame() {
        use std::os::unix::net::UnixStream;
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_message(&mut a, b"{\"id\":1}").unwrap();
        let got = read_message(&mut b).unwrap();
        assert_eq!(got, b"{\"id\":1}");
    }

    #[test]
    fn extension_manifest_is_minimal_and_pins_key() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extension/chrome/manifest.json");
        let text = fs::read_to_string(&path).expect("extension manifest");
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["manifest_version"], 3);
        let permissions = json["permissions"].as_array().unwrap();
        let perms: Vec<&str> = permissions.iter().filter_map(Value::as_str).collect();
        assert!(perms.contains(&"nativeMessaging"));
        assert!(perms.contains(&"debugger"));
        assert!(perms.contains(&"tabs"));
        assert!(perms.contains(&"tabGroups"));
        assert!(!perms.contains(&"history"));
        assert!(!perms.contains(&"bookmarks"));
        assert!(!perms.contains(&"downloads"));
        assert!(!perms.contains(&"notifications"));
        assert!(
            json["key"]
                .as_str()
                .unwrap_or("")
                .starts_with("MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA")
        );
        assert!(
            extension_dir_candidates(Path::new("/tmp/crosspond"))
                .iter()
                .any(|candidate| candidate.ends_with("extension/chrome"))
        );
    }

    #[test]
    fn service_worker_reads_native_host_last_error() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../extension/chrome/service_worker.js");
        let text = fs::read_to_string(&path).expect("service worker");
        assert!(text.contains("chrome.runtime.connectNative"));
        assert!(text.contains("chrome.runtime.lastError"));
    }

    #[test]
    fn install_stages_binary_and_writes_host_manifest() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("crosspond-nmh-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let bin = root.join("fake-host");
        fs::write(&bin, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let home = root.join("home");
        let written = install_native_host_manifests_in(&home, &bin).unwrap();
        assert!(!written.is_empty());
        let staged = installed_host_binary_path_for(&home)
            .canonicalize()
            .unwrap();
        assert!(staged.is_file());
        let manifest = written
            .iter()
            .find(|path| path.ends_with(format!("{NATIVE_HOST_NAME}.json")))
            .expect("host manifest");
        let json: Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
        assert_eq!(json["name"], NATIVE_HOST_NAME);
        assert_eq!(json["type"], "stdio");
        let path = json["path"].as_str().expect("absolute path");
        assert_eq!(Path::new(path), staged.as_path());
        let _ = fs::remove_dir_all(&root);
    }
}
