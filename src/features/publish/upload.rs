use crate::fl;
use crate::shared::api::Api;
use cosmic::dialog::file_chooser;
use std::io::Read;

pub(super) async fn choose_image(api: Api) -> Result<Option<String>, String> {
    let choice = file_chooser::open::Dialog::new()
        .title(fl!("image-choose"))
        .filter(file_chooser::FileFilter::new(&fl!("images")).mimetype("image/*"))
        .open_file()
        .await;
    let path = match choice {
        Ok(response) => response
            .url()
            .to_file_path()
            .map_err(|_| fl!("upload-choose-file"))?,
        Err(file_chooser::Error::Cancelled) => return Ok(None),
        Err(error) => {
            return Err(fl!("upload-picker-failed", error = error.to_string()));
        }
    };
    let (name, mime, bytes) = tokio::task::spawn_blocking(move || {
        const MAX_BYTES: u64 = 12 * 1024 * 1024;
        let file = std::fs::File::open(&path)
            .map_err(|error| fl!("image-read-failed", error = error.to_string()))?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| fl!("image-read-failed", error = error.to_string()))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(fl!("upload-too-large"));
        }
        let mime = match image::guess_format(&bytes) {
            Ok(image::ImageFormat::Jpeg) => "image/jpeg",
            Ok(image::ImageFormat::Png) => "image/png",
            Ok(image::ImageFormat::Gif) => "image/gif",
            Ok(image::ImageFormat::WebP) => "image/webp",
            _ => return Err(fl!("upload-unsupported-format")),
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image")
            .to_owned();
        Ok::<_, String>((name, mime, bytes))
    })
    .await
    .map_err(|_| fl!("image-reader-failed"))??;
    api.upload_image(name, mime, bytes).await.map(Some)
}
