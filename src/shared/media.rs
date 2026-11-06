//! Image downloads and the in-memory cache shared by feeds and discussions.
use cosmic::{
    Element,
    iced::{ContentFit, Length, Task},
    widget::{self, button, icon, image},
};
use image::{Handle, Image};
use lemmy_api_common::lemmy_db_schema::source::{community::Community, person::Person, post::Post};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Cursor,
    sync::Arc,
};
use url::Url;

const DOWNLOAD_LIMIT: usize = 12 * 1024 * 1024;
const CACHE_LIMIT: usize = 48 * 1024 * 1024;
const CACHE_ENTRIES: usize = 512;
const ZOOM_LEVELS: [f32; 7] = [1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Size {
    Avatar,
    Thumbnail,
    Preview,
}

impl Size {
    fn pixels(self) -> u32 {
        match self {
            Self::Avatar => 48,
            Self::Thumbnail => 256,
            Self::Preview => 1600,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Picture {
    handle: Handle,
    bytes: usize,
}

#[derive(Clone, Debug)]
pub enum Message {
    Loaded(String, Size, Option<Picture>),
    Open(String),
    Close,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    OpenInBrowser(String),
}

pub struct Media {
    http: reqwest::Client,
    downloads: Arc<tokio::sync::Semaphore>,
    pending: HashSet<(String, Size)>,
    cache: HashMap<(String, Size), Option<Picture>>,
    order: VecDeque<(String, Size)>,
    bytes: usize,
    opened: Option<String>,
    zoom: usize,
}

impl Media {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            downloads: Arc::new(tokio::sync::Semaphore::new(4)),
            pending: HashSet::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            opened: None,
            zoom: 0,
        }
    }

    pub fn request(&mut self, urls: impl IntoIterator<Item = String>, size: Size) -> Task<Message> {
        let tasks: Vec<_> = urls
            .into_iter()
            .filter_map(|url| {
                let key = (url.clone(), size);
                if self.cache.contains_key(&key) || !web_url(&url) || !self.pending.insert(key) {
                    return None;
                }
                let http = self.http.clone();
                let downloads = self.downloads.clone();
                Some(Task::perform(
                    async move {
                        let picture = async {
                            let _permit = downloads.acquire().await.ok()?;
                            download(http, &url, size).await
                        }
                        .await;
                        (url, picture)
                    },
                    move |(url, picture)| Message::Loaded(url, size, picture),
                ))
            })
            .collect();
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Loaded(url, size, picture) => {
                let key = (url, size);
                if !self.pending.remove(&key) {
                    return Task::none();
                }
                self.bytes += picture.as_ref().map_or(0, |picture| picture.bytes);
                self.order.push_back(key.clone());
                self.cache.insert(key, picture);
                while self.bytes > CACHE_LIMIT || self.cache.len() > CACHE_ENTRIES {
                    let Some(oldest) = self.order.pop_front() else {
                        break;
                    };
                    if let Some(Some(picture)) = self.cache.remove(&oldest) {
                        self.bytes -= picture.bytes;
                    }
                }
            }
            Message::Open(url) if web_url(&url) => {
                self.opened = Some(url.clone());
                self.zoom = 0;
                let key = (url.clone(), Size::Preview);
                if matches!(self.cache.get(&key), Some(None)) {
                    self.cache.remove(&key);
                    self.order.retain(|cached| cached != &key);
                }
                return self.request([url], Size::Preview);
            }
            Message::Close => self.opened = None,
            Message::ZoomIn => self.zoom = (self.zoom + 1).min(ZOOM_LEVELS.len() - 1),
            Message::ZoomOut => self.zoom = self.zoom.saturating_sub(1),
            Message::ResetZoom => self.zoom = 0,
            _ => {}
        }
        Task::none()
    }

    pub fn get(&self, url: &str, size: Size) -> Option<Handle> {
        self.cache
            .get(&(url.to_owned(), size))?
            .as_ref()
            .map(|picture| picture.handle.clone())
    }

    pub fn avatar(&self, url: Option<&str>, fallback: &str) -> icon::Handle {
        url.and_then(|url| self.get(url, Size::Avatar)).map_or_else(
            || icon::from_name(fallback).handle(),
            |handle| icon::Handle {
                symbolic: false,
                data: icon::Data::Image(handle),
            },
        )
    }

    pub fn person_avatar(&self, person: &Person) -> icon::Handle {
        self.avatar(
            person.avatar.as_ref().map(|url| url.as_str()),
            "avatar-default-symbolic",
        )
    }

    pub fn community_avatar(&self, community: &Community) -> icon::Handle {
        self.avatar(
            community.icon.as_ref().map(|url| url.as_str()),
            "system-users-symbolic",
        )
    }

    pub fn dialog(&self) -> Option<Element<'_, Message>> {
        use cosmic::iced::widget::{responsive, scrollable};

        let url = self.opened.as_deref()?;
        let zoom = ZOOM_LEVELS[self.zoom];
        let mut dialog = widget::dialog()
            .title("Image")
            .width(Length::Fill)
            .max_width(1050)
            .max_height(680);
        if let Some(handle @ Handle::Rgba { width, height, .. }) = self.get(url, Size::Preview) {
            let controls = widget::row::with_capacity(3)
                .align_y(cosmic::iced::Alignment::Center)
                .push(
                    button::icon(icon::from_name("zoom-out-symbolic"))
                        .tooltip("Zoom out")
                        .on_press_maybe((self.zoom > 0).then_some(Message::ZoomOut)),
                )
                .push(
                    button::standard(if self.zoom == 0 {
                        "Fit".to_owned()
                    } else {
                        format!("{:.0}%", zoom * 100.0)
                    })
                    .on_press_maybe((self.zoom > 0).then_some(Message::ResetZoom)),
                )
                .push(
                    button::icon(icon::from_name("zoom-in-symbolic"))
                        .tooltip("Zoom in")
                        .on_press_maybe(
                            (self.zoom + 1 < ZOOM_LEVELS.len()).then_some(Message::ZoomIn),
                        ),
                );
            let viewport = responsive(move |size| {
                let width = width as f32;
                let height = height as f32;
                let scale = (size.width / width).min(size.height / height) * zoom;
                let picture = image_view(
                    handle.clone(),
                    Length::Fixed(width * scale),
                    Length::Fixed(height * scale),
                );
                scrollable(
                    widget::container(picture)
                        .center_x(Length::Fixed(size.width.max(width * scale)))
                        .center_y(Length::Fixed(size.height.max(height * scale))),
                )
                .direction(scrollable::Direction::Both {
                    vertical: scrollable::Scrollbar::default(),
                    horizontal: scrollable::Scrollbar::default(),
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            });
            dialog = dialog.control(controls).control(viewport);
        } else {
            let status = if self.pending.contains(&(url.to_owned(), Size::Preview)) {
                "Loading image…"
            } else {
                "This image could not be displayed."
            };
            dialog = dialog.body(status);
        }
        Some(
            dialog
                .primary_action(button::standard("Close").on_press(Message::Close))
                .secondary_action(
                    button::text("Open in browser")
                        .on_press(Message::OpenInBrowser(url.to_owned())),
                )
                .into(),
        )
    }

    pub fn is_open(&self) -> bool {
        self.opened.is_some()
    }
}

pub fn image_view(handle: Handle, width: Length, height: Length) -> Image<'static, Handle> {
    widget::image(handle)
        .width(width)
        .height(height)
        .content_fit(ContentFit::Contain)
}

pub fn direct_image(post: &Post) -> Option<&str> {
    let url = post.url.as_ref()?;
    let image_type = post
        .url_content_type
        .as_deref()
        .is_some_and(|kind| kind.starts_with("image/"));
    let image_extension = url.path().rsplit_once('.').is_some_and(|(_, suffix)| {
        ["png", "jpg", "jpeg", "webp", "gif"]
            .iter()
            .any(|ext| suffix.eq_ignore_ascii_case(ext))
    });
    (image_type || image_extension).then_some(url.as_str())
}

pub fn thumbnail(post: &Post) -> Option<&str> {
    if post.deleted || post.removed || post.nsfw {
        return None;
    }
    post.thumbnail_url
        .as_ref()
        .map(|url| url.as_str())
        .or_else(|| direct_image(post))
}

pub fn web_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

async fn download(http: reqwest::Client, url: &str, size: Size) -> Option<Picture> {
    let mut response = http.get(url).send().await.ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|length| length > DOWNLOAD_LIMIT as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > DOWNLOAD_LIMIT - bytes.len() {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    tokio::task::spawn_blocking(move || decode(bytes, size))
        .await
        .ok()?
}

fn decode(bytes: Vec<u8>, size: Size) -> Option<Picture> {
    let mut reader = ::image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = ::image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let side = size.pixels();
    let pixels = if size == Size::Avatar {
        decoded
            .resize_to_fill(side, side, ::image::imageops::FilterType::Triangle)
            .into_rgba8()
    } else if decoded.width() > side || decoded.height() > side {
        decoded.thumbnail(side, side).into_rgba8()
    } else {
        decoded.into_rgba8()
    };
    let (width, height) = pixels.dimensions();
    let bytes = pixels.into_raw();
    Some(Picture {
        bytes: bytes.len(),
        handle: Handle::from_rgba(width, height, bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_bounds_image_dimensions() {
        let mut png = Cursor::new(Vec::new());
        ::image::DynamicImage::new_rgba8(512, 256)
            .write_to(&mut png, ::image::ImageFormat::Png)
            .unwrap();
        let original = png.into_inner();

        let thumb = decode(original.clone(), Size::Thumbnail).unwrap();
        assert_eq!(thumb.bytes, 256 * 128 * 4);
        let avatar = decode(original.clone(), Size::Avatar).unwrap();
        assert!(matches!(
            avatar.handle,
            Handle::Rgba {
                width: 48,
                height: 48,
                ..
            }
        ));
        let preview = decode(original, Size::Preview).unwrap();
        assert_eq!(preview.bytes, 512 * 256 * 4);
        assert!(decode(b"<html>Not an image</html>".to_vec(), Size::Preview).is_none());
    }

    #[test]
    fn failed_downloads_are_deduplicated_and_can_be_retried() {
        use cosmic::iced::runtime;
        let mut media = Media::new(reqwest::Client::new());
        let url = "https://example.com/image.png".to_owned();
        let size = Size::Preview;
        assert!(runtime::task::into_stream(media.request([url.clone()], size)).is_some());
        assert!(runtime::task::into_stream(media.request([url.clone()], size)).is_none());

        let _ = media.update(Message::Loaded(url.clone(), size, None));
        assert!(runtime::task::into_stream(media.request([url.clone()], size)).is_none());
        assert!(runtime::task::into_stream(media.update(Message::Open(url))).is_some());
    }
    #[test]
    fn image_viewer_clamps_zoom_and_resets_for_each_image() {
        let mut media = Media::new(reqwest::Client::new());
        let _ = media.update(Message::Open("https://example.com/one.png".into()));
        for _ in 0..ZOOM_LEVELS.len() {
            let _ = media.update(Message::ZoomIn);
        }
        assert_eq!(ZOOM_LEVELS[media.zoom], 4.0);
        let _ = media.update(Message::ZoomOut);
        assert_eq!(ZOOM_LEVELS[media.zoom], 3.0);
        let _ = media.update(Message::Open("https://example.com/two.png".into()));
        assert_eq!(ZOOM_LEVELS[media.zoom], 1.0);
    }
}
