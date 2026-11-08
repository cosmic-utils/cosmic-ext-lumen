use crate::shared::api::Api;
use cosmic::dialog::file_chooser;
use std::io::Read;

pub(super) async fn choose_image(api: Api) -> Result<Option<String>, String> {
    let choice = file_chooser::open::Dialog::new()
        .title("Choose an image")
        .filter(file_chooser::FileFilter::new("Images").mimetype("image/*"))
        .open_file()
        .await;
    let path = match choice {
        Ok(response) => response
            .url()
            .to_file_path()
            .map_err(|_| "Choose a local image file.")?,
        Err(file_chooser::Error::Cancelled) => return Ok(None),
        Err(error) => return Err(format!("Could not open the image picker: {error}")),
    };
    let (name, mime, bytes) = tokio::task::spawn_blocking(move || {
        const MAX_BYTES: u64 = 12 * 1024 * 1024;
        let file =
            std::fs::File::open(&path).map_err(|error| format!("Could not read image: {error}"))?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read image: {error}"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Choose an image smaller than 12 MiB.".into());
        }
        let mime = match image::guess_format(&bytes) {
            Ok(image::ImageFormat::Jpeg) => "image/jpeg",
            Ok(image::ImageFormat::Png) => "image/png",
            Ok(image::ImageFormat::Gif) => "image/gif",
            Ok(image::ImageFormat::WebP) => "image/webp",
            _ => return Err("Choose a JPEG, PNG, GIF, or WebP image.".into()),
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image")
            .to_owned();
        Ok::<_, String>((name, mime, bytes))
    })
    .await
    .map_err(|_| "Image reader did not finish.")??;
    api.upload_image(name, mime, bytes).await.map(Some)
}
