use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Settings {
    instance: String,
}

fn path() -> Result<std::path::PathBuf, String> {
    crate::shared::local::config_path("settings.json").map_err(|error| error.to_string())
}

pub fn load() -> (String, Option<String>) {
    let result = (|| {
        let path = path()?;
        if !path.exists() {
            return Ok("lemmy.world".to_owned());
        }
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let settings: Settings = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok::<_, String>(settings.instance)
    })();
    match result {
        Ok(instance) => (instance, None),
        Err(error) => (
            "lemmy.world".into(),
            Some(format!("Could not load settings: {error}")),
        ),
    }
}

pub fn save(instance: &str) -> Result<(), String> {
    let result = (|| {
        let path = path()?;
        std::fs::create_dir_all(path.parent().ok_or("Missing settings directory")?)
            .map_err(|e| e.to_string())?;
        let data = serde_json::to_vec_pretty(&Settings {
            instance: instance.into(),
        })
        .map_err(|e| e.to_string())?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, data).map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    })();
    result.map_err(|e| format!("Connected, but could not save the instance: {e}"))
}
