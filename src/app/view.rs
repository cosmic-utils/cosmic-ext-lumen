use super::{App, Destination, Drawer, Message, SEARCH_ID};
use crate::{
    features::{feed, post},
    shared::presentation::{community_handle, toggle_button_class},
};
use cosmic::{
    Element,
    app::{ContextDrawer, context_drawer},
    cosmic_theme::Layer,
    iced::Length,
    widget::{self, button, icon},
};
use lemmy_api_common::lemmy_db_schema::SubscribedType;

impl App {
    pub(super) fn search_control(&self) -> Vec<Element<'_, Message>> {
        if self.feed.api().is_none() {
            return Vec::new();
        }
        let width = (self.width - 200.0).clamp(110.0, 330.0);
        let search = widget::search_input("Search posts", &self.search)
            .id(SEARCH_ID.into())
            .width(Length::Fixed(width))
            .on_input(Message::SearchInput)
            .on_submit(|_| Message::Search);
        let search = if self.search.is_empty() {
            search
        } else {
            search.on_clear(Message::SearchInput(String::new()))
        };
        vec![search.into()]
    }

    pub(super) fn header_controls(&self) -> Vec<Element<'_, Message>> {
        let mut controls = Vec::with_capacity(3);
        if self.feed.api().is_some() && self.width >= 850.0 {
            controls.push(
                button::icon(icon::from_name("view-list-symbolic"))
                    .tooltip(if self.compact {
                        "Comfortable feed spacing"
                    } else {
                        "Compact feed spacing"
                    })
                    .class(toggle_button_class(self.compact, button::ButtonClass::Icon))
                    .on_press(Message::ToggleDensity)
                    .into(),
            );
        }
        if self.feed.signed_in() {
            if self.width >= 680.0 {
                controls.push(
                    button::suggested("New post")
                        .leading_icon(icon::from_name("list-add-symbolic"))
                        .on_press_maybe((!self.accounts.busy).then_some(Message::NewPost))
                        .into(),
                );
            } else {
                controls.push(
                    button::icon(icon::from_name("list-add-symbolic"))
                        .tooltip("New post")
                        .on_press_maybe((!self.accounts.busy).then_some(Message::NewPost))
                        .into(),
                );
            }
        }
        let account = self.accounts.active();
        if self.width >= 680.0 {
            controls.push(
                button::standard(account.map_or("Accounts", |account| account.name.as_str()))
                    .leading_icon(icon::from_name("avatar-default-symbolic"))
                    .trailing_icon(icon::from_name("pan-down-symbolic"))
                    .on_press(Message::OpenAccounts)
                    .into(),
            );
        } else {
            controls.push(
                button::icon(icon::from_name("avatar-default-symbolic"))
                    .tooltip(account.map_or("Accounts", |account| account.name.as_str()))
                    .on_press(Message::OpenAccounts)
                    .into(),
            );
        }
        controls
    }

    pub(super) fn workspace(&self) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        if self.feed.api().is_none() {
            let mut welcome = widget::column::with_capacity(3)
                .spacing(space.space_s)
                .push(widget::text::title1("Lemmy"));
            if self.accounts.busy {
                welcome = welcome.push(widget::text::body("Restoring account…"));
            } else {
                welcome = welcome
                    .push(self.instance.view().map(Message::Instance))
                    .push(button::standard("Sign in").on_press(Message::OpenAccounts));
            }
            return widget::container(
                widget::container(welcome)
                    .width(Length::Fill)
                    .max_width(440)
                    .padding(space.space_m),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        }

        let split = self.can_split() && !self.focused;
        let discussion = self.post.selected.as_ref();
        let show_feed = split || discussion.is_none();
        let show_discussion = split || discussion.is_some();
        let mut row = widget::row::with_capacity(2).spacing(space.space_xxs);
        if show_feed {
            let list = match self.destination {
                Destination::Feed => self
                    .feed
                    .view(
                        self.feed_title(),
                        discussion.map(|post| post.post.id),
                        self.compact,
                    )
                    .map(Message::Feed),
                Destination::Inbox => self
                    .inbox
                    .view(self.post.focused_comment)
                    .map(Message::Inbox),
            };
            let width = if split {
                let sidebar = if self.core.nav_bar_active() {
                    240.0
                } else {
                    0.0
                };
                Length::Fixed(((self.width - sidebar) * 0.36).clamp(280.0, 400.0))
            } else {
                Length::Fill
            };
            row = row.push(
                widget::layer_container(list)
                    .layer(Layer::Primary)
                    .padding(space.space_xxs)
                    .width(width)
                    .height(Length::Fill),
            );
        }
        if show_discussion {
            row = row.push(
                widget::layer_container(
                    self.post
                        .view(
                            self.accounts.busy,
                            !split,
                            self.can_split(),
                            self.focused,
                            self.width < 420.0,
                        )
                        .map(Message::Post),
                )
                .layer(Layer::Background)
                .width(Length::Fill)
                .height(Length::Fill),
            );
        }
        widget::container(row)
            .padding([0, space.space_xxs, space.space_xxs, space.space_xxs])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn feed_title(&self) -> String {
        match self.feed.scope() {
            feed::Scope::All => "All posts".into(),
            feed::Scope::Local => "Local".into(),
            feed::Scope::Subscribed => "Subscribed".into(),
            feed::Scope::Saved => "Saved".into(),
            feed::Scope::Search => "Search results".into(),
            feed::Scope::Profile(_) => self.feed.profile().map_or_else(
                || "Profile".into(),
                |person| {
                    person
                        .display_name
                        .clone()
                        .unwrap_or_else(|| person.name.clone())
                },
            ),
            feed::Scope::Community(_) => self
                .community_scope
                .as_ref()
                .map_or_else(|| "Community".into(), |community| community.title.clone()),
        }
    }

    pub(super) fn drawer_view(&self) -> Option<ContextDrawer<'_, Message>> {
        let space = cosmic::theme::active().cosmic().spacing;
        match &self.drawer {
            Drawer::Closed => None,
            Drawer::Accounts => {
                let mut content = widget::column::with_capacity(3).spacing(space.space_s);
                if !self.accounts.busy {
                    content = content
                        .push(self.instance.view().map(Message::Instance))
                        .push(widget::divider::horizontal::light());
                }
                content = content.push(self.accounts.view().map(Message::Accounts));
                Some(context_drawer(content, Message::CloseDrawer).title("Accounts and instances"))
            }
            Drawer::Community(details) => {
                let mut content = widget::column::with_capacity(6).spacing(space.space_s);
                match &details.result {
                    Some(Ok(view)) => {
                        let community = &view.community;
                        let handle = community_handle(community);
                        content = content
                            .push(widget::text::title3(&community.title))
                            .push(widget::text::caption(format!("!{handle}")))
                            .push(widget::text::caption(format!(
                                "{} subscribers",
                                view.counts.subscribers
                            )));
                        let mut actions = widget::column::with_capacity(3)
                            .spacing(space.space_xxs)
                            .push(button::standard("Browse community").on_press(Message::Post(
                                post::Message::OpenCommunity(community.id),
                            )));
                        if self.feed.signed_in() {
                            let label = match view.subscribed {
                                SubscribedType::Subscribed => "Unsubscribe",
                                SubscribedType::NotSubscribed => "Subscribe",
                                SubscribedType::Pending => "Cancel pending subscription",
                            };
                            actions = actions.push(
                                button::standard(label)
                                    .class(toggle_button_class(
                                        view.subscribed == SubscribedType::Subscribed,
                                        button::ButtonClass::Standard,
                                    ))
                                    .on_press_maybe(
                                        (self.pending_follow.is_none() && !self.accounts.busy)
                                            .then_some(Message::FollowCommunity),
                                    ),
                            );
                        }
                        actions = actions.push(
                            button::text("Open in browser")
                                .on_press(Message::OpenLink(community.actor_id.to_string())),
                        );
                        content = content.push(actions);
                        if community.description.is_some() {
                            content = content.push(widget::divider::horizontal::light()).push(
                                post::render_markdown(&details.description, Message::OpenLink),
                            );
                        }
                    }
                    Some(Err(error)) => content = content.push(widget::text::body(error)),
                    None => content = content.push(widget::text::body("Loading community…")),
                }
                if let Some(error) = &details.follow_error {
                    content = content.push(widget::text::body(error));
                }
                Some(context_drawer(content, Message::CloseDrawer).title("Community details"))
            }
        }
    }
}
