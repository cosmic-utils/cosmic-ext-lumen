//! Account-scoped replies and mentions.
use crate::shared::{
    api::Api,
    presentation::{age, selectable_card},
};
use cosmic::{
    Element,
    iced::{Alignment, Length, Task},
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
    post::{GetPost, GetPostResponse},
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

    pub fn update(&mut self, message: Message) -> (Task<Message>, Option<(PostView, CommentId)>) {
        match message {
            Message::Refresh if !self.busy && self.opening.is_none() => {
                return (self.load(false), None);
            }
            Message::More if self.more && !self.busy && self.opening.is_none() => {
                return (self.load(true), None);
            }
            Message::Open(id) if self.opening.is_none() && !self.busy => {
                if let (Some(api), Some(notice)) = (
                    self.api.clone(),
                    self.notices.iter().find(|notice| notice.id == id).cloned(),
                ) {
                    self.opening = Some(id);
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
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Opened(generation, id, result) if generation == self.generation => {
                self.opening = None;
                let open = std::mem::take(&mut self.open_on_arrival);
                match result {
                    Ok((post, comment, error)) => {
                        self.error = error;
                        if self.error.is_none()
                            && let Some(notice) =
                                self.notices.iter_mut().find(|notice| notice.id == id)
                            && !notice.read
                        {
                            notice.read = true;
                            self.unread = self.unread.saturating_sub(1).max(0);
                        }
                        return (self.refresh_unread(), open.then_some((*post, comment)));
                    }
                    Err(error) => self.error = Some(error),
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

    pub fn view(&self, selected: Option<CommentId>) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let header = widget::row::with_capacity(3)
            .align_y(Alignment::Center)
            .push(widget::text::title3("Inbox"))
            .push(widget::space().width(Length::Fill))
            .push(
                button::icon(icon::from_name("view-refresh-symbolic"))
                    .tooltip("Refresh inbox")
                    .on_press_maybe(
                        (!self.busy && self.opening.is_none()).then_some(Message::Refresh),
                    ),
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
        for notice in &self.notices {
            let action = match notice.id {
                NoticeId::Reply(_) => "replied",
                NoticeId::Mention(_) => "mentioned you",
            };
            let status = if notice.read { "" } else { " · Unread" };
            let row = widget::column::with_capacity(2)
                .spacing(space.space_xxxs)
                .push(widget::text::caption(format!(
                    "{} {action} · {}{status}",
                    notice.author,
                    age(notice.published)
                )))
                .push(widget::text::body(&notice.title).font(cosmic::font::semibold()))
                .width(Length::Fill);
            content = content.push(selectable_card(
                row,
                space.space_xs,
                selected == Some(notice.comment),
                (self.opening.is_none() && !self.busy).then_some(Message::Open(notice.id)),
            ));
        }
        if self.opening.is_some() {
            content = content.push(widget::text::caption("Opening discussion…"));
        }
        if self.more && !self.busy {
            content = content.push(button::standard("Load more").on_press(Message::More));
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
    let response: GetPostResponse = api
        .get(
            "post",
            &GetPost {
                id: Some(notice.post),
                comment_id: None,
            },
        )
        .await?;
    if response.post_view.post.id != notice.post {
        return Err("The instance returned a different post.".into());
    }
    let mut error = None;
    if !notice.read {
        error = match notice.id {
            NoticeId::Reply(id) => api
                .post::<_, CommentReplyResponse>(
                    "comment/mark_as_read",
                    &MarkCommentReplyAsRead {
                        comment_reply_id: id,
                        read: true,
                    },
                )
                .await
                .err(),
            NoticeId::Mention(id) => api
                .post::<_, PersonMentionResponse>(
                    "user/mention/mark_as_read",
                    &MarkPersonMentionAsRead {
                        person_mention_id: id,
                        read: true,
                    },
                )
                .await
                .err(),
        };
    }
    Ok((Box::new(response.post_view), notice.comment, error))
}
