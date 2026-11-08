use super::Registry;

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(crate::APP_ID, id).map_err(|e| format!("Credential store unavailable: {e}"))
}
pub fn read_token(id: &str) -> Result<String, String> {
    entry(id)?.get_password().map_err(|e| {
        format!("Cannot restore session; unlock your credential store or log in again: {e}")
    })
}
pub fn write_token(id: &str, token: &str) -> Result<(), String> {
    entry(id)?
        .set_password(token)
        .map_err(|e| format!("Cannot save session in credential store: {e}"))
}
pub fn delete_token(id: &str) -> Result<(), String> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Cannot remove saved session: {e}")),
    }
}
fn path() -> Result<std::path::PathBuf, String> {
    crate::shared::local::config_path("accounts.json").map_err(|error| error.to_string())
}
pub fn load() -> Result<Registry, String> {
    let path = path()?;
    if !path.exists() {
        return Ok(Registry::default());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub fn save(registry: &Registry) -> Result<(), String> {
    let path = path()?;
    std::fs::create_dir_all(path.parent().ok_or("Missing settings directory")?)
        .map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(registry).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}
