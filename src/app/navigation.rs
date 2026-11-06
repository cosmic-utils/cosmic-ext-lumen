use super::{App, Destination, Drawer, Message, NavItem};
use crate::{
    features::{feed, inbox, post, publish},
    shared::{api::Api, media, presentation::community_title},
};
use cosmic::{
    ApplicationExt,
    app::Task,
    iced::widget::operation,
    widget::{icon, nav_bar},
};
use lemmy_api_common::{
    lemmy_db_schema::newtypes::{CommentId, CommunityId},
    lemmy_db_views::structs::PostView,
};

impl App {
    pub(super) fn can_split(&self) -> bool {
        let minimum = if self.core.nav_bar_active() {
            980.0
        } else {
            750.0
        };
        self.width >= minimum
    }

    pub(super) fn close_drawer(&mut self) {
        self.drawer = Drawer::Closed;
        self.drawer_request += 1;
        self.set_show_context(false);
    }

    pub(super) fn leave_discussion(&mut self) {
        self.post.close();
        let _ = self.media.update(media::Message::Close);
        self.focused = false;
        self.close_drawer();
    }

    pub(super) fn connect(&mut self, api: Api) -> Task<Message> {
        self.session += 1;
        if self.instance.input != api.base.as_str() {
            self.instance.name = None;
        }
        self.instance.input = api.base.to_string();
        self.post.reset();
        let _ = self.media.update(media::Message::Close);
        self.post.set_person(self.accounts.person_id());
        self.publish = publish::Publish::default();
        self.pending_follow = None;
        self.destination = Destination::Feed;
        self.community_scope = None;
        self.search.clear();
        self.focused = false;
        self.feed_offset = 0.0;
        self.close_drawer();
        let inbox = self
            .inbox
            .connect(&api)
            .map(Message::Inbox)
            .map(cosmic::Action::App);
        let feed = self
            .feed
            .connect(api)
            .map(Message::Feed)
            .map(cosmic::Action::App);
        self.rebuild_nav();
        Task::batch([
            feed,
            inbox,
            self.restore_feed_scroll(),
            self.load_navigation_avatars(),
        ])
    }

    pub(super) fn navigate_feed(&mut self, message: feed::Message) -> Task<Message> {
        self.inbox.dismiss_open();
        if !matches!(message, feed::Message::Scope(feed::Scope::Community(_))) {
            self.community_scope = None;
        }
        if !matches!(message, feed::Message::Search(..)) {
            self.search.clear();
        }
        self.leave_discussion();
        self.destination = Destination::Feed;
        self.feed_offset = 0.0;
        let task = self.update_feed(message);
        self.rebuild_nav();
        Task::batch([
            task,
            self.restore_feed_scroll(),
            self.load_navigation_avatars(),
        ])
    }

    pub(super) fn open_community(&mut self, id: CommunityId) -> Task<Message> {
        let community = self
            .accounts
            .follows()
            .iter()
            .find(|item| item.id == id)
            .or_else(|| {
                self.post
                    .selected
                    .as_ref()
                    .map(|post| &post.community)
                    .filter(|item| item.id == id)
            })
            .or_else(|| self.community_scope.as_ref().filter(|item| item.id == id))
            .or_else(|| self.feed.found_community(id))
            .cloned();
        let Some(community) = community else {
            return Task::none();
        };
        self.community_scope = Some(community);
        self.navigate_feed(feed::Message::Scope(feed::Scope::Community(id)))
    }

    pub(super) fn update_feed(&mut self, message: feed::Message) -> Task<Message> {
        let reset_scroll = matches!(
            message,
            feed::Message::Sort(_)
                | feed::Message::Refresh
                | feed::Message::SavedTab(_)
                | feed::Message::ProfileTab(_)
                | feed::Message::SearchKind(_)
                | feed::Message::SearchCommunity(_)
        );
        if reset_scroll {
            self.feed_offset = 0.0;
        }
        let received_results = matches!(
            message,
            feed::Message::Loaded(..)
                | feed::Message::SearchLoaded(..)
                | feed::Message::ProfileLoaded(..)
                | feed::Message::SavedCommentsLoaded(..)
        );
        let previous_count = self.feed.posts.len();
        let (task, selected) = self.feed.update(message);
        let thumbnails = self.media.request(
            self.feed
                .posts
                .iter()
                .skip(previous_count)
                .filter_map(|post| {
                    (!post.community.nsfw)
                        .then(|| media::thumbnail(&post.post))
                        .flatten()
                        .map(str::to_owned)
                }),
            media::Size::Thumbnail,
        );
        let avatars = if received_results {
            self.media
                .request(self.feed.avatar_urls(), media::Size::Avatar)
        } else {
            cosmic::iced::Task::none()
        };
        let task = Task::batch([
            task.map(Message::Feed).map(cosmic::Action::App),
            thumbnails.map(Message::Media).map(cosmic::Action::App),
            avatars.map(Message::Media).map(cosmic::Action::App),
        ]);
        if let Some((post, comment)) = selected.filter(|_| self.destination == Destination::Feed)
            && (comment.is_some()
                || self
                    .post
                    .selected
                    .as_ref()
                    .is_none_or(|current| current.post.id != post.post.id))
        {
            return Task::batch([task, self.open_post(post, comment)]);
        }
        if reset_scroll && self.destination == Destination::Feed {
            Task::batch([task, self.restore_feed_scroll()])
        } else {
            task
        }
    }

    pub(super) fn update_inbox(&mut self, message: inbox::Message) -> Task<Message> {
        if self.accounts.busy
            && matches!(
                message,
                inbox::Message::Open(_) | inbox::Message::SetRead(..)
            )
        {
            return Task::none();
        }
        let received_notices = matches!(message, inbox::Message::Loaded(..));
        let unread = self.inbox.unread;
        let (task, selected) = self.inbox.update(message);
        if self.inbox.unread != unread {
            self.rebuild_nav();
        }
        let avatars = if received_notices {
            self.media
                .request(self.inbox.avatar_urls(), media::Size::Avatar)
        } else {
            cosmic::iced::Task::none()
        };
        let task = Task::batch([
            task.map(Message::Inbox).map(cosmic::Action::App),
            avatars.map(Message::Media).map(cosmic::Action::App),
        ]);
        if let Some((post, comment)) = selected.filter(|_| self.destination == Destination::Inbox) {
            Task::batch([task, self.open_post(post, Some(comment))])
        } else {
            task
        }
    }

    pub(super) fn open_post(
        &mut self,
        post: PostView,
        comment: Option<CommentId>,
    ) -> Task<Message> {
        let Some(api) = self.feed.api() else {
            return Task::none();
        };
        if matches!(self.drawer, Drawer::Community(_)) {
            self.close_drawer();
        }
        let load = match comment {
            Some(id) => self.post.open_at(api, post, id),
            None => self.post.open(api, post),
        };
        Task::batch([
            load.map(Message::Post).map(cosmic::Action::App),
            self.load_discussion_images(),
            operation::scroll_to(
                post::SCROLL_ID,
                operation::AbsoluteOffset {
                    x: Some(0.0),
                    y: Some(0.0),
                },
            ),
        ])
    }

    fn load_discussion_images(&mut self) -> Task<Message> {
        cosmic::iced::Task::batch([
            self.media
                .request(self.post.image_urls(), media::Size::Preview),
            self.media
                .request(self.post.avatar_urls(), media::Size::Avatar),
        ])
        .map(Message::Media)
        .map(cosmic::Action::App)
    }

    pub(super) fn update_post(&mut self, message: post::Message) -> Task<Message> {
        match message {
            post::Message::Back => {
                if self.focused && self.can_split() {
                    self.focused = false;
                } else {
                    self.leave_discussion();
                }
                self.restore_feed_scroll()
            }
            post::Message::ToggleFocus if self.can_split() => {
                self.focused = !self.focused;
                if self.focused {
                    Task::none()
                } else {
                    self.restore_feed_scroll()
                }
            }
            post::Message::OpenImage(url) => self
                .media
                .update(media::Message::Open(url))
                .map(Message::Media)
                .map(cosmic::Action::App),
            post::Message::OpenCommunity(id) => self.open_community(id),
            post::Message::OpenPerson(id) => {
                self.navigate_feed(feed::Message::Scope(feed::Scope::Profile(id)))
            }
            post::Message::CommunityDetails => {
                if let Some(id) = self.post.selected.as_ref().map(|post| post.community.id) {
                    return self.open_community_details(id);
                }
                Task::none()
            }
            post::Message::Act(..)
            | post::Message::SubmitComment(..)
            | post::Message::Vote(..)
            | post::Message::SaveComment(..)
            | post::Message::DeleteComment(..)
                if self.accounts.busy =>
            {
                Task::none()
            }
            post::Message::CommentActionFinished(generation, result) => {
                if self.post.finish_comment_action(generation, result) {
                    self.feed
                        .saved_comments_changed()
                        .map(Message::Feed)
                        .map(cosmic::Action::App)
                } else {
                    Task::none()
                }
            }
            post::Message::CommentSubmitted(generation, result) => {
                let edited_saved_comment = self.post.editing_submission(generation)
                    && result.as_ref().is_ok_and(|comment| comment.saved);
                if let Some(id) = self.post.finish_comment(generation, result) {
                    self.feed.comment_added(id);
                }
                let reload = if edited_saved_comment {
                    self.feed
                        .saved_comments_changed()
                        .map(Message::Feed)
                        .map(cosmic::Action::App)
                } else {
                    Task::none()
                };
                Task::batch([reload, self.load_discussion_images()])
            }
            post::Message::ActionFinished(generation, result) => {
                if let Some(updated) = self.post.finish_action(generation, result) {
                    self.feed
                        .apply_post_update(updated)
                        .map(Message::Feed)
                        .map(cosmic::Action::App)
                } else {
                    Task::none()
                }
            }
            message @ (post::Message::Loaded(..) | post::Message::FocusLoaded(..)) => {
                let task = self
                    .post
                    .update(message)
                    .map(Message::Post)
                    .map(cosmic::Action::App);
                Task::batch([task, self.load_discussion_images()])
            }
            message => self
                .post
                .update(message)
                .map(Message::Post)
                .map(cosmic::Action::App),
        }
    }

    pub(super) fn restore_feed_scroll(&self) -> Task<Message> {
        operation::scroll_to(
            feed::SCROLL_ID,
            operation::AbsoluteOffset {
                x: Some(0.0),
                y: Some(self.feed_offset),
            },
        )
    }

    pub(super) fn navigation_communities(
        &self,
    ) -> impl Iterator<Item = &lemmy_api_common::lemmy_db_schema::source::community::Community>
    {
        self.accounts
            .follows()
            .iter()
            .chain(self.community_scope.iter())
    }

    pub(super) fn load_navigation_avatars(&mut self) -> Task<Message> {
        let urls: Vec<_> = self
            .navigation_communities()
            .filter_map(|community| community.icon.as_ref().map(|url| url.to_string()))
            .chain(self.accounts.avatars().map(str::to_owned))
            .collect();
        self.media
            .request(urls, media::Size::Avatar)
            .map(Message::Media)
            .map(cosmic::Action::App)
    }

    pub(super) fn rebuild_nav(&mut self) {
        let mut nav = nav_bar::Model::default();
        let scope = self.feed.scope();
        for (item, text, image) in [
            (feed::Scope::All, "All posts", "network-workgroup-symbolic"),
            (feed::Scope::Local, "Local", "network-server-symbolic"),
            (feed::Scope::Subscribed, "Subscribed", "view-list-symbolic"),
            (feed::Scope::Saved, "Saved", "user-bookmarks-symbolic"),
        ] {
            if !self.feed.signed_in()
                && matches!(item, feed::Scope::Subscribed | feed::Scope::Saved)
            {
                continue;
            }
            let entry = nav
                .insert()
                .text(text)
                .icon(icon::from_name(image))
                .data(NavItem::Feed(item));
            let entry = if item == feed::Scope::Saved {
                entry.divider_above(true)
            } else {
                entry
            };
            if self.destination == Destination::Feed && scope == item {
                entry.activate();
            }
        }
        if self.feed.signed_in() {
            let title = if self.inbox.unread > 0 {
                format!("Inbox ({})", self.inbox.unread)
            } else {
                "Inbox".into()
            };
            let entry = nav
                .insert()
                .text(title)
                .icon(icon::from_name("mail-folder-inbox-symbolic"))
                .data(NavItem::Inbox);
            if self.destination == Destination::Inbox {
                entry.activate();
            }
        }
        let mut communities: Vec<_> = self.accounts.follows().iter().collect();
        if let Some(community) = &self.community_scope
            && !communities.iter().any(|item| item.id == community.id)
        {
            communities.push(community);
        }
        communities.sort_by_key(|community| community.name.to_lowercase());
        for (index, community) in communities.into_iter().enumerate() {
            let item = feed::Scope::Community(community.id);
            let entry = nav
                .insert()
                .text(community_title(community).to_owned())
                .icon(self.media.community_avatar(community).icon())
                .data(NavItem::Feed(item));
            let entry = if let Some(host) = community.actor_id.host_str() {
                entry.data(host.to_owned())
            } else {
                entry
            };
            let entry = if index == 0 {
                entry.divider_above(true)
            } else {
                entry
            };
            if self.destination == Destination::Feed && scope == item {
                entry.activate();
            }
        }
        self.nav = nav;
    }
}
