//! Post reading, authenticated actions, and paginated comment threads.
mod service;
mod thread;
mod view;
pub(crate) use view::render_markdown;

pub const SCROLL_ID: &str = "lemmy-discussion";

use crate::shared::{api::Api, links::open_web_link};
use cosmic::{
    iced::Task,
    widget::{markdown, text_editor},
};
use lemmy_api_common::{
    comment::GetCommentsResponse,
    lemmy_db_schema::{
        CommentSortType,
        newtypes::{CommentId, CommunityId, PostId},
    },
    lemmy_db_views::structs::{CommentView, PostView},
};
use std::collections::{HashMap, HashSet};

pub struct Comment {
    pub data: CommentView,
    pub body: markdown::Content,
    pub path: Vec<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentTarget {
    Post,
    Reply(CommentId),
}

#[derive(Default)]
struct Composer {
    draft: text_editor::Content<cosmic::Renderer>,
    error: Option<String>,
}

struct DiscussionDrafts {
    composer: Composer,
    composer_open: bool,
    replies: HashMap<CommentId, Composer>,
    active_reply: Option<CommentId>,
}

#[derive(Default)]
pub struct Post {
    pub selected: Option<PostView>,
    pub body: markdown::Content,
    pub comments: Vec<Comment>,
    pub collapsed: HashSet<i32>,
    pub busy: bool,
    pub error: Option<String>,
    pub more: bool,
    page: i64,
    generation: u64,
    api: Option<Api>,
    pending_action: Option<(u64, PostId)>,
    action_error: Option<String>,
    composer: Composer,
    replies: HashMap<CommentId, Composer>,
    active_reply: Option<CommentId>,
    submitting: Option<(u64, PostId, CommentTarget)>,
    drafts: HashMap<PostId, DiscussionDrafts>,
    composer_open: bool,
    sort: usize,
    voting: Option<(u64, CommentId)>,
    vote_error: Option<(CommentId, String)>,
    pub focused_comment: Option<CommentId>,
    focus_error: Option<String>,
    link_error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Upvote,
    Downvote,
    ToggleSaved,
}

#[derive(Clone, Debug)]
pub enum Message {
    Back,
    ToggleFocus,
    OpenCommunity(CommunityId),
    CommunityDetails,
    Retry,
    More,
    Sort(usize),
    ToggleComposer,
    Collapse(i32),
    Link(String),
    Loaded(u64, Result<GetCommentsResponse, String>),
    Act(u64, Action),
    ActionFinished(u64, Result<Box<PostView>, String>),
    OpenReply(u64, CommentId),
    CancelReply(u64, CommentId),
    EditComment(u64, CommentTarget, text_editor::Action),
    SubmitComment(u64, CommentTarget),
    CommentSubmitted(u64, Result<Box<CommentView>, String>),
    Vote(u64, CommentId, i16),
    VoteFinished(u64, Result<Box<CommentView>, String>),
    FocusLoaded(u64, Result<Box<CommentView>, String>),
}

const COMMENT_SORTS: [CommentSortType; 3] = [
    CommentSortType::Top,
    CommentSortType::New,
    CommentSortType::Old,
];

impl Post {
    pub fn open(&mut self, api: Api, post: PostView) -> Task<Message> {
        self.close();
        if let Some(drafts) = self.drafts.remove(&post.post.id) {
            self.composer = drafts.composer;
            self.composer_open = drafts.composer_open;
            self.replies = drafts.replies;
            self.active_reply = drafts.active_reply;
        }
        self.api = Some(api);
        self.set_selected(post);
        self.page = 1;
        self.load()
    }

    pub fn open_at(&mut self, api: Api, post: PostView, comment: CommentId) -> Task<Message> {
        let post_id = post.post.id;
        let load = self.open(api.clone(), post);
        self.focused_comment = Some(comment);
        let generation = self.generation;
        Task::batch([
            load,
            Task::perform(service::get_comment(api, comment, post_id), move |result| {
                Message::FocusLoaded(generation, result.map(Box::new))
            }),
        ])
    }

    pub fn reset(&mut self) {
        self.close();
        self.pending_action = None;
        self.voting = None;
        self.submitting = None;
        self.drafts.clear();
    }

    pub fn close(&mut self) {
        let composer = std::mem::take(&mut self.composer);
        let mut replies = std::mem::take(&mut self.replies);
        replies.retain(|_, reply| !reply.draft.text().trim().is_empty());
        if let Some(post) = &self.selected
            && (!composer.draft.text().trim().is_empty() || !replies.is_empty())
        {
            self.drafts.insert(
                post.post.id,
                DiscussionDrafts {
                    composer,
                    composer_open: self.composer_open,
                    active_reply: self.active_reply.filter(|id| replies.contains_key(id)),
                    replies,
                },
            );
        }
        self.generation += 1;
        self.api = None;
        self.selected = None;
        self.comments.clear();
        self.collapsed.clear();
        self.body = markdown::Content::new();
        self.busy = false;
        self.more = false;
        self.error = None;
        self.action_error = None;
        self.active_reply = None;
        self.composer_open = false;
        self.vote_error = None;
        self.focused_comment = None;
        self.focus_error = None;
        self.link_error = None;
    }

    fn set_selected(&mut self, post: PostView) {
        self.body = markdown::Content::parse(post.post.body.as_deref().unwrap_or_default());
        self.selected = Some(post);
    }

    fn act(&mut self, generation: u64, action: Action) -> Task<Message> {
        if generation != self.generation
            || self.pending_action.is_some()
            || self.submitting.is_some()
        {
            return Task::none();
        }
        let (Some(api), Some(post)) = (&self.api, &self.selected) else {
            return Task::none();
        };
        if !api.is_authenticated() {
            return Task::none();
        }
        self.pending_action = Some((generation, post.post.id));
        self.action_error = None;
        let (api, post) = (api.clone(), post.clone());
        Task::perform(
            async move { service::act(api, &post, action).await.map(Box::new) },
            move |result| Message::ActionFinished(generation, result),
        )
    }

    fn composer_mut(&mut self, target: CommentTarget) -> Option<&mut Composer> {
        match target {
            CommentTarget::Post => Some(&mut self.composer),
            CommentTarget::Reply(id) if self.active_reply == Some(id) => self.replies.get_mut(&id),
            CommentTarget::Reply(_) => None,
        }
    }

    fn submit_comment(&mut self, generation: u64, target: CommentTarget) -> Task<Message> {
        if generation != self.generation
            || self.submitting.is_some()
            || self.pending_action.is_some()
        {
            return Task::none();
        }
        let (Some(api), Some(post)) = (&self.api, &self.selected) else {
            return Task::none();
        };
        if !api.is_authenticated() || post.post.locked || post.post.deleted || post.post.removed {
            return Task::none();
        }
        let (api, post_id) = (api.clone(), post.post.id);
        let Some(composer) = self.composer_mut(target) else {
            return Task::none();
        };
        let content = composer.draft.text();
        if content.trim().is_empty() {
            return Task::none();
        }
        composer.error = None;
        self.submitting = Some((generation, post_id, target));
        Task::perform(
            service::create_comment(api, post_id, target, content),
            move |result| Message::CommentSubmitted(generation, result.map(Box::new)),
        )
    }

    pub fn finish_comment(
        &mut self,
        generation: u64,
        result: Result<Box<CommentView>, String>,
    ) -> Option<PostId> {
        let (_, post_id, target) = self
            .submitting
            .filter(|(request, _, _)| *request == generation)?;
        self.submitting = None;
        if !self
            .selected
            .as_ref()
            .is_some_and(|post| post.post.id == post_id)
        {
            if let Some(drafts) = self.drafts.get_mut(&post_id) {
                match (target, &result) {
                    (CommentTarget::Post, Ok(_)) => {
                        drafts.composer = Composer::default();
                        drafts.composer_open = false;
                    }
                    (CommentTarget::Reply(id), Ok(_)) => {
                        drafts.replies.remove(&id);
                        drafts.active_reply = None;
                    }
                    (CommentTarget::Post, Err(error)) => {
                        drafts.composer.error = Some(error.clone())
                    }
                    (CommentTarget::Reply(id), Err(error)) => {
                        if let Some(reply) = drafts.replies.get_mut(&id) {
                            reply.error = Some(error.clone());
                        }
                    }
                }
            }
            return result.ok().map(|_| post_id);
        }
        match result {
            Ok(comment) => {
                match target {
                    CommentTarget::Post => {
                        self.composer = Composer::default();
                        self.composer_open = false;
                    }
                    CommentTarget::Reply(id) => {
                        self.replies.remove(&id);
                        self.active_reply = None;
                        for ancestor in thread::path_ids(&comment.comment.path)
                            .into_iter()
                            .take_while(|ancestor| *ancestor != comment.comment.id.0)
                        {
                            self.collapsed.remove(&ancestor);
                        }
                    }
                }
                let already_loaded = self
                    .comments
                    .iter()
                    .any(|item| item.data.comment.id == comment.comment.id);
                self.add_comment(*comment);
                self.sort_comments();
                if !already_loaded {
                    self.selected.as_mut()?.counts.comments += 1;
                }
                Some(post_id)
            }
            Err(error) => {
                if let Some(composer) = self.composer_mut(target) {
                    composer.error = Some(error);
                }
                None
            }
        }
    }

    fn submitting_target(&self) -> Option<CommentTarget> {
        let (_, post_id, target) = self.submitting?;
        self.selected
            .as_ref()
            .filter(|post| post.post.id == post_id)
            .map(|_| target)
    }

    fn sort_comments(&mut self) {
        let order = COMMENT_SORTS[self.sort];
        let ranks: HashMap<_, _> = self
            .comments
            .iter()
            .map(|comment| {
                let rank = match order {
                    CommentSortType::New => -comment.data.comment.published.timestamp_micros(),
                    CommentSortType::Old => comment.data.comment.published.timestamp_micros(),
                    _ => -comment.data.counts.score,
                };
                (
                    comment.data.comment.id.0,
                    (!comment.data.comment.distinguished, rank),
                )
            })
            .collect();
        self.comments.sort_by_cached_key(|comment| {
            comment
                .path
                .iter()
                .map(|id| (ranks.get(id).copied().unwrap_or((true, 0)), *id))
                .collect::<Vec<_>>()
        });
    }

    fn vote(&mut self, generation: u64, id: CommentId, direction: i16) -> Task<Message> {
        let Some(api) = self.api.as_ref().filter(|api| api.is_authenticated()) else {
            return Task::none();
        };
        if generation != self.generation || self.voting.is_some() || !matches!(direction, -1 | 1) {
            return Task::none();
        }
        let Some(comment) = self
            .comments
            .iter()
            .find(|comment| comment.data.comment.id == id)
        else {
            return Task::none();
        };
        let score = if comment.data.my_vote == Some(direction) {
            0
        } else {
            direction
        };
        self.voting = Some((generation, id));
        self.vote_error = None;
        Task::perform(service::vote(api.clone(), id, score), move |result| {
            Message::VoteFinished(generation, result.map(Box::new))
        })
    }

    fn add_comment(&mut self, data: CommentView) {
        if self
            .comments
            .iter()
            .any(|c| c.data.comment.id == data.comment.id)
        {
            return;
        }
        let text = if data.comment.deleted {
            "[deleted]"
        } else if data.comment.removed {
            "[removed]"
        } else {
            &data.comment.content
        };
        self.comments.push(Comment {
            body: markdown::Content::parse(text),
            path: thread::path_ids(&data.comment.path),
            data,
        });
    }

    pub fn finish_action(
        &mut self,
        generation: u64,
        result: Result<Box<PostView>, String>,
    ) -> Option<PostView> {
        let (_, post_id) = self.pending_action.filter(|(id, _)| *id == generation)?;
        self.pending_action = None;
        let is_selected = self.selected.as_ref().is_some_and(|p| p.post.id == post_id);
        match result {
            Ok(post) => {
                if is_selected {
                    self.set_selected((*post).clone());
                }
                Some(*post)
            }
            Err(error) => {
                if is_selected {
                    self.action_error = Some(error);
                }
                None
            }
        }
    }

    fn load(&mut self) -> Task<Message> {
        let (Some(api), Some(post)) = (self.api.clone(), &self.selected) else {
            return Task::none();
        };
        self.busy = true;
        self.error = None;
        let generation = self.generation;
        Task::perform(
            service::comments(api, post.post.id, self.page, COMMENT_SORTS[self.sort]),
            move |result| Message::Loaded(generation, result),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Act(generation, action) => return self.act(generation, action),
            Message::Vote(generation, id, score) => return self.vote(generation, id, score),
            Message::VoteFinished(generation, result) => {
                if let Some((_, id)) = self.voting.filter(|(pending, _)| *pending == generation) {
                    self.voting = None;
                    if generation == self.generation {
                        match result {
                            Ok(updated) => {
                                if let Some(comment) = self
                                    .comments
                                    .iter_mut()
                                    .find(|comment| comment.data.comment.id == id)
                                {
                                    comment.data.counts = updated.counts;
                                    comment.data.my_vote = updated.my_vote;
                                }
                            }
                            Err(error) => self.vote_error = Some((id, error)),
                        }
                    }
                }
            }
            Message::FocusLoaded(generation, result) if generation == self.generation => {
                match result {
                    Ok(comment) => {
                        self.add_comment(*comment);
                        self.sort_comments();
                    }
                    Err(error) => self.focus_error = Some(error),
                }
            }
            Message::Sort(sort)
                if !self.busy
                    && self.submitting.is_none()
                    && self.voting.is_none()
                    && sort < COMMENT_SORTS.len()
                    && sort != self.sort =>
            {
                self.sort = sort;
                self.comments
                    .retain(|comment| Some(comment.data.comment.id) == self.focused_comment);
                self.page = 1;
                self.more = false;
                return self.load();
            }
            Message::ToggleComposer if self.submitting.is_none() => {
                self.composer_open = !self.composer_open;
                self.active_reply = None;
            }
            Message::OpenReply(generation, id)
                if generation == self.generation
                    && self.submitting.is_none()
                    && self.api.as_ref().is_some_and(Api::is_authenticated)
                    && self
                        .comments
                        .iter()
                        .any(|comment| comment.data.comment.id == id) =>
            {
                self.replies.entry(id).or_default();
                self.active_reply = Some(id);
                self.composer_open = false;
            }
            Message::CancelReply(generation, id)
                if generation == self.generation
                    && self.submitting.is_none()
                    && self.active_reply == Some(id) =>
            {
                self.replies.remove(&id);
                self.active_reply = None;
            }
            Message::EditComment(generation, target, action)
                if generation == self.generation && self.submitting_target() != Some(target) =>
            {
                if let Some(composer) = self.composer_mut(target) {
                    composer.draft.perform(action);
                }
            }
            Message::SubmitComment(generation, target) => {
                return self.submit_comment(generation, target);
            }
            Message::More if !self.busy && self.more => return self.load(),
            Message::Retry if !self.busy => return self.load(),
            Message::Collapse(id) => {
                if !self.collapsed.remove(&id) {
                    self.collapsed.insert(id);
                }
            }
            Message::Link(url) => {
                self.link_error = open_web_link(&url).err();
            }
            Message::Loaded(generation, result) if generation == self.generation => {
                self.busy = false;
                match result {
                    Ok(response) => {
                        self.more = response.comments.len() == service::PAGE_SIZE;
                        self.page += 1;
                        for data in response.comments {
                            self.add_comment(data);
                        }
                        self.sort_comments();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            _ => {}
        }
        Task::none()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::shared::api::tests::serve;
    use cosmic::iced::{futures::StreamExt, runtime};
    use serde_json::{Value, json};

    pub(crate) fn post_view() -> PostView {
        let published = "2024-01-01T00:00:00Z";
        serde_json::from_value(json!({
            "post": {
                "id": 17, "name": "Example post", "creator_id": 1, "community_id": 2,
                "removed": false, "locked": false, "published": published, "deleted": false,
                "nsfw": false, "ap_id": "https://example.com/post/17", "local": true,
                "language_id": 0, "featured_community": false, "featured_local": false
            },
            "creator": {
                "id": 1, "name": "author", "banned": false, "published": published,
                "actor_id": "https://example.com/u/author", "local": true, "deleted": false,
                "bot_account": false, "instance_id": 1
            },
            "community": {
                "id": 2, "name": "test", "title": "Test", "removed": false,
                "published": published, "deleted": false, "nsfw": false,
                "actor_id": "https://example.com/c/test", "local": true, "hidden": false,
                "posting_restricted_to_mods": false, "instance_id": 1, "visibility": "Public"
            },
            "counts": {
                "post_id": 17, "comments": 0, "score": 9, "upvotes": 10, "downvotes": 1,
                "published": published, "newest_comment_time": published
            },
            "creator_banned_from_community": false, "banned_from_community": false,
            "creator_is_moderator": false, "creator_is_admin": false, "creator_blocked": false,
            "subscribed": "NotSubscribed", "saved": false, "read": false, "hidden": false,
            "my_vote": 0, "unread_comments": 0
        }))
        .unwrap()
    }

    pub(super) fn comment_view(post: &PostView, text: &str) -> CommentView {
        let published = "2024-01-01T00:00:00Z";
        serde_json::from_value(json!({
            "comment": {
                "id": 42, "creator_id": 1, "post_id": post.post.id, "content": text,
                "removed": false, "published": published, "deleted": false,
                "ap_id": "https://example.com/comment/42", "local": true, "path": "0.42",
                "distinguished": false, "language_id": 0
            },
            "creator": post.creator, "post": post.post, "community": post.community,
            "counts": {
                "comment_id": 42, "score": 1, "upvotes": 1, "downvotes": 0,
                "published": published, "child_count": 0
            },
            "creator_banned_from_community": false, "banned_from_community": false,
            "creator_is_moderator": false, "creator_is_admin": false, "creator_blocked": false,
            "subscribed": "NotSubscribed", "saved": false, "my_vote": 1
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn opening_a_post_loads_its_thread_without_the_default_feed_scope() {
        let mut selected = post_view();
        selected.post.local = false;
        selected.community.local = false;
        let comment = comment_view(&selected, "Federated comment");
        let body = json!({"comments": [comment]}).to_string();
        let (api, request) = serve("200 OK", &body).await;
        let mut post = Post::default();
        let mut task = runtime::task::into_stream(
            post.open(api.authenticated("local-feed-account".into()), selected),
        )
        .unwrap();
        let Some(runtime::Action::Output(message @ Message::Loaded(..))) = task.next().await else {
            panic!("comment loading did not finish");
        };
        let _ = post.update(message);

        assert!(!post.busy);
        assert!(post.error.is_none());
        assert_eq!(post.comments.len(), 1);
        assert_eq!(post.comments[0].data.comment.content, "Federated comment");

        let request = request.await.unwrap();
        let target = request.split_whitespace().nth(1).unwrap();
        let (path, query) = target.split_once('?').unwrap();
        assert_eq!(path, "/api/v3/comment/list");
        assert!(query.split('&').any(|param| param == "type_=All"));
        assert!(query.split('&').any(|param| param == "post_id=17"));
    }

    #[tokio::test]
    async fn clicking_an_existing_comment_vote_clears_it_after_confirmation() {
        let selected = post_view();
        let existing = comment_view(&selected, "Vote on this comment");
        let mut updated = existing.clone();
        updated.my_vote = Some(0);
        updated.counts.score = 0;
        let body = json!({"comment_view": updated, "recipient_ids": []}).to_string();
        let (api, request) = serve("200 OK", &body).await;
        let mut post = Post::default();
        let _ = post.open(api.authenticated("test-account".into()), selected);
        let id = existing.comment.id;
        post.add_comment(existing);
        let mut task =
            runtime::task::into_stream(post.update(Message::Vote(post.generation, id, 1))).unwrap();
        assert_eq!(post.comments[0].data.my_vote, Some(1));
        let Some(runtime::Action::Output(message @ Message::VoteFinished(..))) = task.next().await
        else {
            panic!("vote did not finish");
        };
        let _ = post.update(message);
        assert_eq!(post.comments[0].data.my_vote, Some(0));
        assert_eq!(post.comments[0].data.counts.score, 0);
        let request = request.await.unwrap();
        assert!(request.starts_with("POST /api/v3/comment/like HTTP/1.1"));
        assert!(request.contains("authorization: Bearer test-account\r\n"));
        let (_, payload) = request.split_once("\r\n\r\n").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(payload).unwrap(),
            json!({"comment_id": 42, "score": 0})
        );
    }

    #[tokio::test]
    async fn reply_uses_the_selected_parent_and_preserves_draft_on_failure() {
        let previous = post_view();
        let parent = comment_view(&previous, "Parent comment");
        let parent_id = parent.comment.id;
        let mut sibling = parent.clone();
        sibling.comment.id = CommentId(43);
        sibling.comment.path = "0.43".into();
        let mut reply = comment_view(&previous, "My reply");
        reply.comment.id = CommentId(44);
        reply.comment.path = "0.42.44".into();
        reply.counts.comment_id = reply.comment.id;
        let body = json!({"comment_view": reply, "recipient_ids": []}).to_string();

        for success in [true, false] {
            let status = if success { "200 OK" } else { "403 Forbidden" };
            let (api, request) = serve(status, &body).await;
            let mut post = Post::default();
            let _ = post.open(api.authenticated("reply-account".into()), previous.clone());
            post.add_comment(parent.clone());
            post.add_comment(sibling.clone());
            post.composer.draft = text_editor::Content::with_text("Top-level draft");
            let generation = post.generation;
            let _ = post.update(Message::OpenReply(generation, parent_id));
            let target = CommentTarget::Reply(parent_id);
            let _ = post.update(Message::EditComment(
                generation,
                target,
                text_editor::Action::Edit(text_editor::Edit::Paste("My reply".to_owned().into())),
            ));
            let _ = post.update(Message::OpenReply(generation, sibling.comment.id));
            assert!(post.replies[&sibling.comment.id].draft.text().is_empty());
            assert!(
                runtime::task::into_stream(post.update(Message::SubmitComment(generation, target)))
                    .is_none()
            );
            let _ = post.update(Message::OpenReply(generation, parent_id));
            let mut task =
                runtime::task::into_stream(post.update(Message::SubmitComment(generation, target)))
                    .unwrap();
            assert_eq!(post.comments.len(), 2);
            let _ = post.update(Message::Collapse(parent_id.0));
            let Some(runtime::Action::Output(Message::CommentSubmitted(generation, result))) =
                task.next().await
            else {
                panic!("reply submission did not finish");
            };
            let updated = post.finish_comment(generation, result);
            assert_eq!(post.composer.draft.text(), "Top-level draft");
            if success {
                assert_eq!(updated, Some(previous.post.id));
                assert_eq!(post.comments[1].path, vec![42, 44]);
                assert!(!thread::hidden(&post.comments[1].path, &post.collapsed));
                assert!(!post.replies.contains_key(&parent_id));
                assert!(post.active_reply.is_none());
            } else {
                assert!(updated.is_none());
                assert_eq!(post.comments.len(), 2);
                assert_eq!(post.replies[&parent_id].draft.text(), "My reply");
                assert!(
                    post.replies[&parent_id]
                        .error
                        .as_deref()
                        .unwrap()
                        .contains("403")
                );
            }
            let request = request.await.unwrap();
            let (headers, payload) = request.split_once("\r\n\r\n").unwrap();
            assert!(
                headers
                    .lines()
                    .any(|line| line == "authorization: Bearer reply-account")
            );
            assert_eq!(
                serde_json::from_str::<Value>(payload).unwrap(),
                json!({
                    "post_id": 17, "parent_id": 42, "content": "My reply"
                })
            );
        }
    }

    #[tokio::test]
    async fn comment_submission_uses_selected_account_and_preserves_rejected_draft() {
        let text = "Hello\n\n**Lemmy**";
        let previous = post_view();
        let reply = json!({
            "comment_view": comment_view(&previous, text),
            "recipient_ids": []
        })
        .to_string();
        for success in [true, false] {
            let status = if success { "200 OK" } else { "403 Forbidden" };
            let (api, request) = serve(status, &reply).await;
            let mut post = Post::default();
            let _ = post.open(
                api.clone().authenticated("old-account".into()),
                previous.clone(),
            );
            let old_generation = post.generation;
            post.reset();
            let _ = post.open(
                api.authenticated("selected-account".into()),
                previous.clone(),
            );
            let generation = post.generation;
            post.composer.draft = text_editor::Content::with_text("   ");
            assert!(
                runtime::task::into_stream(
                    post.update(Message::SubmitComment(generation, CommentTarget::Post))
                )
                .is_none()
            );
            post.composer.draft = text_editor::Content::with_text(text);
            assert!(
                runtime::task::into_stream(
                    post.update(Message::SubmitComment(old_generation, CommentTarget::Post))
                )
                .is_none()
            );
            let mut task = runtime::task::into_stream(
                post.update(Message::SubmitComment(generation, CommentTarget::Post)),
            )
            .unwrap();
            assert!(post.comments.is_empty());
            assert!(
                runtime::task::into_stream(
                    post.update(Message::SubmitComment(generation, CommentTarget::Post))
                )
                .is_none()
            );
            let Some(runtime::Action::Output(Message::CommentSubmitted(generation, result))) =
                task.next().await
            else {
                panic!("comment submission did not finish");
            };
            assert!(
                post.finish_comment(old_generation, result.clone())
                    .is_none()
            );
            let updated = post.finish_comment(generation, result);
            assert!(post.submitting.is_none());
            if success {
                assert_eq!(updated, Some(previous.post.id));
                assert_eq!(post.comments.len(), 1);
                assert_eq!(post.comments[0].data.comment.content, text);
                assert_eq!(post.selected.as_ref().unwrap().counts.comments, 1);
                assert!(post.composer.draft.text().is_empty());
            } else {
                assert!(updated.is_none());
                assert!(post.comments.is_empty());
                assert_eq!(post.selected.as_ref(), Some(&previous));
                assert_eq!(post.composer.draft.text(), text);
                assert!(post.composer.error.as_deref().unwrap().contains("403"));
            }
            let request = request.await.unwrap();
            let (headers, payload) = request.split_once("\r\n\r\n").unwrap();
            assert_eq!(
                headers.lines().next(),
                Some("POST /api/v3/comment HTTP/1.1")
            );
            assert!(
                headers
                    .lines()
                    .any(|line| line == "authorization: Bearer selected-account")
            );
            assert_eq!(
                serde_json::from_str::<Value>(payload).unwrap(),
                json!({"post_id": 17, "content": text})
            );
        }
    }

    #[tokio::test]
    async fn actions_use_the_selected_account_and_only_apply_successful_responses() {
        for (action, route, body, (old_vote, old_saved), (vote, saved)) in [
            (
                Action::Upvote,
                "POST /api/v3/post/like",
                json!({"post_id": 17, "score": 1}),
                (0, false),
                (1, false),
            ),
            (
                Action::Downvote,
                "POST /api/v3/post/like",
                json!({"post_id": 17, "score": -1}),
                (1, false),
                (-1, false),
            ),
            (
                Action::ToggleSaved,
                "PUT /api/v3/post/save",
                json!({"post_id": 17, "save": true}),
                (-1, false),
                (-1, true),
            ),
            (
                Action::Upvote,
                "POST /api/v3/post/like",
                json!({"post_id": 17, "score": 0}),
                (1, true),
                (0, true),
            ),
            (
                Action::Downvote,
                "POST /api/v3/post/like",
                json!({"post_id": 17, "score": 0}),
                (-1, false),
                (0, false),
            ),
            (
                Action::ToggleSaved,
                "PUT /api/v3/post/save",
                json!({"post_id": 17, "save": false}),
                (1, true),
                (1, false),
            ),
        ] {
            for success in [true, false] {
                let mut previous = post_view();
                previous.my_vote = Some(old_vote);
                previous.saved = old_saved;
                let mut updated = previous.clone();
                updated.my_vote = Some(vote);
                updated.saved = saved;
                updated.counts.score += i64::from(vote - old_vote);
                let reply = json!({"post_view": updated}).to_string();
                let status = if success { "200 OK" } else { "403 Forbidden" };
                let (api, request) = serve(status, &reply).await;
                let mut post = Post::default();
                let _ = post.open(
                    api.clone().authenticated("first-account".into()),
                    previous.clone(),
                );
                let stale = Message::Act(post.generation, action);
                post.reset();
                let _ = post.open(
                    api.authenticated("selected-account".into()),
                    previous.clone(),
                );
                assert!(runtime::task::into_stream(post.update(stale)).is_none());
                let mut task =
                    runtime::task::into_stream(post.update(Message::Act(post.generation, action)))
                        .unwrap();
                assert_eq!(post.selected.as_ref(), Some(&previous));
                assert!(
                    runtime::task::into_stream(post.update(Message::Act(post.generation, action)))
                        .is_none()
                );
                let Some(runtime::Action::Output(Message::ActionFinished(generation, result))) =
                    task.next().await
                else {
                    panic!("post action did not finish");
                };
                let feed_update = post.finish_action(generation, result);
                assert!(post.pending_action.is_none());
                if success {
                    assert_eq!(feed_update.as_ref(), Some(&updated));
                    assert_eq!(post.selected.as_ref(), Some(&updated));
                    assert!(post.action_error.is_none());
                } else {
                    assert!(feed_update.is_none());
                    assert_eq!(post.selected.as_ref(), Some(&previous));
                    assert!(post.action_error.as_deref().unwrap().contains("403"));
                }

                let request = request.await.unwrap();
                let (headers, payload) = request.split_once("\r\n\r\n").unwrap();
                assert_eq!(headers.lines().next().unwrap(), format!("{route} HTTP/1.1"));
                assert!(
                    headers
                        .lines()
                        .any(|line| line == "authorization: Bearer selected-account")
                );
                assert_eq!(serde_json::from_str::<Value>(payload).unwrap(), body);
            }
        }
    }

    #[test]
    fn session_changes_discard_old_action_results_and_anonymous_actions() {
        let api = Api::new(reqwest::Client::new(), "example.com").unwrap();
        let previous = post_view();
        let mut post = Post::default();
        let _ = post.open(
            api.clone().authenticated("first-account".into()),
            previous.clone(),
        );
        let generation = post.generation;
        let _ = post.update(Message::Act(generation, Action::Upvote));
        assert!(post.pending_action.is_some());
        post.reset();
        let _ = post.open(api, previous.clone());
        let mut updated = previous.clone();
        updated.my_vote = Some(1);
        assert!(
            post.finish_action(generation, Ok(Box::new(updated)))
                .is_none()
        );
        assert_eq!(post.selected.as_ref(), Some(&previous));
        assert!(
            runtime::task::into_stream(
                post.update(Message::Act(post.generation, Action::ToggleSaved))
            )
            .is_none()
        );
    }

    #[test]
    fn closing_post_invalidates_pending_comment_response() {
        let mut post = Post::default();
        post.reset();
        let _ = post.update(Message::Loaded(0, Err("stale error".into())));
        assert!(post.error.is_none());
        assert!(!post.busy);
    }

    #[tokio::test]
    async fn switching_discussions_preserves_drafts_and_recovers_a_failed_submission() {
        let (api, request) = serve("403 Forbidden", "{}").await;
        let api = api.authenticated("test-account".into());
        let first = post_view();
        let mut second = first.clone();
        second.post.id = PostId(18);
        let mut post = Post::default();
        let _ = post.open(api.clone(), first.clone());
        let parent = comment_view(&first, "Parent");
        let parent_id = parent.comment.id;
        post.add_comment(parent);
        post.replies.insert(
            parent_id,
            Composer {
                draft: text_editor::Content::with_text("Unfinished reply"),
                error: None,
            },
        );
        post.composer_open = true;
        post.composer.draft = text_editor::Content::with_text("Unfinished comment");

        let mut task =
            runtime::task::into_stream(post.submit_comment(post.generation, CommentTarget::Post))
                .unwrap();
        let _ = post.open(api.clone(), second);
        assert!(post.composer.draft.text().is_empty());
        let Some(runtime::Action::Output(Message::CommentSubmitted(generation, result))) =
            task.next().await
        else {
            panic!("comment submission did not finish");
        };
        assert!(post.finish_comment(generation, result).is_none());
        let _ = post.open(api.clone(), first.clone());
        assert!(post.composer_open);
        assert_eq!(post.composer.draft.text(), "Unfinished comment");
        assert!(post.composer.error.as_deref().unwrap().contains("403"));
        assert_eq!(post.replies[&parent_id].draft.text(), "Unfinished reply");
        post.reset();
        let _ = post.open(api, first);
        assert!(post.composer.draft.text().is_empty());
        assert!(post.replies.is_empty());
        request.await.unwrap();
    }
}
