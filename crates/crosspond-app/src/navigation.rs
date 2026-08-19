use tauri::Url;

/// Origins Crosspond itself serves in the WebView (dev server, custom protocol, IPC).
pub fn is_app_webview_url(url: &Url) -> bool {
    match url.scheme() {
        "tauri" | "ipc" | "asset" | "about" => true,
        "http" | "https" => matches!(
            url.host_str(),
            Some(
                "localhost"
                    | "127.0.0.1"
                    | "::1"
                    | "tauri.localhost"
                    | "ipc.localhost"
                    | "asset.localhost"
            )
        ),
        _ => false,
    }
}

pub fn is_browser_scheme(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "mailto" | "tel")
}

/// Allow in-app navigations. External http(s)/mailto/tel open in the default
/// browser; everything else is blocked so the launcher cannot be hijacked.
pub fn handle_navigation(url: &Url) -> bool {
    if is_app_webview_url(url) {
        return true;
    }
    open_in_browser(url);
    false
}

pub fn handle_new_window(url: &Url) {
    if !is_app_webview_url(url) {
        open_in_browser(url);
    }
}

pub fn open_in_browser(url: &Url) {
    if !is_browser_scheme(url) {
        return;
    }
    let _ = tauri_plugin_opener::open_url(url.as_str(), None::<&str>);
}

pub fn open_external_url(raw: &str) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|_| "invalid url".to_string())?;
    if !is_browser_scheme(&url) {
        return Err("unsupported url".into());
    }
    tauri_plugin_opener::open_url(url.as_str(), None::<&str>).map_err(|_| "open failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Url {
        Url::parse(raw).expect("url")
    }

    #[test]
    fn allows_app_webview_origins() {
        assert!(is_app_webview_url(&parse("http://localhost:5173/")));
        assert!(is_app_webview_url(&parse("http://localhost:5173/settings")));
        assert!(is_app_webview_url(&parse("http://tauri.localhost/")));
        assert!(is_app_webview_url(&parse(
            "https://tauri.localhost/settings"
        )));
        assert!(is_app_webview_url(&parse("http://ipc.localhost/")));
        assert!(is_app_webview_url(&parse("about:blank")));
        assert!(is_app_webview_url(&parse("tauri://localhost/")));
    }

    #[test]
    fn rejects_external_pages() {
        assert!(!is_app_webview_url(&parse(
            "https://opencode.ai/docs/ja/zen"
        )));
        assert!(!is_app_webview_url(&parse(
            "https://github.com/anomalyco/opencode"
        )));
        assert!(!is_app_webview_url(&parse("http://example.com/")));
    }

    #[test]
    fn only_browser_schemes_are_handed_to_the_os() {
        assert!(is_browser_scheme(&parse("https://example.com/a?q=1")));
        assert!(is_browser_scheme(&parse("http://example.com")));
        assert!(is_browser_scheme(&parse("mailto:user@example.com")));
        assert!(is_browser_scheme(&parse("tel:+15555550100")));
        assert!(!is_browser_scheme(&parse("javascript:alert(1)")));
        assert!(!is_browser_scheme(&parse("file:///etc/passwd")));
        assert!(!is_browser_scheme(&parse("data:text/html,hi")));
    }

    #[test]
    fn in_app_navigation_is_allowed() {
        assert!(handle_navigation(&parse("http://localhost:5173/")));
        assert!(handle_navigation(&parse(
            "https://tauri.localhost/settings"
        )));
    }

    #[test]
    fn open_external_url_rejects_non_browser_targets() {
        assert!(open_external_url("javascript:alert(1)").is_err());
        assert!(open_external_url("file:///etc/passwd").is_err());
        assert!(open_external_url("not a url").is_err());
    }
}
