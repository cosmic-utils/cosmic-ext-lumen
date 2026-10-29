//! Post feeds, sorting, pagination, and selection.
mod service;
mod view;

use crate::shared::api::Api;
use cosmic::iced::Task;
use lemmy_api_common::{
    lemmy_db_schema::{SortType, newtypes::PostId},
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Local,
    Subscribed,
    Saved,
}

#[derive(Default)]
pub struct Feed {
    pub posts: Vec<PostView>,
    pub sort: Sort,
    scope: Scope,
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
    More,
    Sort(usize),
    Scope(Scope),
    Open(i32),
    Loaded(u64, bool, Result<GetPostsResponse, String>),
}

impl Feed {
    pub fn connect(&mut self, api: Api) -> Task<Message> {
        if self.scope == Scope::Saved && !api.is_authenticated() {
            self.scope = Scope::All;
        }
        self.api = Some(api);
        self.reload()
    }

    fn can_view_saved(&self) -> bool {
        self.api.as_ref().is_some_and(Api::is_authenticated)
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
        let cursor = if append { self.cursor.clone() } else { None };
        Task::perform(
            service::load(api, self.sort, self.scope, cursor),
            move |result| Message::Loaded(generation, append, result),
        )
    }

    pub fn update(&mut self, message: Message) -> (Task<Message>, Option<PostView>) {
        match message {
            Message::Refresh => return (self.reload(), None),
            Message::More if !self.busy && self.cursor.is_some() => {
                return (self.request(true), None);
            }
            Message::Sort(index) => {
                self.sort = Sort::from_index(index);
                return (self.reload(), None);
            }
            Message::Scope(scope) if scope != Scope::Saved || self.can_view_saved() => {
                self.scope = scope;
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

    pub fn comment_added(&mut self, id: PostId) {
        if let Some(post) = self.posts.iter_mut().find(|post| post.post.id == id) {
            post.counts.comments += 1;
        }
    }

    pub fn apply_post_update(&mut self, updated: PostView) -> Task<Message> {
        if self.scope == Scope::Saved && !updated.saved {
            if self.busy {
                // The in-flight page may still include the bookmark.
                return self.reload();
            }
            self.posts.retain(|post| post.post.id != updated.post.id);
        } else if let Some(post) = self.posts.iter_mut().find(|p| p.post.id == updated.post.id) {
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
    use crate::{features::post::tests::post_view, shared::api::tests::serve};
    use cosmic::iced::{futures::StreamExt, runtime};
    use serde_json::json;

    async fn loaded(task: Task<Message>) -> Message {
        let mut stream = runtime::task::into_stream(task).unwrap();
        let Some(runtime::Action::Output(message)) = stream.next().await else {
            panic!("feed request did not finish");
        };
        message
    }

    #[tokio::test]
    async fn saved_feed_uses_the_selected_account_and_discards_old_pages() {
        let mut first_post = post_view();
        first_post.saved = true;
        let mut second_post = first_post.clone();
        second_post.post.id.0 += 1;
        second_post.counts.post_id = second_post.post.id;
        let first_reply = json!({"posts": [first_post], "next_page": "first-page"}).to_string();
        let second_reply = json!({"posts": [second_post]}).to_string();
        let (first_api, first_request) = serve("200 OK", &first_reply).await;
        let (second_api, second_request) = serve("200 OK", &second_reply).await;
        let mut feed = Feed::default();
        let _ = feed.connect(first_api.authenticated("first-account".into()));
        let (first_task, _) = feed.update(Message::Scope(Scope::Saved));
        let first_response = loaded(first_task).await;
        let _ = feed.update(first_response.clone());
        assert_eq!(feed.posts, vec![first_post]);
        assert!(feed.cursor.is_some());

        let second_task = feed.connect(second_api.authenticated("second-account".into()));
        assert!(feed.posts.is_empty());
        assert!(feed.cursor.is_none());
        let _ = feed.update(first_response);
        assert!(feed.posts.is_empty());
        let _ = feed.update(loaded(second_task).await);
        assert_eq!(feed.posts, vec![second_post.clone()]);

        for (request, token) in [
            (first_request, "first-account"),
            (second_request, "second-account"),
        ] {
            let request = request.await.unwrap();
            let line = request.lines().next().unwrap();
            assert!(line.starts_with("GET /api/v3/post/list?"));
            assert!(line.contains("saved_only=true"));
            assert!(request.contains(&format!("authorization: Bearer {token}\r\n")));
        }

        second_post.saved = false;
        let _ = feed.apply_post_update(second_post);
        assert!(feed.posts.is_empty());

        let anonymous = Api::new(reqwest::Client::new(), "example.com").unwrap();
        let _ = feed.connect(anonymous);
        assert_eq!(feed.scope, Scope::All);
        let (task, _) = feed.update(Message::Scope(Scope::Saved));
        assert!(runtime::task::into_stream(task).is_none());
        assert_eq!(feed.scope, Scope::All);
    }

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
