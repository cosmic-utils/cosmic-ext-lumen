//! Account-scoped replies and mentions.
use crate::shared::{api::Api, presentation::age};
use cosmic::{
    Element,
    iced::{Alignment, Length, Task, core::text::Wrapping},
    theme::Container,
    widget::{self, button, icon},
};
use lemmy_api_common::{
    lemmy_db_schema::{
        CommentSortType,
        newtypes::{CommentId, CommentReplyId, PersonMentionId, PostId},
    },
    lemmy_db_views::structs::PostView,
    person::{
        CommentReplyResponse, GetPersonMentions, GetPersonMentionsResponse, GetReplies,
        GetRepliesResponse, GetUnreadCountResponse, MarkCommentReplyAsRead,
        MarkPersonMentionAsRead, PersonMentionResponse,
    },
};

const PAGE_SIZE: i64 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeId {
    Reply(CommentReplyId),
    Mention(PersonMentionId),
}

#[derive(Clone, Debug)]
pub struct Notice {
    id: NoticeId,
    comment: CommentId,
    post: PostId,
    title: String,
    author: String,
    published: i64,
    read: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    Refresh,
    More,
    Open(NoticeId),
    SetRead(NoticeId, bool),
    ReadUpdated(u64, NoticeId, Result<bool, String>),
    Loaded(u64, bool, Result<Vec<Notice>, String>),
    Opened(
        u64,
        NoticeId,
        Result<(Box<PostView>, CommentId, Option<String>), String>,
    ),
    Unread(u64, u64, Result<i64, String>),
    Scrolled(f32),
}

#[derive(Default)]
pub struct Inbox {
    api: Option<Api>,
    generation: u64,
    notices: Vec<Notice>,
    page: i64,
    more: bool,
    loaded: bool,
    busy: bool,
    opening: Option<NoticeId>,
    updating_read: Option<NoticeId>,
    open_on_arrival: bool,
    unread_request: u64,
    error: Option<String>,
    pub unread: i64,
}

impl Inbox {
    pub fn connect(&mut self, api: &Api) -> Task<Message> {
        let generation = self.generation + 1;
        *self = Self {
            generation,
            api: api.is_authenticated().then(|| api.clone()),
            page: 1,
            ..Default::default()
        };
        self.refresh_unread()
    }

    fn refresh_unread(&mut self) -> Task<Message> {
        let Some(api) = self.api.clone() else {
            return Task::none();
        };
        let generation = self.generation;
        self.unread_request += 1;
        let request = self.unread_request;
        Task::perform(
            async move {
                let count: GetUnreadCountResponse = api.get("user/unread_count", &()).await?;
                Ok(count.replies + count.mentions)
            },
            move |result| Message::Unread(generation, request, result),
        )
    }

    pub fn dismiss_open(&mut self) {
        self.open_on_arrival = false;
    }

    fn load(&mut self, append: bool) -> Task<Message> {
        let Some(api) = self.api.clone() else {
            return Task::none();
        };
        self.busy = true;
        self.error = None;
        let page = if append { self.page } else { 1 };
        let generation = self.generation;
        Task::perform(load(api, page), move |result| {
            Message::Loaded(generation, append, result)
        })
    }

    fn can_act(&self) -> bool {
        !self.busy && self.opening.is_none() && self.updating_read.is_none()
    }

    fn apply_read(&mut self, id: NoticeId, read: bool) {
        if let Some(notice) = self.notices.iter_mut().find(|notice| notice.id == id)
            && notice.read != read
        {
            notice.read = read;
            self.unread = if read {
                self.unread.saturating_sub(1).max(0)
            } else {
                self.unread.saturating_add(1)
            };
        }
    }

    pub fn update(&mut self, message: Message) -> (Task<Message>, Option<(PostView, CommentId)>) {
        match message {
            Message::Refresh if self.can_act() => {
                return (self.load(false), None);
            }
            Message::More if self.more && self.can_act() => {
                return (self.load(true), None);
            }
            Message::Open(id) if self.can_act() => {
                if let (Some(api), Some(notice)) = (
                    self.api.clone(),
                    self.notices.iter().find(|notice| notice.id == id).cloned(),
                ) {
                    self.opening = Some(id);
                    if !notice.read {
                        self.unread_request += 1;
                    }
                    self.open_on_arrival = true;
                    self.error = None;
                    let generation = self.generation;
                    return (
                        Task::perform(open(api, notice), move |result| {
                            Message::Opened(generation, id, result)
                        }),
                        None,
                    );
                }
            }
            Message::SetRead(id, read) if self.can_act() => {
                if let Some(api) = self.api.clone()
                    && self
                        .notices
                        .iter()
                        .any(|notice| notice.id == id && notice.read != read)
                {
                    self.updating_read = Some(id);
                    self.unread_request += 1;
                    self.error = None;
                    let generation = self.generation;
                    return (
                        Task::perform(
                            async move { set_read(&api, id, read).await },
                            move |result| Message::ReadUpdated(generation, id, result),
                        ),
                        None,
                    );
                }
            }
            Message::ReadUpdated(generation, id, result)
                if generation == self.generation && self.updating_read == Some(id) =>
            {
                self.updating_read = None;
                match result {
                    Ok(read) => {
                        self.apply_read(id, read);
                    }
                    Err(error) => {
                        self.error = Some(error);
                        return (self.refresh_unread(), None);
                    }
                }
                return (self.refresh_unread(), None);
            }
            Message::Loaded(generation, append, result) if generation == self.generation => {
                self.busy = false;
                match result {
                    Ok(notices) => {
                        self.loaded = true;
                        self.more = notices
                            .iter()
                            .filter(|notice| matches!(notice.id, NoticeId::Reply(_)))
                            .count()
                            == PAGE_SIZE as usize
                            || notices
                                .iter()
                                .filter(|notice| matches!(notice.id, NoticeId::Mention(_)))
                                .count()
                                == PAGE_SIZE as usize;
                        self.page = if append { self.page + 1 } else { 2 };
                        if !append {
                            self.notices.clear();
                        }
                        for notice in notices {
                            if !self.notices.iter().any(|current| current.id == notice.id) {
                                self.notices.push(notice);
                            }
                        }
                        self.notices
                            .sort_by_key(|notice| std::cmp::Reverse(notice.published));
                        if !append {
                            return (self.refresh_unread(), None);
                        }
                    }
                    Err(error) => {
                        self.error = Some(error);
                        return (self.refresh_unread(), None);
                    }
                }
            }
            Message::Opened(generation, id, result)
                if generation == self.generation && self.opening == Some(id) =>
            {
                self.opening = None;
                let open = std::mem::take(&mut self.open_on_arrival);
                match result {
                    Ok((post, comment, error)) => {
                        self.error = error;
                        if self.error.is_none() {
                            self.apply_read(id, true);
                        }
                        return (self.refresh_unread(), open.then_some((*post, comment)));
                    }
                    Err(error) => {
                        self.error = Some(error);
                        return (self.refresh_unread(), None);
                    }
                }
            }
            Message::Unread(generation, request, Ok(count))
                if generation == self.generation && request == self.unread_request =>
            {
                self.unread = count
            }
            _ => {}
        }
        (Task::none(), None)
    }

    pub fn view(&self, selected: Option<CommentId>, account_busy: bool) -> Element<'_, Message> {
        let theme = cosmic::theme::active();
        let space = theme.cosmic().spacing;
        let radius = theme.cosmic().corner_radii.radius_s;
        let header = widget::row::with_capacity(3)
            .align_y(Alignment::Center)
            .push(widget::text::title3("Inbox"))
            .push(widget::space().width(Length::Fill))
            .push(
                button::icon(icon::from_name("view-refresh-symbolic"))
                    .tooltip("Refresh inbox")
                    .on_press_maybe(self.can_act().then_some(Message::Refresh)),
            );
        let mut content =
            widget::column::with_capacity(self.notices.len() + 2).spacing(space.space_xxs);
        if let Some(error) = &self.error {
            content = content.push(widget::text::body(error));
        } else if self.notices.is_empty() {
            let label = if self.busy {
                "Loading inbox…"
            } else if self.loaded {
                "No replies or mentions."
            } else {
                "Refresh to load your inbox."
            };
            content = content.push(widget::text::body(label));
        }
        let can_open = self.can_act() && !account_busy;
        for notice in &self.notices {
            let action = match notice.id {
                NoticeId::Reply(_) => "replied",
                NoticeId::Mention(_) => "mentioned you",
            };
            let status = if notice.read { "" } else { " · Unread" };
            let title = button::custom(
                widget::text::body(&notice.title)
                    .font(cosmic::font::semibold())
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::Fill),
            )
            .class(button::ButtonClass::ListItem(radius))
            .padding([space.space_xxs, space.space_xs])
            .width(Length::Fill)
            .selected(selected == Some(notice.comment))
            .on_press_maybe(can_open.then_some(Message::Open(notice.id)));
            let (symbol, label) = if notice.read {
                ("image-red-eye-symbolic", "Mark as unread")
            } else {
                ("document-properties-symbolic", "Mark as read")
            };
            let read_control = button::icon(icon::from_name(symbol))
                .tooltip(label)
                .on_press_maybe(can_open.then_some(Message::SetRead(notice.id, !notice.read)));
            let metadata = widget::row::with_capacity(2)
                .spacing(space.space_xxs)
                .align_y(Alignment::Center)
                .push(
                    widget::text::caption(format!(
                        "{} {action} · {}{status}",
                        notice.author,
                        age(notice.published)
                    ))
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(Length::Fill),
                )
                .push(read_control);
            let card = widget::column::with_capacity(2)
                .push(title)
                .push(widget::container(metadata).padding([0, space.space_xs]));
            content = content.push(
                widget::container(card)
                    .class(Container::Card)
                    .width(Length::Fill),
            );
        }
        if self.opening.is_some() {
            content = content.push(widget::text::caption("Opening discussion…"));
        }
        if self.more && !self.busy {
            content = content.push(
                button::standard("Load more")
                    .on_press_maybe(self.can_act().then_some(Message::More)),
            );
        }
        widget::column::with_capacity(2)
            .spacing(space.space_xs)
            .push(header)
            .push(
                widget::scrollable(widget::container(content).padding([0, space.space_s, 0, 0]))
                    .id(super::feed::SCROLL_ID)
                    .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
                    .height(Length::Fill),
            )
            .height(Length::Fill)
            .into()
    }
}

async fn load(api: Api, page: i64) -> Result<Vec<Notice>, String> {
    let reply_query = GetReplies {
        page: Some(page),
        limit: Some(PAGE_SIZE),
        sort: Some(CommentSortType::New),
        ..Default::default()
    };
    let mention_query = GetPersonMentions {
        page: Some(page),
        limit: Some(PAGE_SIZE),
        sort: Some(CommentSortType::New),
        ..Default::default()
    };
    let (replies, mentions): (GetRepliesResponse, GetPersonMentionsResponse) = cosmic::iced::futures::try_join!(
        api.get("user/replies", &reply_query),
        api.get("user/mention", &mention_query)
    )?;
    Ok(replies
        .replies
        .into_iter()
        .map(|item| Notice {
            id: NoticeId::Reply(item.comment_reply.id),
            comment: item.comment.id,
            post: item.post.id,
            title: item.post.name,
            author: item.creator.name,
            published: item.comment_reply.published.timestamp(),
            read: item.comment_reply.read,
        })
        .chain(mentions.mentions.into_iter().map(|item| Notice {
            id: NoticeId::Mention(item.person_mention.id),
            comment: item.comment.id,
            post: item.post.id,
            title: item.post.name,
            author: item.creator.name,
            published: item.person_mention.published.timestamp(),
            read: item.person_mention.read,
        }))
        .collect())
}

async fn open(
    api: Api,
    notice: Notice,
) -> Result<(Box<PostView>, CommentId, Option<String>), String> {
    let post = super::post::get_post(&api, notice.post).await?;
    let error = if notice.read {
        None
    } else {
        set_read(&api, notice.id, true).await.err()
    };
    Ok((Box::new(post), notice.comment, error))
}

async fn set_read(api: &Api, id: NoticeId, read: bool) -> Result<bool, String> {
    let (updated_id, updated_read) = match id {
        NoticeId::Reply(comment_reply_id) => {
            let response: CommentReplyResponse = api
                .post(
                    "comment/mark_as_read",
                    &MarkCommentReplyAsRead {
                        comment_reply_id,
                        read,
                    },
                )
                .await?;
            let reply = response.comment_reply_view.comment_reply;
            (NoticeId::Reply(reply.id), reply.read)
        }
        NoticeId::Mention(person_mention_id) => {
            let response: PersonMentionResponse = api
                .post(
                    "user/mention/mark_as_read",
                    &MarkPersonMentionAsRead {
                        person_mention_id,
                        read,
                    },
                )
                .await?;
            let mention = response.person_mention_view.person_mention;
            (NoticeId::Mention(mention.id), mention.read)
        }
    };
    if updated_id != id || updated_read != read {
        return Err("The instance did not confirm the requested read status.".into());
    }
    Ok(updated_read)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        features::post::tests::{comment_view, post_view},
        shared::api::tests::serve,
    };
    use cosmic::iced::{futures::StreamExt, runtime};
    use serde_json::{Value, json};

    #[tokio::test]
    async fn read_controls_use_the_active_account_and_wait_for_confirmation() {
        let post = post_view();
        let comment = comment_view(&post, "Inbox comment");
        for (id, endpoint, view_key, notice_key, id_key, read) in [
            (
                NoticeId::Reply(CommentReplyId(9)),
                "comment/mark_as_read",
                "comment_reply_view",
                "comment_reply",
                "comment_reply_id",
                true,
            ),
            (
                NoticeId::Mention(PersonMentionId(9)),
                "user/mention/mark_as_read",
                "person_mention_view",
                "person_mention",
                "person_mention_id",
                false,
            ),
        ] {
            let mut view = serde_json::to_value(&comment).unwrap();
            view["recipient"] = json!(post.creator);
            view[notice_key] = json!({
                "id": 9,
                "recipient_id": post.creator.id,
                "comment_id": comment.comment.id,
                "read": read,
                "published": comment.comment.published
            });
            let response = json!({(view_key): view}).to_string();
            let (api, request) = serve("200 OK", &response).await;
            let mut inbox = Inbox::default();
            let _ = inbox.connect(&api.authenticated("reader".into()));
            inbox.unread = i64::from(read);
            inbox.notices.push(Notice {
                id,
                comment: comment.comment.id,
                post: post.post.id,
                title: post.post.name.clone(),
                author: post.creator.name.clone(),
                published: comment.comment.published.timestamp(),
                read: !read,
            });

            let (task, opened) = inbox.update(Message::SetRead(id, read));
            assert!(opened.is_none());
            assert_eq!(inbox.notices[0].read, !read);
            let mut stream = runtime::task::into_stream(task).unwrap();
            let Some(runtime::Action::Output(message)) = stream.next().await else {
                panic!("read status request did not finish");
            };
            let (_, opened) = inbox.update(message.clone());
            assert!(opened.is_none());
            assert_eq!(inbox.notices[0].read, read);
            assert_eq!(inbox.unread, i64::from(!read));

            let request = request.await.unwrap();
            assert!(request.starts_with(&format!("POST /api/v3/{endpoint} HTTP/1.1")));
            assert!(request.contains("authorization: Bearer reader\r\n"));
            let (_, body) = request.split_once("\r\n\r\n").unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(body).unwrap(),
                json!({(id_key): 9, "read": read})
            );

            let _ = inbox.connect(&Api::new(reqwest::Client::new(), "example.com").unwrap());
            let _ = inbox.update(message);
            assert!(inbox.notices.is_empty());
            assert_eq!(inbox.unread, 0);
        }
    }
}
