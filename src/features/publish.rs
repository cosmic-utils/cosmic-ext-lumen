//! Text and link post composer.
use crate::shared::{api::Api, presentation::community_handle};
use cosmic::{
    Element,
    iced::{Length, Task},
    widget::{self, button, text_editor},
};
use lemmy_api_common::{
    community::{GetCommunity, GetCommunityResponse},
    lemmy_db_schema::{newtypes::CommunityId, source::community::Community},
    lemmy_db_views::structs::PostView,
    post::{CreatePost, PostResponse},
};
use url::Url;

#[derive(Clone, Debug)]
pub enum Message {
    Close,
    Community(String),
    PickCommunity(usize),
    Title(String),
    Url(String),
    Body(text_editor::Action),
    Submit,
    Submitted(u64, Result<Box<PostView>, String>),
}

#[derive(Default)]
pub struct Publish {
    pub visible: bool,
    pub busy: bool,
    community: String,
    manual_community: bool,
    title: String,
    url: String,
    body: text_editor::Content<cosmic::Renderer>,
    error: Option<String>,
}

impl Publish {
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
                        *self = Self::default();
                        return (Task::none(), Some(*post));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            _ if self.busy => {}
            Message::Close => self.visible = false,
            Message::Community(value) => self.community = value,
            Message::PickCommunity(index) => {
                if let Some(community) = follows.get(index) {
                    self.community = community_handle(community);
                    self.manual_community = false;
                } else if index == follows.len() {
                    self.community.clear();
                    self.manual_community = true;
                }
            }
            Message::Title(value) => self.title = value,
            Message::Url(value) => self.url = value,
            Message::Body(action) => self.body.perform(action),
            Message::Submit if self.can_submit() => {
                let Some(api) = api.filter(Api::is_authenticated) else {
                    return (Task::none(), None);
                };
                let url = self.url.trim();
                if !url.is_empty()
                    && !Url::parse(url).is_ok_and(|url| {
                        matches!(url.scheme(), "http" | "https") && url.host().is_some()
                    })
                {
                    self.error = Some("Enter a valid HTTP or HTTPS link.".into());
                    return (Task::none(), None);
                }
                let name = self.title.trim().to_owned();
                let community = self.community.trim().trim_start_matches('!').to_owned();
                let community_id = follows
                    .iter()
                    .find(|item| community_handle(item) == community)
                    .map(|item| item.id);
                let url = (!url.is_empty()).then(|| url.to_owned());
                let body = self.body.text();
                let body = (!body.trim().is_empty()).then_some(body);
                self.busy = true;
                self.error = None;
                return (
                    Task::perform(
                        create(api, name, community, community_id, url, body),
                        move |result| Message::Submitted(session, result.map(Box::new)),
                    ),
                    None,
                );
            }
            _ => {}
        }
        (Task::none(), None)
    }

    fn can_submit(&self) -> bool {
        !self.title.trim().is_empty() && !self.community.trim().trim_start_matches('!').is_empty()
    }

    pub fn view(&self, follows: &[Community]) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let mut controls = widget::column::with_capacity(8).spacing(space.space_xxs);
        let manual = if follows.is_empty() {
            true
        } else {
            let mut choices: Vec<_> = follows.iter().map(community_handle).collect();
            let selected = choices.iter().position(|name| name == &self.community);
            let manual =
                self.manual_community || (!self.community.is_empty() && selected.is_none());
            let selected = if manual {
                Some(choices.len())
            } else {
                selected
            };
            choices.push("Other community…".into());
            controls = controls.push(widget::text::body("Community")).push(
                widget::dropdown(choices, selected, Message::PickCommunity)
                    .placeholder("Choose a community"),
            );
            manual
        };
        if manual {
            let input = widget::text_input("Community name or name@instance", &self.community)
                .on_input(Message::Community);
            controls = controls.push(if follows.is_empty() {
                input.label("Community")
            } else {
                input
            });
        }
        let editor = crate::shared::presentation::text_editor(&self.body, "Write in Markdown")
            .height(Length::Fixed(110.0));
        let editor = if self.busy {
            editor
        } else {
            editor.on_action(Message::Body)
        };
        controls = controls
            .push(
                widget::text_input("Post title", &self.title)
                    .label("Title")
                    .on_input(Message::Title),
            )
            .push(
                widget::text_input("https://", &self.url)
                    .label("Link (optional)")
                    .on_input(Message::Url),
            )
            .push(widget::text::body("Body"))
            .push(editor);
        if let Some(error) = &self.error {
            controls = controls.push(widget::text::body(error));
        }
        widget::dialog()
            .title("New post")
            .max_width(480)
            .control(widget::scrollable(controls).height(Length::Shrink))
            .primary_action(
                button::suggested(if self.busy {
                    "Publishing…"
                } else {
                    "Publish"
                })
                .on_press_maybe((!self.busy && self.can_submit()).then_some(Message::Submit)),
            )
            .secondary_action(
                button::standard("Cancel").on_press_maybe((!self.busy).then_some(Message::Close)),
            )
            .into()
    }
}

async fn create(
    api: Api,
    name: String,
    community: String,
    community_id: Option<CommunityId>,
    url: Option<String>,
    body: Option<String>,
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
    let response: PostResponse = api
        .post(
            "post",
            &CreatePost {
                name,
                community_id,
                url,
                body,
                ..Default::default()
            },
        )
        .await?;
    if response.post_view.post.community_id != community_id {
        return Err("The instance returned a post in a different community.".into());
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
    async fn publishes_in_the_selected_community_and_preserves_a_failed_draft() {
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
}
