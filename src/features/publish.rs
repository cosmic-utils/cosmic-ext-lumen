use crate::fl;
mod drafts;
mod upload;

use crate::shared::{
    api::Api,
    local,
    media::{self, Media, Size},
    presentation::{community_handle, community_label},
};
use cosmic::{
    Element,
    iced::{Alignment, Length, Task, core::text::Wrapping},
    widget::{self, button, markdown, text_editor},
};
use drafts::Draft;
use lemmy_api_common::{
    community::{GetCommunity, GetCommunityResponse},
    lemmy_db_schema::{
        newtypes::{CommunityId, PostId},
        source::community::Community,
    },
    lemmy_db_views::structs::PostView,
    post::{CreatePost, EditPost, PostResponse},
};
use std::path::PathBuf;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    New,
    Cross {
        post: PostId,
        community: CommunityId,
    },
    Edit(PostId),
}

#[derive(Clone, Debug)]
pub enum Message {
    Close,
    Discard,
    TogglePreview,
    OpenLink(String),
    UploadImage,
    UploadFinished(u64, Result<Option<String>, String>),
    Community(String),
    PickCommunity(usize),
    Title(String),
    Url(String),
    AltText(String),
    RemoveImage,
    Body(text_editor::Action),
    Submit,
    Submitted(u64, Result<Box<PostView>, String>),
}

#[derive(Default)]
pub struct Publish {
    pub visible: bool,
    pub busy: bool,
    uploading: bool,
    mode: Mode,
    draft_path: Option<PathBuf>,
    draft_error: Option<String>,
    preview: Option<markdown::Content>,
    alt_text: String,
    image: bool,
    nsfw: Option<bool>,
    community: String,
    manual_community: bool,
    title: String,
    url: String,
    body: text_editor::Content<cosmic::Renderer>,
    error: Option<String>,
}

impl Publish {
    pub fn open_new(&mut self, account: &str, community: Option<&Community>) {
        self.prepare(account, Mode::New, Draft::default());
        self.open(community);
    }

    pub fn open_edit(&mut self, account: &str, post: &PostView) {
        self.prepare(
            account,
            Mode::Edit(post.post.id),
            Draft {
                community: community_label(&post.community),
                title: post.post.name.clone(),
                url: post
                    .post
                    .url
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                body: post.post.body.clone().unwrap_or_default(),
                alt_text: post.post.alt_text.clone().unwrap_or_default(),
                image: media::direct_image(&post.post).is_some(),
                ..Draft::default()
            },
        );
    }

    pub fn open_cross(&mut self, account: &str, source: &PostView) {
        let attribution = fl!(
            "cross-post-attribution",
            link = format!("[{}]({})", fl!("cross-post-original"), source.post.ap_id)
        );
        let body = source
            .post
            .body
            .as_deref()
            .filter(|body| !body.is_empty())
            .map_or_else(
                || attribution.clone(),
                |body| {
                    format!(
                        "{attribution}

{body}"
                    )
                },
            );
        self.prepare(
            account,
            Mode::Cross {
                post: source.post.id,
                community: source.community.id,
            },
            Draft {
                title: source.post.name.clone(),
                url: source
                    .post
                    .url
                    .as_ref()
                    .unwrap_or(&source.post.ap_id)
                    .to_string(),
                body,
                alt_text: source.post.alt_text.clone().unwrap_or_default(),
                image: media::direct_image(&source.post).is_some(),
                nsfw: Some(source.post.nsfw),
                ..Draft::default()
            },
        );
    }

    fn prepare(&mut self, account: &str, mode: Mode, initial: Draft) {
        let path = drafts::path(account, mode);
        if self.mode == mode
            && self.draft_path.is_some()
            && self.draft_path.as_ref() == path.as_ref().ok()
        {
            self.visible = true;
            return;
        }

        *self = Self {
            visible: true,
            mode,
            ..Self::default()
        };
        let mut draft = initial;
        match path {
            Ok(path) => {
                match local::read(&path) {
                    Ok(Some(saved)) => draft = saved,
                    Ok(None) => {}
                    Err(error) => {
                        self.draft_error = Some(fl!("draft-open-failed", error = error.to_string()))
                    }
                }
                self.draft_path = Some(path);
            }
            Err(error) => {
                self.draft_error = Some(fl!("draft-not-found", error = error.to_string()))
            }
        }
        self.community = draft.community;
        self.manual_community = draft.manual_community;
        self.title = draft.title;
        self.url = draft.url;
        self.body = text_editor::Content::with_text(&draft.body);
        self.alt_text = draft.alt_text;
        self.image = draft.image;
        self.nsfw = draft.nsfw;
    }

    fn save_draft(&mut self) {
        let Some(path) = &self.draft_path else { return };
        let draft = Draft {
            community: self.community.clone(),
            manual_community: self.manual_community,
            title: self.title.clone(),
            url: self.url.clone(),
            body: self.body.text(),
            alt_text: self.alt_text.clone(),
            image: self.image,
            nsfw: self.nsfw,
        };
        self.draft_error = local::write(path, &draft)
            .err()
            .map(|error| fl!("draft-save-failed", error = error.to_string()));
    }

    fn remove_draft(&self) -> Result<(), std::io::Error> {
        self.draft_path.as_deref().map_or(Ok(()), local::remove)
    }

    pub fn editing(&self) -> bool {
        matches!(self.mode, Mode::Edit(_))
    }

    pub fn open(&mut self, community: Option<&Community>) {
        if self.community.is_empty()
            && let Some(community) = community
        {
            self.community = community_handle(community);
        }
        self.visible = true;
    }

    pub fn update(
        &mut self,
        message: Message,
        api: Option<Api>,
        follows: &[Community],
        session: u64,
    ) -> (Task<Message>, Option<PostView>) {
        match message {
            Message::Submitted(request_session, result) if request_session == session => {
                self.busy = false;
                match result {
                    Ok(post) => {
                        let draft_error = self.remove_draft().err().map(|error| {
                            fl!("draft-published-remove-failed", error = error.to_string())
                        });
                        *self = Self {
                            draft_error,
                            ..Self::default()
                        };
                        return (Task::none(), Some(*post));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::UploadFinished(request_session, result) if request_session == session => {
                self.busy = false;
                self.uploading = false;
                match result {
                    Ok(Some(url)) => {
                        self.url = url;
                        self.image = true;
                        self.save_draft();
                    }
                    Ok(None) => {}
                    Err(error) => self.error = Some(error),
                }
            }
            _ if self.busy || !self.visible => {}
            Message::Close => {
                self.save_draft();
                self.visible = false;
            }
            Message::Discard => match self.remove_draft() {
                Ok(()) => *self = Self::default(),
                Err(error) => {
                    self.draft_error = Some(fl!("draft-discard-failed", error = error.to_string()))
                }
            },
            Message::TogglePreview => {
                self.preview = if self.preview.is_none() {
                    Some(markdown::Content::parse(&self.body.text()))
                } else {
                    None
                };
            }
            Message::UploadImage => {
                let Some(api) = api.filter(Api::is_authenticated) else {
                    return (Task::none(), None);
                };
                self.busy = true;
                self.uploading = true;
                self.error = None;
                return (
                    Task::perform(upload::choose_image(api), move |result| {
                        Message::UploadFinished(session, result)
                    }),
                    None,
                );
            }
            Message::Community(value) => {
                self.community = value;
                self.save_draft();
            }
            Message::PickCommunity(index) => {
                if let Some(community) = follows.get(index) {
                    self.community = community_handle(community);
                    self.manual_community = false;
                } else if index == follows.len() {
                    self.community.clear();
                    self.manual_community = true;
                }
                self.save_draft();
            }
            Message::Title(value) => {
                self.title = value;
                self.save_draft();
            }
            Message::Url(value) => {
                self.url = value;
                self.image = false;
                self.save_draft();
            }
            Message::RemoveImage => {
                self.url.clear();
                self.alt_text.clear();
                self.image = false;
                self.save_draft();
            }
            Message::AltText(value) => {
                self.alt_text = value;
                self.save_draft();
            }
            Message::Body(action) => {
                let edited = matches!(&action, text_editor::Action::Edit(_));
                self.body.perform(action);
                if edited {
                    self.save_draft();
                }
            }
            Message::Submit if self.can_submit() => {
                let Some(api) = api.filter(Api::is_authenticated) else {
                    return (Task::none(), None);
                };
                let url = self.url.trim();
                if !url.is_empty() && !crate::shared::media::web_url(url) {
                    self.error = Some(fl!("link-invalid"));
                    return (Task::none(), None);
                }
                let name = self.title.trim().to_owned();
                let community = self.community.trim().trim_start_matches('!').to_owned();
                let community_id = follows
                    .iter()
                    .find(|item| community_handle(item) == community)
                    .map(|item| item.id);
                let url = url.to_owned();
                let body = self.body.text();
                let alt_text = self.alt_text.clone();
                let mode = self.mode;
                let nsfw = self.nsfw;
                self.busy = true;
                self.error = None;
                let request = async move {
                    if let Mode::Edit(post_id) = mode {
                        edit(
                            api,
                            EditPost {
                                post_id,
                                name: Some(name),
                                url: Some(url),
                                body: Some(body),
                                alt_text: Some(alt_text),
                                ..Default::default()
                            },
                        )
                        .await
                    } else {
                        let source = match mode {
                            Mode::Cross { community, .. } => Some(community),
                            _ => None,
                        };
                        create(
                            api,
                            community,
                            community_id,
                            source,
                            CreatePost {
                                name,
                                url: (!url.is_empty()).then_some(url),
                                body: (!body.trim().is_empty()).then_some(body),
                                alt_text: (!alt_text.trim().is_empty()).then_some(alt_text),
                                nsfw,
                                ..Default::default()
                            },
                        )
                        .await
                    }
                };
                return (
                    Task::perform(request, move |result| {
                        Message::Submitted(session, result.map(Box::new))
                    }),
                    None,
                );
            }
            _ => {}
        }
        (Task::none(), None)
    }

    fn can_submit(&self) -> bool {
        !self.title.trim().is_empty()
            && (self.editing() || !self.community.trim().trim_start_matches('!').is_empty())
    }

    pub fn image_url(&self) -> Option<&str> {
        let url = self.url.trim();
        (self.image || url::Url::parse(url).is_ok_and(|url| media::looks_like_image(&url)))
            .then_some(url)
            .filter(|url| media::web_url(url))
    }

    fn image_preview(&self, media: &Media, url: &str) -> Element<'_, Message> {
        let height = Length::Fixed(230.0);
        if let Some(image) = media
            .get(url, Size::Preview)
            .or_else(|| media.get(url, Size::Thumbnail))
        {
            media::image_view(image, Length::Fill, height).into()
        } else {
            let status = if media.preview_pending(url) {
                fl!("loading-image")
            } else {
                fl!("image-preview-failed")
            };
            widget::container(widget::text::caption(status))
                .center_x(Length::Fill)
                .center_y(height)
                .into()
        }
    }

    fn form(&self, follows: &[Community], media: &Media) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let mut controls = widget::column::with_capacity(8).spacing(space.space_xxs);
        if self.editing() {
            controls = controls.push(widget::text::caption(&self.community));
        } else {
            let manual = if follows.is_empty() {
                true
            } else {
                let mut choices: Vec<_> = follows.iter().map(community_label).collect();
                let selected = follows
                    .iter()
                    .position(|community| community_handle(community) == self.community);
                let manual =
                    self.manual_community || (!self.community.is_empty() && selected.is_none());
                let selected = if manual {
                    Some(choices.len())
                } else {
                    selected
                };
                choices.push(fl!("other-community"));
                controls = controls.push(widget::text::body(fl!("community"))).push(
                    widget::dropdown(choices, selected, Message::PickCommunity)
                        .placeholder(fl!("community-choose")),
                );
                manual
            };
            if manual {
                let input =
                    widget::text_input(fl!("community-handle-placeholder"), &self.community)
                        .on_input(Message::Community);
                controls = controls.push(if follows.is_empty() {
                    input.label(fl!("community"))
                } else {
                    input
                });
            }
        }
        let editor = crate::shared::presentation::text_editor(&self.body, fl!("write-in-markdown"))
            .height(Length::Fixed(110.0));
        let editor = if self.busy {
            editor
        } else {
            editor.on_action(Message::Body)
        };
        controls = controls.push(
            widget::text_input(fl!("post-title"), &self.title)
                .label(fl!("title"))
                .on_input(Message::Title),
        );
        let image_url = self.image_url();
        if let Some(url) = image_url {
            controls = controls.push(self.image_preview(media, url));
        } else {
            controls = controls.push(
                widget::text_input("https://", &self.url)
                    .label(fl!("post-url-placeholder"))
                    .on_input(Message::Url),
            );
        }
        let upload_label = if self.uploading {
            fl!("uploading")
        } else if image_url.is_some() {
            fl!("replace-image")
        } else {
            fl!("upload-image")
        };
        let mut image_actions = widget::row::with_capacity(2).spacing(space.space_xxs).push(
            button::standard(upload_label)
                .on_press_maybe((!self.busy).then_some(Message::UploadImage)),
        );
        if image_url.is_some() {
            image_actions = image_actions.push(
                button::text(fl!("remove-image"))
                    .on_press_maybe((!self.busy).then_some(Message::RemoveImage)),
            );
            controls = controls.push(image_actions).push(
                widget::text_input(fl!("image-description"), &self.alt_text)
                    .label(fl!("image-description-optional"))
                    .on_input(Message::AltText),
            );
        } else {
            controls = controls.push(image_actions);
        }
        controls
            .push(widget::text::body(fl!("body")))
            .push(editor)
            .into()
    }

    fn post_preview<'a>(
        &'a self,
        body: &'a markdown::Content,
        follows: &[Community],
        media: &Media,
    ) -> Element<'a, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let mut content = widget::column::with_capacity(5)
            .spacing(space.space_xs)
            .width(Length::Fill);
        if !self.community.is_empty() {
            let community = follows
                .iter()
                .find(|community| community_handle(community) == self.community)
                .map_or_else(|| self.community.clone(), community_label);
            content = content.push(widget::text::caption(community));
        }
        let title = self.title.trim();
        content = content.push(
            widget::text::title2(if title.is_empty() {
                fl!("untitled-post").into()
            } else {
                std::borrow::Cow::Borrowed(title)
            })
            .wrapping(Wrapping::WordOrGlyph),
        );
        if let Some(url) = self.image_url() {
            content = content.push(self.image_preview(media, url));
            if !self.alt_text.trim().is_empty() {
                content = content
                    .push(widget::text::caption(&self.alt_text).wrapping(Wrapping::WordOrGlyph));
            }
        } else if !self.url.trim().is_empty() {
            let url = self.url.trim();
            content = content
                .push(button::link(url.to_owned()).on_press_maybe(
                    media::web_url(url).then(|| Message::OpenLink(url.to_owned())),
                ));
        }
        if !body.items().is_empty() {
            content = content.push(super::post::render_markdown(body, Message::OpenLink));
        }
        widget::container(content)
            .class(cosmic::theme::Container::Card)
            .padding(space.space_s)
            .width(Length::Fill)
            .into()
    }

    pub fn view(&self, follows: &[Community], media: &Media) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let title = match self.mode {
            Mode::New => fl!("new-post"),
            Mode::Edit(_) => fl!("edit-post"),
            Mode::Cross { .. } => fl!("cross-post"),
        };
        let header = widget::row::with_capacity(2)
            .align_y(Alignment::Center)
            .spacing(space.space_xxs)
            .push(widget::text::title3(title).width(Length::Fill))
            .push(
                button::standard(if self.preview.is_some() {
                    fl!("back-to-editing")
                } else {
                    fl!("preview-post")
                })
                .on_press_maybe((!self.busy).then_some(Message::TogglePreview)),
            );
        let body = match &self.preview {
            Some(preview) => self.post_preview(preview, follows, media),
            None => self.form(follows, media),
        };
        let mut controls = widget::column::with_capacity(3)
            .spacing(space.space_xxs)
            .push(body);
        for error in [&self.error, &self.draft_error].into_iter().flatten() {
            controls = controls.push(widget::text::body(error));
        }
        widget::dialog()
            .control(header)
            .max_width(520)
            .height(Length::Fill)
            .max_height(650)
            .control(widget::scrollable(controls).height(Length::Fill))
            .primary_action(
                button::suggested(match (self.busy, self.editing(), self.uploading) {
                    (true, _, true) => fl!("uploading"),
                    (true, true, _) => fl!("saving"),
                    (true, false, _) => fl!("publishing"),
                    (false, true, _) => fl!("save-changes"),
                    (false, false, _) => fl!("publish"),
                })
                .on_press_maybe((!self.busy && self.can_submit()).then_some(Message::Submit)),
            )
            .secondary_action(
                button::standard(fl!("close"))
                    .on_press_maybe((!self.busy).then_some(Message::Close)),
            )
            .tertiary_action(
                button::text(fl!("discard-draft"))
                    .on_press_maybe((!self.busy).then_some(Message::Discard)),
            )
            .into()
    }
}

async fn create(
    api: Api,
    community: String,
    community_id: Option<CommunityId>,
    source: Option<CommunityId>,
    mut form: CreatePost,
) -> Result<PostView, String> {
    let community_id = match community_id {
        Some(id) => id,
        None => {
            let response: GetCommunityResponse = api
                .get(
                    "community",
                    &GetCommunity {
                        id: None,
                        name: Some(community),
                    },
                )
                .await?;
            response.community_view.community.id
        }
    };
    if Some(community_id) == source {
        return Err(fl!("cross-post-choose-community"));
    }
    form.community_id = community_id;
    let response: PostResponse = api.post("post", &form).await?;
    if response.post_view.post.community_id != community_id {
        return Err(fl!("post-community-mismatch"));
    }
    Ok(response.post_view)
}

async fn edit(api: Api, form: EditPost) -> Result<PostView, String> {
    let response: PostResponse = api.put("post", &form).await?;
    if response.post_view.post.id != form.post_id {
        return Err(fl!("post-mismatch"));
    }
    Ok(response.post_view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{features::post::tests::post_view, shared::api::tests::serve};
    use cosmic::iced::{futures::StreamExt, runtime};
    use serde_json::{Value, json};

    #[tokio::test]
    async fn publish_draft() {
        let post = post_view();
        let follows = [post.community.clone()];
        let response = json!({"post_view": post}).to_string();
        for success in [true, false] {
            let status = if success { "200 OK" } else { "403 Forbidden" };
            let (api, request) = serve(status, &response).await;
            let mut composer = Publish::default();
            composer.open(Some(&follows[0]));
            composer.title = "Example post".into();
            composer.body = text_editor::Content::with_text("Draft content");
            let (task, _) = composer.update(
                Message::Submit,
                Some(api.authenticated("test-account".into())),
                &follows,
                1,
            );
            let mut stream = runtime::task::into_stream(task).unwrap();
            let Some(runtime::Action::Output(message @ Message::Submitted(..))) =
                stream.next().await
            else {
                panic!("submission did not finish");
            };
            let (_, created) = composer.update(message, None, &follows, 1);
            assert_eq!(created.is_some(), success);
            assert_eq!(composer.visible, !success);
            if !success {
                assert_eq!(composer.body.text(), "Draft content");
                assert!(composer.error.as_deref().unwrap().contains("403"));
            }
            let request = request.await.unwrap();
            assert!(request.starts_with("POST /api/v3/post HTTP/1.1"));
            assert!(request.contains("authorization: Bearer test-account\r\n"));
            let (_, body) = request.split_once("\r\n\r\n").unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(body).unwrap(),
                json!({"name": "Example post", "community_id": 2, "body": "Draft content"})
            );
        }
    }
    #[test]
    fn draft_persistence() {
        let account = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let post = post_view();
        let mut composer = Publish::default();
        composer.open_new(&account, Some(&post.community));
        let _ = composer.update(Message::Title("My draft".into()), None, &[], 1);
        let _ = composer.update(
            Message::Body(text_editor::Action::Edit(text_editor::Edit::Paste(
                "**Draft body**".to_owned().into(),
            ))),
            None,
            &[],
            1,
        );
        let image_url = "https://example.com/pictrs/image/alias";
        let _ = composer.update(
            Message::UploadFinished(1, Ok(Some(image_url.into()))),
            None,
            &[],
            1,
        );
        let _ = composer.update(Message::Close, None, &[], 1);

        let mut reopened = Publish::default();
        reopened.open_new(&account, None);
        assert_eq!(reopened.title, "My draft");
        assert_eq!(reopened.body.text(), "**Draft body**");
        assert_eq!(reopened.image_url(), Some(image_url));
        let _ = reopened.update(Message::TogglePreview, None, &[], 1);
        assert!(!reopened.preview.as_ref().unwrap().items().is_empty());
        let _ = reopened.update(Message::TogglePreview, None, &[], 1);
        let _ = reopened.update(Message::RemoveImage, None, &[], 1);
        assert!(reopened.image_url().is_none());
        assert!(reopened.url.is_empty());

        reopened.open_edit(&account, &post);
        assert_eq!(reopened.title, post.post.name);
        let _ = reopened.update(Message::Title("Separate edit".into()), None, &[], 1);
        reopened.open_new(&other, None);
        assert!(reopened.title.is_empty());
        reopened.open_new(&account, None);
        assert_eq!(reopened.title, "My draft");
        let _ = reopened.update(Message::Discard, None, &[], 1);
        assert!(!drafts::path(&account, Mode::New).unwrap().exists());

        reopened.open_edit(&account, &post);
        assert_eq!(reopened.title, "Separate edit");
        let _ = reopened.update(Message::Discard, None, &[], 1);
    }

    #[tokio::test]
    async fn edit_post_fields() {
        let mut post = post_view();
        post.post.name = "Updated title".into();
        let response = json!({"post_view": post}).to_string();
        let (api, request) = serve("200 OK", &response).await;
        let mut composer = Publish {
            visible: true,
            title: "Updated title".into(),
            mode: Mode::Edit(post.post.id),
            ..Publish::default()
        };
        let (task, _) = composer.update(
            Message::Submit,
            Some(api.authenticated("reader".into())),
            &[],
            1,
        );
        let mut stream = runtime::task::into_stream(task).unwrap();
        let Some(runtime::Action::Output(message @ Message::Submitted(..))) = stream.next().await
        else {
            panic!("edit did not finish");
        };
        let (_, updated) = composer.update(message, None, &[], 1);
        assert_eq!(updated, Some(post));
        let request = request.await.unwrap();
        assert!(request.starts_with("PUT /api/v3/post HTTP/1.1"));
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap(),
            json!({
                "post_id": 17, "name": "Updated title", "url": "", "body": "", "alt_text": ""
            })
        );
    }

    #[tokio::test]
    async fn cross_post() {
        let source = post_view();
        let mut created = source.clone();
        created.community.id = CommunityId(3);
        created.community.name = "destination".into();
        created.post.community_id = created.community.id;
        let response = json!({"post_view": created}).to_string();
        let (api, request) = serve("200 OK", &response).await;
        let api = api.authenticated("reader".into());
        let account = uuid::Uuid::new_v4().to_string();
        let mut composer = Publish::default();
        composer.open_cross(&account, &source);
        composer.community = community_handle(&source.community);
        let follows = [source.community.clone(), created.community.clone()];

        let (task, _) = composer.update(Message::Submit, Some(api.clone()), &follows, 1);
        let mut stream = runtime::task::into_stream(task).unwrap();
        let Some(runtime::Action::Output(message @ Message::Submitted(..))) = stream.next().await
        else {
            panic!("cross-post validation did not finish");
        };
        let (_, result) = composer.update(message, None, &follows, 1);
        assert!(result.is_none());
        assert!(
            composer
                .error
                .as_deref()
                .unwrap()
                .contains("different community")
        );

        composer.community = community_handle(&created.community);
        let (task, _) = composer.update(Message::Submit, Some(api), &follows, 1);
        let mut stream = runtime::task::into_stream(task).unwrap();
        let Some(runtime::Action::Output(message @ Message::Submitted(..))) = stream.next().await
        else {
            panic!("cross-post did not finish");
        };
        let (_, result) = composer.update(message, None, &follows, 1);
        assert_eq!(result, Some(created));
        let request = request.await.unwrap();
        assert!(request.starts_with("POST /api/v3/post HTTP/1.1"));
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["community_id"], 3);
        assert_eq!(body["url"], source.post.ap_id.as_str());
        assert!(
            body["body"]
                .as_str()
                .unwrap()
                .contains(&format!("[the original discussion]({})", source.post.ap_id))
        );
    }
}
