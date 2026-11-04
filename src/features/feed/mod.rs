//! Post feeds, sorting, pagination, and selection.
mod service;
mod view;
pub use view::SCROLL_ID;

use std::sync::Arc;

use crate::{features::post, shared::api::Api};
use cosmic::{iced::Task, widget::markdown};
use lemmy_api_common::{
    lemmy_db_schema::{
        ListingType, SearchType, SortType,
        newtypes::{CommentId, CommunityId, PersonId, PostId},
        source::{community::Community, person::Person},
    },
    lemmy_db_views::structs::{CommentView, PaginationCursor, PostView},
    lemmy_db_views_actor::structs::{CommunityView, PersonView},
    person::GetPersonDetailsResponse,
    post::GetPostsResponse,
    site::{ResolveObjectResponse, Search, SearchResponse},
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
    Community(CommunityId),
    Profile(PersonId),
    Search,
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
    query: String,
    resolve_reference: bool,
    search_type: SearchKind,
    search_community: Option<CommunityId>,
    search_communities: Vec<Community>,
    communities: Vec<CommunityView>,
    people: Vec<PersonView>,
    profile: Option<Person>,
    profile_bio: markdown::Content,
    profile_comments: bool,
    page: i64,
    page_more: bool,
    show_saved_comments: bool,
    comments: Vec<CommentView>,
    opening_comment: Option<CommentId>,
    open_error: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SearchKind {
    #[default]
    Posts,
    Comments,
    Communities,
    People,
}

impl SearchKind {
    const OPTIONS: [Self; 4] = [Self::Posts, Self::Comments, Self::Communities, Self::People];

    fn index(self) -> usize {
        self as usize
    }

    fn supports_community(self) -> bool {
        matches!(self, Self::Posts | Self::Comments)
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Refresh,
    More,
    Sort(usize),
    Scope(Scope),
    Search(String, Option<CommunityId>),
    SearchKind(usize),
    SearchKeywords,
    ReferenceResolved(u64, Result<Arc<ResolveObjectResponse>, String>),
    SearchCommunity(usize),
    OpenCommunity(CommunityId),
    CommunityDetails(CommunityId),
    SavedTab(usize),
    ProfileTab(usize),
    OpenPerson(PersonId),
    OpenLink(String),
    ProfileLoaded(u64, bool, Result<Box<GetPersonDetailsResponse>, String>),
    Open(i32),
    OpenComment(CommentId),
    CommentOpened(u64, CommentId, Result<Box<PostView>, String>),
    SavedCommentsLoaded(u64, Result<Vec<CommentView>, String>),
    Scrolled(f32),
    Loaded(u64, bool, Result<GetPostsResponse, String>),
    SearchLoaded(u64, bool, Result<SearchResponse, String>),
}

impl Feed {
    pub fn connect(&mut self, api: Api) -> Task<Message> {
        if matches!(
            self.scope,
            Scope::Community(_) | Scope::Search | Scope::Profile(_)
        ) || (matches!(self.scope, Scope::Saved | Scope::Subscribed) && !api.is_authenticated())
        {
            self.scope = Scope::All;
        }
        self.api = Some(api);
        self.show_saved_comments = false;
        self.profile = None;
        self.profile_comments = false;
        self.search_type = SearchKind::Posts;
        self.search_community = None;
        self.search_communities.clear();
        self.reload()
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn signed_in(&self) -> bool {
        self.api.as_ref().is_some_and(Api::is_authenticated)
    }

    fn reload(&mut self) -> Task<Message> {
        self.generation += 1;
        self.posts.clear();
        self.comments.clear();
        self.communities.clear();
        self.people.clear();
        self.opening_comment = None;
        self.open_error = None;
        self.cursor = None;
        self.page = 1;
        self.page_more = false;
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
        if self.viewing_saved_comments() {
            return Task::perform(service::saved_comments(api, self.page), move |result| {
                Message::SavedCommentsLoaded(generation, result)
            });
        }
        if let Scope::Profile(id) = self.scope {
            return Task::perform(
                service::profile(api, id, self.sort, self.page),
                move |result| Message::ProfileLoaded(generation, append, result.map(Box::new)),
            );
        }
        if self.viewing_reference() {
            return Task::perform(service::resolve(api, self.query.clone()), move |result| {
                Message::ReferenceResolved(generation, result.map(Arc::new))
            });
        }
        if self.scope == Scope::Search {
            return Task::perform(
                service::search(
                    api,
                    Search {
                        q: self.query.clone(),
                        type_: Some(match self.search_type {
                            SearchKind::Posts => SearchType::Posts,
                            SearchKind::Comments => SearchType::Comments,
                            SearchKind::Communities => SearchType::Communities,
                            SearchKind::People => SearchType::Users,
                        }),
                        community_id: self
                            .search_community
                            .filter(|_| self.search_type.supports_community()),
                        listing_type: Some(ListingType::All),
                        sort: Some(self.sort.api()),
                        page: Some(self.page),
                        limit: Some(service::PAGE_SIZE),
                        ..Default::default()
                    },
                ),
                move |result| Message::SearchLoaded(generation, append, result),
            );
        }
        let cursor = if append { self.cursor.clone() } else { None };
        Task::perform(
            service::load(api, self.sort, self.scope, cursor),
            move |result| Message::Loaded(generation, append, result),
        )
    }

    pub fn update(
        &mut self,
        message: Message,
    ) -> (Task<Message>, Option<(PostView, Option<CommentId>)>) {
        match message {
            Message::Refresh => return (self.reload(), None),
            Message::More if !self.busy && self.has_more() => {
                return (self.request(true), None);
            }
            Message::Sort(index) => {
                self.sort = Sort::from_index(index);
                return (self.reload(), None);
            }
            Message::Scope(scope)
                if scope != Scope::Search
                    && (!matches!(scope, Scope::Saved | Scope::Subscribed) || self.signed_in()) =>
            {
                self.scope = scope;
                self.profile = None;
                self.profile_comments = false;
                return (self.reload(), None);
            }
            Message::Search(query, community) if !query.is_empty() && self.api.is_some() => {
                if self.scope != Scope::Search {
                    self.search_type = SearchKind::Posts;
                    self.search_community = community;
                }
                self.profile = None;
                self.resolve_reference = service::is_federated_reference(&query);
                self.query = query;
                self.scope = Scope::Search;
                return (self.reload(), None);
            }
            Message::SearchKeywords if self.viewing_reference() => {
                self.resolve_reference = false;
                return (self.reload(), None);
            }
            Message::SearchKind(index) if self.scope == Scope::Search => {
                if let Some(kind) = SearchKind::OPTIONS
                    .get(index)
                    .copied()
                    .filter(|kind| *kind != self.search_type)
                {
                    self.search_type = kind;
                    return (self.reload(), None);
                }
            }
            Message::SearchCommunity(index) if self.scope == Scope::Search => {
                let community = index
                    .checked_sub(1)
                    .and_then(|index| self.search_communities.get(index))
                    .map(|community| community.id);
                if community != self.search_community {
                    self.search_community = community;
                    return (self.reload(), None);
                }
            }
            Message::ProfileTab(index) if matches!(self.scope, Scope::Profile(_)) && index < 2 => {
                let comments = index == 1;
                if self.profile_comments != comments {
                    self.profile_comments = comments;
                    return (self.reload(), None);
                }
            }
            Message::ProfileLoaded(generation, append, result) if generation == self.generation => {
                self.busy = false;
                match result {
                    Ok(response) => {
                        let GetPersonDetailsResponse {
                            person_view,
                            posts,
                            comments,
                            ..
                        } = *response;
                        self.profile_bio = markdown::Content::parse(
                            person_view.person.bio.as_deref().unwrap_or_default(),
                        );
                        self.profile = Some(person_view.person);
                        let count = if self.profile_comments {
                            let count = comments.len();
                            self.apply_comments(
                                comments
                                    .into_iter()
                                    .filter(|item| !item.post.nsfw && !item.community.nsfw),
                            );
                            count
                        } else {
                            let count = posts.len();
                            self.apply_posts(
                                posts
                                    .into_iter()
                                    .filter(|item| !item.post.nsfw && !item.community.nsfw),
                                append,
                            );
                            count
                        };
                        self.loaded = true;
                        self.page_more = count == service::PAGE_SIZE as usize;
                        self.page += 1;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::SavedTab(index) if self.scope == Scope::Saved && index < 2 => {
                let show_comments = index == 1;
                if self.show_saved_comments != show_comments {
                    self.show_saved_comments = show_comments;
                    return (self.reload(), None);
                }
            }
            Message::OpenComment(id) if self.viewing_comments() => {
                let (Some(api), Some(comment)) = (
                    self.api.clone(),
                    self.comments.iter().find(|item| item.comment.id == id),
                ) else {
                    return (Task::none(), None);
                };
                self.opening_comment = Some(id);
                self.open_error = None;
                let generation = self.generation;
                let post_id = comment.post.id;
                return (
                    Task::perform(
                        async move { post::get_post(&api, post_id).await.map(Box::new) },
                        move |result| Message::CommentOpened(generation, id, result),
                    ),
                    None,
                );
            }
            Message::CommentOpened(generation, id, result)
                if generation == self.generation && self.opening_comment == Some(id) =>
            {
                self.opening_comment = None;
                match result {
                    Ok(post) => return (Task::none(), Some((*post, Some(id)))),
                    Err(error) => self.open_error = Some(error),
                }
            }
            Message::SavedCommentsLoaded(generation, result) if generation == self.generation => {
                self.busy = false;
                match result {
                    Ok(comments) => {
                        self.loaded = true;
                        self.page_more = comments.len() == service::PAGE_SIZE as usize;
                        self.page += 1;
                        self.apply_comments(comments.into_iter().filter(|comment| comment.saved));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Open(id) => {
                return (
                    Task::none(),
                    self.posts
                        .iter()
                        .find(|p| p.post.id.0 == id)
                        .cloned()
                        .map(|post| (post, None)),
                );
            }
            Message::Loaded(generation, append, result) if generation == self.generation => {
                self.busy = false;
                match result {
                    Ok(response) => {
                        self.cursor = response.next_page;
                        self.apply_posts(response.posts, append);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::SearchLoaded(generation, append, result) if generation == self.generation => {
                self.busy = false;
                match result {
                    Ok(response) => {
                        self.loaded = true;
                        let count = match self.search_type {
                            SearchKind::Posts => {
                                let count = response.posts.len();
                                self.apply_posts(
                                    response
                                        .posts
                                        .into_iter()
                                        .filter(|post| !post.post.nsfw && !post.community.nsfw),
                                    append,
                                );
                                count
                            }
                            SearchKind::Comments => {
                                let count = response.comments.len();
                                self.apply_comments(response.comments.into_iter().filter(
                                    |comment| !comment.post.nsfw && !comment.community.nsfw,
                                ));
                                count
                            }
                            SearchKind::People => {
                                let count = response.users.len();
                                for view in response.users {
                                    if !view.person.deleted
                                        && !self
                                            .people
                                            .iter()
                                            .any(|item| item.person.id == view.person.id)
                                    {
                                        self.people.push(view);
                                    }
                                }
                                count
                            }
                            SearchKind::Communities => {
                                let count = response.communities.len();
                                for view in response.communities {
                                    if !view.community.nsfw
                                        && !self
                                            .communities
                                            .iter()
                                            .any(|item| item.community.id == view.community.id)
                                    {
                                        self.communities.push(view);
                                    }
                                }
                                count
                            }
                        };
                        self.page_more = count == service::PAGE_SIZE as usize;
                        self.page += 1;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            _ => {}
        }
        (Task::none(), None)
    }

    fn viewing_reference(&self) -> bool {
        self.scope == Scope::Search && self.resolve_reference
    }

    pub fn finish_resolution(
        &mut self,
        generation: u64,
        result: Result<Arc<ResolveObjectResponse>, String>,
    ) -> Option<Message> {
        if generation != self.generation || !self.viewing_reference() {
            return None;
        }
        self.busy = false;
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                self.error = Some(format!("Could not open this link or handle. {error}"));
                return None;
            }
        };
        let message = if let Some(comment) = &response.comment {
            let id = comment.comment.id;
            self.comments.push(comment.clone());
            Message::OpenComment(id)
        } else if let Some(post) = &response.post {
            let id = post.post.id.0;
            self.posts.push(post.clone());
            Message::Open(id)
        } else if let Some(community) = &response.community {
            let id = community.community.id;
            self.communities.push(community.clone());
            Message::OpenCommunity(id)
        } else if let Some(person) = &response.person {
            Message::OpenPerson(person.person.id)
        } else {
            self.error = Some("The instance found no match for this link or handle.".into());
            return None;
        };
        self.loaded = true;
        Some(message)
    }

    pub fn set_search_communities(
        &mut self,
        communities: &[Community],
        current: Option<&Community>,
    ) {
        self.search_communities = communities.to_vec();
        if let Some(current) = current
            && !self
                .search_communities
                .iter()
                .any(|community| community.id == current.id)
        {
            self.search_communities.push(current.clone());
        }
        self.search_communities
            .sort_by_key(|community| community.name.to_lowercase());
    }

    pub fn found_community(&self, id: CommunityId) -> Option<&Community> {
        self.communities
            .iter()
            .map(|view| &view.community)
            .find(|community| community.id == id)
    }

    pub fn profile(&self) -> Option<&Person> {
        self.profile.as_ref()
    }

    fn viewing_comments(&self) -> bool {
        if self.viewing_reference() {
            return !self.comments.is_empty();
        }
        self.viewing_saved_comments()
            || (self.scope == Scope::Search && self.search_type == SearchKind::Comments)
            || (matches!(self.scope, Scope::Profile(_)) && self.profile_comments)
    }

    fn apply_comments(&mut self, comments: impl IntoIterator<Item = CommentView>) {
        for comment in comments {
            if !self
                .comments
                .iter()
                .any(|item| item.comment.id == comment.comment.id)
            {
                self.comments.push(comment);
            }
        }
    }

    fn viewing_saved_comments(&self) -> bool {
        self.scope == Scope::Saved && self.show_saved_comments
    }

    pub fn saved_comments_changed(&mut self) -> Task<Message> {
        if self.viewing_saved_comments() {
            return self.reload();
        }
        Task::none()
    }

    fn has_more(&self) -> bool {
        self.cursor.is_some() || self.page_more
    }

    fn apply_posts(&mut self, posts: impl IntoIterator<Item = PostView>, append: bool) {
        self.loaded = true;
        if !append {
            self.posts.clear();
        }
        for post in posts {
            if !self.posts.iter().any(|p| p.post.id == post.post.id) {
                self.posts.push(post);
            }
        }
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
    use crate::{
        features::post::tests::{comment_view, post_view},
        shared::api::tests::serve,
    };
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

    #[tokio::test]
    async fn saved_comments_are_account_scoped_and_open_their_discussion() {
        let post = post_view();
        let mut comment = comment_view(&post, "Saved reply");
        comment.saved = true;
        let response = json!({"comments": [comment]}).to_string();
        let (api, request) = serve("200 OK", &response).await;
        let mut feed = Feed::default();
        let _ = feed.connect(api.authenticated("reader".into()));
        let _ = feed.update(Message::Scope(Scope::Saved));
        let (task, _) = feed.update(Message::SavedTab(1));
        let loaded = loaded(task).await;
        let _ = feed.update(loaded.clone());
        assert_eq!(feed.comments, vec![comment.clone()]);

        let request = request.await.unwrap();
        assert!(request.starts_with("GET /api/v3/comment/list?"));
        assert!(request.contains("saved_only=true"));
        assert!(request.contains("authorization: Bearer reader\r\n"));

        let id = comment.comment.id;
        let _ = feed.update(Message::OpenComment(id));
        let opened = Message::CommentOpened(feed.generation, id, Ok(Box::new(post.clone())));
        assert_eq!(feed.update(opened.clone()).1, Some((post, Some(id))));
        let _ = feed.connect(Api::new(reqwest::Client::new(), "example.com").unwrap());
        let _ = feed.update(loaded);
        assert!(feed.comments.is_empty());
        assert!(feed.update(opened).1.is_none());
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

    #[tokio::test]
    async fn searches_all_communities_and_keeps_results_openable() {
        let post = post_view();
        let body = json!({
            "type_": "Posts",
            "posts": [post],
            "comments": [],
            "communities": [],
            "users": []
        })
        .to_string();
        let (api, request) = serve("200 OK", &body).await;
        let mut feed = Feed::default();
        let _ = feed.connect(api);
        let (task, _) = feed.update(Message::Search("split reading".into(), None));
        let _ = feed.update(loaded(task).await);
        assert_eq!(feed.scope(), Scope::Search);
        assert_eq!(
            feed.update(Message::Open(post.post.id.0)).1,
            Some((post, None))
        );
        let request = request.await.unwrap();
        let line = request.lines().next().unwrap();
        assert!(line.starts_with("GET /api/v3/search?"));
        for param in [
            "q=split+reading",
            "type_=Posts",
            "listing_type=All",
            "page=1",
        ] {
            assert!(line.contains(param), "{line}");
        }
    }
    #[tokio::test]
    async fn comment_search_uses_the_selected_community_and_ignores_stale_results() {
        let post = post_view();
        let mut comment = comment_view(&post, "A matching comment");
        comment.saved = false;
        let body = json!({
            "type_": "Comments",
            "comments": [comment],
            "posts": [],
            "communities": [],
            "users": []
        })
        .to_string();
        let (api, request) = serve("200 OK", &body).await;
        let mut feed = Feed::default();
        let _ = feed.connect(api);
        feed.set_search_communities(&[], Some(&post.community));
        let _ = feed.update(Message::Search("matching".into(), Some(post.community.id)));
        let (task, _) = feed.update(Message::SearchKind(SearchKind::Comments.index()));
        let response = loaded(task).await;
        let _ = feed.update(response.clone());
        assert_eq!(feed.comments, vec![comment]);

        let request = request.await.unwrap();
        let line = request.lines().next().unwrap();
        assert!(line.starts_with("GET /api/v3/search?"));
        for param in ["q=matching", "type_=Comments", "community_id=2", "page=1"] {
            assert!(line.contains(param), "{line}");
        }

        let _ = feed.update(Message::SearchCommunity(0));
        let _ = feed.update(response);
        assert!(feed.search_community.is_none());
        assert!(feed.comments.is_empty());
        assert!(feed.busy);
    }
    #[tokio::test]
    async fn people_search_opens_profile_post_and_comment_history() {
        let post = post_view();
        let comment = comment_view(&post, "Author's comment");
        let person = json!({
            "person": post.creator,
            "counts": {
                "person_id": post.creator.id,
                "post_count": 1,
                "comment_count": 1
            },
            "is_admin": false
        });
        let search = json!({
            "type_": "Users",
            "posts": [],
            "comments": [],
            "communities": [],
            "users": [person]
        })
        .to_string();
        let (api, search_request) = serve("200 OK", &search).await;
        let mut feed = Feed::default();
        let _ = feed.connect(api);
        let _ = feed.update(Message::Search("author".into(), Some(post.community.id)));
        let (task, _) = feed.update(Message::SearchKind(SearchKind::People.index()));
        let _ = feed.update(loaded(task).await);
        assert_eq!(feed.people[0].person.id, post.creator.id);
        let request = search_request.await.unwrap();
        let line = request.lines().next().unwrap();
        assert!(line.contains("type_=Users"));
        assert!(!line.contains("community_id"));

        let profile = json!({
            "person_view": person,
            "posts": [post],
            "comments": [comment],
            "moderates": []
        })
        .to_string();
        let (api, profile_request) = serve("200 OK", &profile).await;
        feed.api = Some(api);
        let (task, _) = feed.update(Message::Scope(Scope::Profile(post.creator.id)));
        let posts = loaded(task).await;
        let _ = feed.update(posts.clone());
        assert_eq!(feed.profile().unwrap().id, post.creator.id);
        assert_eq!(feed.posts, vec![post.clone()]);
        assert!(feed.comments.is_empty());

        let (api, comments_request) = serve("200 OK", &profile).await;
        feed.api = Some(api);
        let (task, _) = feed.update(Message::ProfileTab(1));
        let _ = feed.update(posts);
        assert!(feed.posts.is_empty());
        assert!(feed.busy);
        let _ = feed.update(loaded(task).await);
        assert_eq!(feed.comments, vec![comment]);
        assert!(feed.viewing_comments());

        for request in [profile_request, comments_request] {
            let request = request.await.unwrap();
            let line = request.lines().next().unwrap();
            assert!(line.starts_with("GET /api/v3/user?"));
            assert!(line.contains("person_id=1"));
            assert!(line.contains("page=1"));
        }
    }
    #[tokio::test]
    async fn federated_reference_opens_the_resolved_item_for_the_active_instance() {
        let post = post_view();
        let url = "https://remote.example/post/200";
        let body = json!({"post": post}).to_string();
        let (api, request) = serve("200 OK", &body).await;
        let mut feed = Feed::default();
        let _ = feed.connect(api.authenticated("reader".into()));
        let (task, _) = feed.update(Message::Search(url.into(), None));
        let Message::ReferenceResolved(generation, result) = loaded(task).await else {
            panic!("expected a reference lookup");
        };
        let open = feed.finish_resolution(generation, result.clone()).unwrap();
        assert_eq!(feed.update(open).1, Some((post, None)));

        let request = request.await.unwrap();
        let line = request.lines().next().unwrap();
        assert!(line.starts_with(
            "GET /api/v3/resolve_object?q=https%3A%2F%2Fremote.example%2Fpost%2F200 "
        ));
        assert!(request.contains("authorization: Bearer reader\r\n"));

        let _ = feed.update(Message::Search("other keywords".into(), None));
        assert!(feed.finish_resolution(generation, result).is_none());
        assert!(feed.posts.is_empty());
    }

    #[test]
    fn distinguishes_federated_references_from_keywords() {
        for query in [
            "https://remote.example/comment/12",
            "!community@remote.example",
            "@person@remote.example",
        ] {
            assert!(service::is_federated_reference(query), "{query}");
        }
        for query in [
            "hello world",
            "!community",
            "person@example.org",
            "@person@",
            "discussion about https://remote.example/post/1",
            "file:///tmp/post",
        ] {
            assert!(!service::is_federated_reference(query), "{query}");
        }
    }
}
