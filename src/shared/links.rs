use crate::fl;
/// Only open web links supplied by remote content, never arbitrary URI handlers.
pub fn open_web_link(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(fl!("link-scheme-unsupported"));
    }
    open::that_detached(url.as_str()).map_err(|e| fl!("browser-open-failed", error = e.to_string()))
}
