//! Post reading, authenticated actions, and paginated comment threads.
mod service;
mod thread;
mod view;

use crate::shared::{api::Api, links::open_web_link};
use cosmic::{iced::Task, widget::markdown};
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
    }

    fn set_selected(&mut self, post: PostView) {
        self.body = markdown::Content::parse(post.post.body.as_deref().unwrap_or_default());
        self.selected = Some(post);
    }

    fn act(&mut self, generation: u64, action: Action) -> Task<Message> {
        if generation != self.generation || self.pending_action.is_some() {
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
                            if self
                                .comments
                                .iter()
                                .any(|c| c.data.comment.id == data.comment.id)
                            {
                                continue;
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
mod tests {
    use super::*;
    use crate::shared::api::tests::serve;
    use cosmic::iced::{futures::StreamExt, runtime};
    use serde_json::{Value, json};

    fn post_view() -> PostView {
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
