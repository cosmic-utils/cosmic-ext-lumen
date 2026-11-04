//! Application shell and cross-feature navigation.
mod navigation;
mod view;

use crate::{
    features::{accounts, community, feed, inbox, instance, post, publish},
    shared::links::open_web_link,
};
use cosmic::{
    ApplicationExt,
    app::Task,
    iced::{self, Subscription, keyboard, widget::operation, window},
    widget::{markdown, nav_bar},
};
use lemmy_api_common::{
    lemmy_db_schema::{SubscribedType, newtypes::CommunityId, source::community::Community},
    lemmy_db_views_actor::structs::CommunityView,
};

const SEARCH_ID: &str = "lemmy-search";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Destination {
    #[default]
    Feed,
    Inbox,
}

#[derive(Clone, Copy)]
enum NavItem {
    Feed(feed::Scope),
    Inbox,
}

#[derive(Default)]
enum Drawer {
    #[default]
    Closed,
    Accounts,
    Community(Box<CommunityDetails>),
}

struct CommunityDetails {
    id: CommunityId,
    result: Option<Result<Box<CommunityView>, String>>,
    follow_error: Option<String>,
    description: markdown::Content,
}

pub struct App {
    accounts: accounts::Accounts,
    core: cosmic::Core,
    instance: instance::Instance,
    feed: feed::Feed,
    post: post::Post,
    inbox: inbox::Inbox,
    publish: publish::Publish,
    nav: nav_bar::Model,
    destination: Destination,
    community_scope: Option<Community>,
    drawer: Drawer,
    drawer_request: u64,
    pending_follow: Option<CommunityId>,
    session: u64,
    search: String,
    feed_offset: f32,
    width: f32,
    focused: bool,
    compact: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    Accounts(accounts::Message),
    Instance(instance::Message),
    Feed(feed::Message),
    Post(post::Message),
    Inbox(inbox::Message),
    Publish(publish::Message),
    OpenAccounts,
    CloseDrawer,
    SearchInput(String),
    Search,
    FocusSearch,
    NewPost,
    ToggleDensity,
    Resized(window::Id, f32),
    CommunityLoaded(u64, Result<Box<CommunityView>, String>),
    FollowCommunity,
    Followed(u64, CommunityId, Result<Box<CommunityView>, String>),
    OpenLink(String),
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = reqwest::Client;
    type Message = Message;
    const APP_ID: &'static str = "io.github.cosmic_utils.cosmic_ext_lemmy";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, http: Self::Flags) -> (Self, Task<Message>) {
        let mut app = Self {
            accounts: accounts::Accounts::new(http.clone()),
            core,
            instance: instance::Instance::new(http),
            feed: feed::Feed::default(),
            post: post::Post::default(),
            inbox: inbox::Inbox::default(),
            publish: publish::Publish::default(),
            nav: nav_bar::Model::default(),
            destination: Destination::Feed,
            community_scope: None,
            drawer: Drawer::Closed,
            drawer_request: 0,
            pending_follow: None,
            session: 0,
            search: String::new(),
            feed_offset: 0.0,
            width: 1100.0,
            focused: false,
            compact: false,
        };
        app.set_header_title("Lemmy".into());
        app.rebuild_nav();
        let title = app
            .core
            .main_window_id()
            .map_or_else(Task::none, |id| app.set_window_title("Lemmy".into(), id));
        let restore = app
            .accounts
            .restore()
            .map(Message::Accounts)
            .map(cosmic::Action::App);
        (app, Task::batch([title, restore]))
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        match self.nav.data::<NavItem>(id).copied() {
            Some(NavItem::Feed(feed::Scope::Community(id))) => self.open_community(id),
            Some(NavItem::Feed(scope)) => self.navigate_feed(feed::Message::Scope(scope)),
            Some(NavItem::Inbox) => {
                self.leave_discussion();
                self.destination = Destination::Inbox;
                self.feed_offset = 0.0;
                self.search.clear();
                self.rebuild_nav();
                Task::batch([
                    self.restore_feed_scroll(),
                    self.update_inbox(inbox::Message::Refresh),
                ])
            }
            None => Task::none(),
        }
    }

    fn header_center(&self) -> Vec<cosmic::Element<'_, Message>> {
        self.search_control()
    }

    fn header_end(&self) -> Vec<cosmic::Element<'_, Message>> {
        self.header_controls()
    }

    fn context_drawer(&self) -> Option<cosmic::app::ContextDrawer<'_, Message>> {
        self.drawer_view()
    }

    fn dialog(&self) -> Option<cosmic::Element<'_, Message>> {
        self.publish.visible.then(|| {
            self.publish
                .view(self.accounts.follows())
                .map(Message::Publish)
        })
    }

    fn view(&self) -> cosmic::Element<'_, Message> {
        self.workspace()
    }

    fn on_escape(&mut self) -> Task<Message> {
        if self.publish.visible {
            return self.update(Message::Publish(publish::Message::Close));
        }
        if !matches!(self.drawer, Drawer::Closed) {
            return self.update(Message::CloseDrawer);
        }
        if self.focused || (!self.can_split() && self.post.selected.is_some()) {
            return self.update(Message::Post(post::Message::Back));
        }
        Task::none()
    }

    fn on_search(&mut self) -> Task<Message> {
        self.update(Message::FocusSearch)
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            window::resize_events().map(|(id, size)| Message::Resized(id, size.width)),
            iced::event::listen_with(|event, _, _| {
                if let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) =
                    event
                    && modifiers.control()
                    && key.as_ref() == keyboard::Key::Character("k")
                {
                    return Some(Message::FocusSearch);
                }
                None
            }),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Accounts(message) => {
                let (task, session) = self.accounts.update(message, &self.instance.input);
                let task = task.map(Message::Accounts).map(cosmic::Action::App);
                if let Some(api) = session {
                    Task::batch([task, self.connect(api)])
                } else {
                    task
                }
            }
            Message::Instance(message) => {
                let (task, connected) = self.instance.update(message);
                let task = task.map(Message::Instance).map(cosmic::Action::App);
                if let Some(api) = connected {
                    self.accounts.anonymous();
                    Task::batch([task, self.connect(api)])
                } else {
                    task
                }
            }
            Message::Feed(feed::Message::Scrolled(offset))
            | Message::Inbox(inbox::Message::Scrolled(offset)) => {
                self.feed_offset = offset;
                Task::none()
            }
            Message::Feed(feed::Message::OpenPerson(id)) => {
                self.navigate_feed(feed::Message::Scope(feed::Scope::Profile(id)))
            }
            Message::Feed(feed::Message::OpenLink(url)) => self.update(Message::OpenLink(url)),
            Message::Feed(feed::Message::OpenCommunity(id)) => self.open_community(id),
            Message::Feed(feed::Message::CommunityDetails(id)) => self.open_community_details(id),
            Message::Feed(message) => self.update_feed(message),
            Message::Inbox(message) => self.update_inbox(message),
            Message::Post(message) => self.update_post(message),
            Message::Publish(message) => {
                if self.accounts.busy && matches!(message, publish::Message::Submit) {
                    return Task::none();
                }
                let (task, created) = self.publish.update(
                    message,
                    self.feed.api(),
                    self.accounts.follows(),
                    self.session,
                );
                let task = task.map(Message::Publish).map(cosmic::Action::App);
                if let Some(post) = created {
                    let feed = self.navigate_feed(feed::Message::Scope(feed::Scope::All));
                    Task::batch([task, feed, self.open_post(post, None)])
                } else {
                    task
                }
            }
            Message::OpenAccounts => {
                if matches!(self.drawer, Drawer::Accounts) {
                    self.close_drawer();
                } else {
                    self.drawer = Drawer::Accounts;
                    self.set_show_context(true);
                }
                Task::none()
            }
            Message::CloseDrawer => {
                self.close_drawer();
                Task::none()
            }
            Message::SearchInput(value) => {
                self.search = value;
                if self.search.is_empty()
                    && self.destination == Destination::Feed
                    && self.feed.scope() == feed::Scope::Search
                {
                    return self.navigate_feed(feed::Message::Scope(feed::Scope::All));
                }
                Task::none()
            }
            Message::Search if !self.search.trim().is_empty() && self.feed.api().is_some() => {
                if self.feed.scope() != feed::Scope::Search {
                    self.feed.set_search_communities(
                        self.accounts.follows(),
                        self.community_scope
                            .as_ref()
                            .or_else(|| self.post.selected.as_ref().map(|post| &post.community)),
                    );
                }
                self.navigate_feed(feed::Message::Search(
                    self.search.trim().to_owned(),
                    self.community_scope.as_ref().map(|community| community.id),
                ))
            }
            Message::FocusSearch if self.feed.api().is_some() && !self.publish.visible => {
                operation::focus(SEARCH_ID)
            }
            Message::NewPost if self.feed.signed_in() && !self.accounts.busy => {
                self.close_drawer();
                self.publish.open(
                    self.community_scope
                        .as_ref()
                        .or_else(|| self.post.selected.as_ref().map(|post| &post.community)),
                );
                Task::none()
            }
            Message::ToggleDensity => {
                self.compact = !self.compact;
                Task::none()
            }
            Message::Resized(id, width) if Some(id) == self.core.main_window_id() => {
                self.width = width;
                if self.can_split() && !self.focused && self.post.selected.is_some() {
                    return self.restore_feed_scroll();
                }
                Task::none()
            }
            Message::CommunityLoaded(request, result) if request == self.drawer_request => {
                if let Drawer::Community(details) = &mut self.drawer {
                    if let Ok(view) = &result {
                        details.description = markdown::Content::parse(
                            view.community.description.as_deref().unwrap_or_default(),
                        );
                    }
                    details.result = Some(result);
                }
                Task::none()
            }
            Message::FollowCommunity => self.follow_community(),
            Message::Followed(session, id, result) if session == self.session => {
                self.pending_follow = None;
                match result {
                    Ok(view) => {
                        self.accounts.update_follow(&view);
                        if let Drawer::Community(details) = &mut self.drawer
                            && details.id == id
                        {
                            self.drawer_request += 1;
                            details.description = markdown::Content::parse(
                                view.community.description.as_deref().unwrap_or_default(),
                            );
                            details.result = Some(Ok(view));
                        }
                        self.rebuild_nav();
                        if self.destination == Destination::Feed
                            && self.feed.scope() == feed::Scope::Subscribed
                        {
                            return self.update_feed(feed::Message::Refresh);
                        }
                    }
                    Err(error) => {
                        if let Drawer::Community(details) = &mut self.drawer
                            && details.id == id
                        {
                            details.follow_error = Some(error);
                        }
                    }
                }
                Task::none()
            }
            Message::OpenLink(url) => {
                if let Err(error) = open_web_link(&url)
                    && let Drawer::Community(details) = &mut self.drawer
                {
                    details.follow_error = Some(error);
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }
}

impl App {
    fn open_community_details(&mut self, id: CommunityId) -> Task<Message> {
        let Some(api) = self.feed.api() else {
            return Task::none();
        };
        if matches!(&self.drawer, Drawer::Community(details) if details.id == id) {
            self.close_drawer();
            return Task::none();
        }
        self.drawer_request += 1;
        let request = self.drawer_request;
        self.drawer = Drawer::Community(Box::new(CommunityDetails {
            id,
            result: None,
            follow_error: None,
            description: markdown::Content::new(),
        }));
        self.set_show_context(true);
        Task::perform(
            async move { community::get(&api, id).await.map(Box::new) },
            move |result| cosmic::Action::App(Message::CommunityLoaded(request, result)),
        )
    }

    fn follow_community(&mut self) -> Task<Message> {
        let (Some(api), Drawer::Community(details)) = (self.feed.api(), &mut self.drawer) else {
            return Task::none();
        };
        if !api.is_authenticated() || self.accounts.busy || self.pending_follow.is_some() {
            return Task::none();
        }
        let Some(Ok(view)) = &details.result else {
            return Task::none();
        };
        details.follow_error = None;
        let id = details.id;
        let follow = view.subscribed == SubscribedType::NotSubscribed;
        let session = self.session;
        self.pending_follow = Some(id);
        Task::perform(
            async move { community::follow(api, id, follow).await.map(Box::new) },
            move |result| cosmic::Action::App(Message::Followed(session, id, result)),
        )
    }
}
