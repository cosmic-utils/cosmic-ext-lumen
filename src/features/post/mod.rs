//! Post reading, Markdown, paginated comments, and collapsible threads.
mod service;
mod thread;
mod view;

use crate::shared::{api::Api, links::open_web_link};
use cosmic::{iced::Task, widget::markdown};
use lemmy_api_common::{
    comment::GetCommentsResponse,
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
}

#[derive(Clone, Debug)]
pub enum Message {
    Back,
    Retry,
    More,
    Collapse(i32),
    Link(String),
    Loaded(u64, Result<GetCommentsResponse, String>),
}

impl Post {
    pub fn open(&mut self, api: Api, post: PostView) -> Task<Message> {
        self.reset();
        self.api = Some(api);
        self.body = markdown::Content::parse(post.post.body.as_deref().unwrap_or_default());
        self.selected = Some(post);
        self.page = 1;
        self.load()
    }

    pub fn reset(&mut self) {
        self.generation += 1;
        self.selected = None;
        self.comments.clear();
        self.collapsed.clear();
        self.body = markdown::Content::new();
        self.busy = false;
        self.more = false;
        self.error = None;
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
    #[test]
    fn closing_post_invalidates_pending_comment_response() {
        let mut post = Post::default();
        post.reset();
        let _ = post.update(Message::Loaded(0, Err("stale error".into())));
        assert!(post.error.is_none());
        assert!(!post.busy);
    }
}
