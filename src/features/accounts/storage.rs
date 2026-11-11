use super::Registry;
use crate::fl;

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(crate::APP_ID, id)
        .map_err(|e| fl!("credentials-unavailable", error = e.to_string()))
}
pub fn read_token(id: &str) -> Result<String, String> {
    entry(id)?
        .get_password()
        .map_err(|e| fl!("credentials-restore-failed", error = e.to_string()))
}
pub fn write_token(id: &str, token: &str) -> Result<(), String> {
    entry(id)?
        .set_password(token)
        .map_err(|e| fl!("credentials-save-failed", error = e.to_string()))
}
pub fn delete_token(id: &str) -> Result<(), String> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(fl!("credentials-remove-failed", error = e.to_string())),
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
    std::fs::create_dir_all(path.parent().ok_or(fl!("settings-dir-missing"))?)
        .map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(registry).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}
