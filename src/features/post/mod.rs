//! Post reading, authenticated actions, and paginated comment threads.
mod service;
mod thread;
mod view;

use crate::shared::{api::Api, links::open_web_link};
use cosmic::{
    iced::Task,
    widget::{markdown, text_editor},
};
use lemmy_api_common::{
    comment::GetCommentsResponse,
    lemmy_db_schema::newtypes::PostId,
    lemmy_db_views::structs::{CommentView, PostView},
};
use std::collections::HashSet;

pub struct Comment {
    pub data: CommentView,
    pub body: markdown::Content,
    pub path: Vec<i32>,
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
    draft: text_editor::Content<cosmic::Renderer>,
    submitting: bool,
    submit_error: Option<String>,
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
    Retry,
    More,
    Collapse(i32),
    Link(String),
    Loaded(u64, Result<GetCommentsResponse, String>),
    Act(u64, Action),
    ActionFinished(u64, Result<Box<PostView>, String>),
    EditComment(u64, text_editor::Action),
    SubmitComment(u64),
    CommentSubmitted(u64, Result<Box<CommentView>, String>),
}

impl Post {
    pub fn open(&mut self, api: Api, post: PostView) -> Task<Message> {
        self.close();
        self.api = Some(api);
        self.set_selected(post);
        self.page = 1;
        self.load()
    }

    pub fn reset(&mut self) {
        self.close();
        self.pending_action = None;
    }

    pub fn close(&mut self) {
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
        self.draft = text_editor::Content::new();
        self.submitting = false;
        self.submit_error = None;
    }

    fn set_selected(&mut self, post: PostView) {
        self.body = markdown::Content::parse(post.post.body.as_deref().unwrap_or_default());
        self.selected = Some(post);
    }

    fn act(&mut self, generation: u64, action: Action) -> Task<Message> {
        if generation != self.generation || self.pending_action.is_some() || self.submitting {
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

    fn submit_comment(&mut self, generation: u64) -> Task<Message> {
        if generation != self.generation || self.submitting || self.pending_action.is_some() {
            return Task::none();
        }
        let (Some(api), Some(post)) = (&self.api, &self.selected) else {
            return Task::none();
        };
        let content = self.draft.text();
        if !api.is_authenticated() || content.trim().is_empty() {
            return Task::none();
        }
        self.submitting = true;
        self.submit_error = None;
        Task::perform(
            service::create_comment(api.clone(), post.post.id, content),
            move |result| Message::CommentSubmitted(generation, result.map(Box::new)),
        )
    }

    pub fn finish_comment(
        &mut self,
        generation: u64,
        result: Result<Box<CommentView>, String>,
    ) -> Option<PostId> {
        if generation != self.generation || !self.submitting {
            return None;
        }
        self.submitting = false;
        match result {
            Ok(comment) => {
                self.draft = text_editor::Content::new();
                self.add_comment(*comment);
                self.comments.sort_by(|a, b| a.path.cmp(&b.path));
                let post = self.selected.as_mut()?;
                post.counts.comments += 1;
                Some(post.post.id)
            }
            Err(error) => {
                self.submit_error = Some(error);
                None
            }
        }
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
            service::comments(api, post.post.id.0, self.page),
            move |result| Message::Loaded(generation, result),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Act(generation, action) => return self.act(generation, action),
            Message::EditComment(generation, action)
                if generation == self.generation && !self.submitting =>
            {
                self.draft.perform(action);
            }
            Message::SubmitComment(generation) => return self.submit_comment(generation),
            Message::More if !self.busy && self.more => return self.load(),
            Message::Retry if !self.busy => return self.load(),
            Message::Collapse(id) => {
                if !self.collapsed.remove(&id) {
                    self.collapsed.insert(id);
                }
            }
            Message::Link(url) => {
                if let Err(error) = open_web_link(&url) {
                    self.error = Some(error);
                }
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
                        self.comments.sort_by(|a, b| a.path.cmp(&b.path));
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
            post.draft = text_editor::Content::with_text("   ");
            assert!(
                runtime::task::into_stream(post.update(Message::SubmitComment(generation)))
                    .is_none()
            );
            post.draft = text_editor::Content::with_text(text);
            assert!(
                runtime::task::into_stream(post.update(Message::SubmitComment(old_generation)))
                    .is_none()
            );
            let mut task =
                runtime::task::into_stream(post.update(Message::SubmitComment(generation)))
                    .unwrap();
            assert!(post.comments.is_empty());
            assert!(
                runtime::task::into_stream(post.update(Message::SubmitComment(generation)))
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
            assert!(!post.submitting);
            if success {
                assert_eq!(updated, Some(previous.post.id));
                assert_eq!(post.comments.len(), 1);
                assert_eq!(post.comments[0].data.comment.content, text);
                assert_eq!(post.selected.as_ref().unwrap().counts.comments, 1);
                assert!(post.draft.text().is_empty());
            } else {
                assert!(updated.is_none());
                assert!(post.comments.is_empty());
                assert_eq!(post.selected.as_ref(), Some(&previous));
                assert_eq!(post.draft.text(), text);
                assert!(post.submit_error.as_deref().unwrap().contains("403"));
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
}
