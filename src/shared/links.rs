/// Only open web links supplied by remote content, never arbitrary URI handlers.
pub fn open_web_link(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err("Only HTTP and HTTPS links can be opened.".into());
    }
    open::that_detached(url.as_str()).map_err(|e| format!("Could not open browser: {e}"))
}
