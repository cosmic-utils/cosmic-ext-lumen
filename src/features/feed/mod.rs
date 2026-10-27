//! Public feed: sorting, pagination, selection, and stale-response protection.
mod service;
mod view;

use crate::shared::api::Api;
use cosmic::iced::Task;
use lemmy_api_common::{
    lemmy_db_schema::SortType,
    lemmy_db_views::structs::{PaginationCursor, PostView},
    post::GetPostsResponse,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Hot,
    New,
    Active,
    TopDay,
}
impl Sort {
    pub const LABELS: [&str; 4] = ["Hot", "New", "Active", "Top today"];
    pub fn index(self) -> usize {
        match self {
            Self::Hot => 0,
            Self::New => 1,
            Self::Active => 2,
            Self::TopDay => 3,
        }
    }
    fn from_index(index: usize) -> Self {
        match index {
            1 => Self::New,
            2 => Self::Active,
            3 => Self::TopDay,
            _ => Self::Hot,
        }
    }
    fn api(self) -> SortType {
        match self {
            Self::Hot => SortType::Hot,
            Self::New => SortType::New,
            Self::Active => SortType::Active,
            Self::TopDay => SortType::TopDay,
        }
    }
}

#[derive(Default)]
pub struct Feed {
    pub posts: Vec<PostView>,
    pub sort: Sort,
    pub local: bool,
    pub subscribed: bool,
    pub busy: bool,
    pub loaded: bool,
    pub error: Option<String>,
    cursor: Option<PaginationCursor>,
    generation: u64,
    api: Option<Api>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Refresh,
    Subscribed,
    More,
    Sort(usize),
    Scope(bool),
    Open(i32),
    Loaded(u64, bool, Result<GetPostsResponse, String>),
}

impl Feed {
    pub fn connect(&mut self, api: Api) -> Task<Message> {
        self.api = Some(api);
        self.reload()
    }

    fn reload(&mut self) -> Task<Message> {
        self.generation += 1;
        self.posts.clear();
        self.cursor = None;
        self.loaded = false;
        self.request(false)
    }

    fn request(&mut self, append: bool) -> Task<Message> {
        let Some(api) = self.api.clone() else {
            return Task::none();
        };
        self.busy = true;
        self.error = None;
        let generation = self.generation;
        let (sort, local) = (self.sort, self.local);
        let cursor = if append { self.cursor.clone() } else { None };
        Task::perform(
            service::load(api, sort, local, self.subscribed, cursor),
            move |result| Message::Loaded(generation, append, result),
        )
    }

    pub fn update(&mut self, message: Message) -> (Task<Message>, Option<PostView>) {
        match message {
            Message::Subscribed => {
                self.subscribed = true;
                return (self.reload(), None);
            }
            Message::Refresh => return (self.reload(), None),
            Message::More if !self.busy && self.cursor.is_some() => {
                return (self.request(true), None);
            }
            Message::Sort(index) => {
                self.sort = Sort::from_index(index);
                return (self.reload(), None);
            }
            Message::Scope(local) => {
                self.local = local;
                self.subscribed = false;
                return (self.reload(), None);
            }
            Message::Open(id) => {
                return (
                    Task::none(),
                    self.posts.iter().find(|p| p.post.id.0 == id).cloned(),
                );
            }
            Message::Loaded(generation, append, result) if generation == self.generation => {
                self.busy = false;
                match result {
                    Ok(response) => {
                        self.loaded = true;
                        self.cursor = response.next_page;
                        if !append {
                            self.posts.clear();
                        }
                        for post in response.posts {
                            if !self.posts.iter().any(|p| p.post.id == post.post.id) {
                                self.posts.push(post);
                            }
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            _ => {}
        }
        (Task::none(), None)
    }

    pub fn api(&self) -> Option<Api> {
        self.api.clone()
    }

    pub fn apply_post_update(&mut self, updated: PostView) -> Task<Message> {
        if let Some(post) = self.posts.iter_mut().find(|p| p.post.id == updated.post.id) {
            *post = updated;
        } else if self.busy && !self.loaded {
            // A refresh started before the write finished. Fetch its first page again.
            return self.reload();
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_response_from_previous_feed() {
        let mut feed = Feed {
            generation: 2,
            busy: true,
            ..Default::default()
        };
        let _ = feed.update(Message::Loaded(1, false, Err("old error".into())));
        assert!(feed.busy);
        assert!(feed.error.is_none());
    }
}
