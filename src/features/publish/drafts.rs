use super::Mode;
use crate::shared::local;
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Draft {
    pub community: String,
    pub manual_community: bool,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub image: bool,
    pub body: String,
    pub alt_text: String,
    pub nsfw: Option<bool>,
}

pub(super) fn path(account: &str, mode: Mode) -> io::Result<PathBuf> {
    let account = uuid::Uuid::parse_str(account).map_err(io::Error::other)?;
    let filename = match mode {
        Mode::New => "new.json".into(),
        Mode::Cross { post, .. } => format!("cross-{}.json", post.0),
        Mode::Edit(post) => format!("edit-{}.json", post.0),
    };
    local::config_path(
        PathBuf::from("drafts")
            .join(account.to_string())
            .join(filename),
    )
}
