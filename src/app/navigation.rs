use super::{App, Destination, Drawer, Message, NavItem};
use crate::{
    features::{feed, inbox, post, publish},
    shared::{api::Api, cache::Cache, media, presentation::community_title},
};
use cosmic::{
    ApplicationExt,
    app::Task,
    iced::widget::operation,
    widget::{icon, nav_bar},
};
use lemmy_api_common::{
    lemmy_db_schema::{
        newtypes::{CommentId, CommunityId},
        source::community::Community,
    },
    lemmy_db_views::structs::PostView,
};

const NAVIGATION_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Clone, Debug)]
pub struct CachedNavigation {
    scope: String,
    follows: Vec<Community>,
    unread: i64,
}

impl CachedNavigation {
    pub(super) fn load(scope: String) -> Task<Message> {
        cosmic::iced::Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let follows = Cache::Navigation
                        .read(&format!("{scope}follows"), NAVIGATION_MAX_AGE)
                        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
                    let unread = Cache::Navigation
                        .read(&format!("{scope}unread"), NAVIGATION_MAX_AGE)
                        .and_then(|bytes| serde_json::from_slice::<i64>(&bytes).ok());
                    if follows.is_none() && unread.is_none() {
                        return None;
                    }
                    Some(Self {
                        scope,
                        follows: follows.unwrap_or_default(),
                        unread: unread.unwrap_or_default().max(0),
                    })
                })
                .await
                .ok()
                .flatten()
            },
            |snapshot| snapshot,
        )
        .and_then(cosmic::iced::Task::done)
        .map(Message::CachedNavigation)
        .map(cosmic::Action::App)
    }
}

fn save_navigation(api: &Api, part: &str, value: &impl serde::Serialize) -> Task<Message> {
    let (Some(scope), Ok(bytes)) = (api.reading_scope(), serde_json::to_vec(value)) else {
        return Task::none();
    };
    let key = format!("{scope}{part}");
    cosmic::iced::Task::future(async move {
        let _ = tokio::task::spawn_blocking(move || Cache::Navigation.write(&key, &bytes)).await;
    })
    .discard()
}

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
        if !self.feed.has_startup_preview() {
            self.feed_offset = 0.0;
        }
        self.close_drawer();
        let unread = self
            .startup_navigation
            .take()
            .filter(|cached| api.reading_scope().as_deref() == Some(&cached.scope))
            .map_or(0, |cached| cached.unread);
        let cache_navigation = if api.is_authenticated() {
            save_navigation(&api, "follows", &self.accounts.follows())
        } else {
            Task::none()
        };
        let inbox = self
            .inbox
            .connect(&api)
            .map(Message::Inbox)
            .map(cosmic::Action::App);
        self.inbox.unread = unread;
        let feed = self
            .feed
            .connect(api)
            .map(Message::Feed)
            .map(cosmic::Action::App);
        self.rebuild_nav();
        Task::batch([
            feed,
            inbox,
            cache_navigation,
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
            feed::Message::CachedFeed(..)
                | feed::Message::Loaded(..)
                | feed::Message::SearchLoaded(..)
                | feed::Message::ProfileLoaded(..)
                | feed::Message::SavedCommentsLoaded(..)
        );
        let skip_thumbnails = if matches!(message, feed::Message::Loaded(_, false, _)) {
            0
        } else {
            self.feed.posts.len()
        };
        let (task, selected) = self.feed.update(message);
        let thumbnails = self.media.request(
            self.feed
                .posts
                .iter()
                .skip(skip_thumbnails)
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
        let confirmed_count = self.inbox.is_current_unread_response(&message);
        let unread = self.inbox.unread;
        let (task, selected) = self.inbox.update(message);
        let unread_changed = self.inbox.unread != unread;
        if unread_changed {
            self.rebuild_nav();
        }
        let cache = if confirmed_count || unread_changed {
            self.feed
                .api()
                .filter(Api::is_authenticated)
                .map_or_else(Task::none, |api| {
                    save_navigation(&api, "unread", &self.inbox.unread)
                })
        } else {
            Task::none()
        };
        let avatars = if received_notices {
            self.media
                .request(self.inbox.avatar_urls(), media::Size::Avatar)
        } else {
            cosmic::iced::Task::none()
        };
        let task = Task::batch([
            task.map(Message::Inbox).map(cosmic::Action::App),
            cache,
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

    fn navigation_follows(&self) -> &[Community] {
        self.startup_navigation
            .as_ref()
            .map_or_else(|| self.accounts.follows(), |cached| &cached.follows)
    }

    pub(super) fn navigation_communities(&self) -> impl Iterator<Item = &Community> {
        self.navigation_follows()
            .iter()
            .chain(self.community_scope.iter())
    }

    pub(super) fn cache_navigation_follows(&self) -> Task<Message> {
        self.feed
            .api()
            .filter(Api::is_authenticated)
            .map_or_else(Task::none, |api| {
                save_navigation(&api, "follows", &self.accounts.follows())
            })
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
        let signed_in = self.feed.signed_in() || self.startup_navigation.is_some();
        for (item, text, image) in [
            (feed::Scope::All, "All posts", "network-workgroup-symbolic"),
            (feed::Scope::Local, "Local", "network-server-symbolic"),
            (feed::Scope::Subscribed, "Subscribed", "view-list-symbolic"),
            (feed::Scope::Saved, "Saved", "user-bookmarks-symbolic"),
        ] {
            if !signed_in && matches!(item, feed::Scope::Subscribed | feed::Scope::Saved) {
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
        if signed_in {
            let unread = self
                .startup_navigation
                .as_ref()
                .map_or(self.inbox.unread, |cached| cached.unread);
            let title = if unread > 0 {
                format!("Inbox ({unread})")
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
        let mut communities: Vec<_> = self.navigation_follows().iter().collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::post::tests::post_view;
    use cosmic::iced::{futures::StreamExt, runtime};

    #[tokio::test]
    async fn navigation_preview_loads_communities_and_badge_for_one_account() {
        let instance = "https://example.com";
        let account = uuid::Uuid::new_v4().to_string();
        let scope = Api::account_cache_scope(instance, &account).unwrap();
        let community = post_view().community;
        Cache::Navigation.write(
            &format!("{scope}follows"),
            &serde_json::to_vec(&[&community]).unwrap(),
        );
        Cache::Navigation.write(&format!("{scope}unread"), b"3");

        let mut preview =
            runtime::task::into_stream(CachedNavigation::load(scope.clone())).unwrap();
        let Some(runtime::Action::Output(cosmic::Action::App(Message::CachedNavigation(cached)))) =
            preview.next().await
        else {
            panic!("expected cached navigation");
        };
        assert_eq!(cached.scope, scope);
        assert_eq!(cached.follows[0].id, community.id);
        assert_eq!(cached.unread, 3);

        let other = Api::account_cache_scope(instance, &format!("{account}-other")).unwrap();
        let mut preview = runtime::task::into_stream(CachedNavigation::load(other)).unwrap();
        assert!(preview.next().await.is_none());
        Cache::Navigation.invalidate(&scope);
    }
}
