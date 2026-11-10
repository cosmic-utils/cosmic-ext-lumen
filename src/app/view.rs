use super::{App, Column, Destination, Drawer, Message, SEARCH_ID};
use crate::{
    features::{feed, post},
    shared::{
        media,
        presentation::{community_label, community_title, toggle_button_class},
    },
};
use cosmic::{
    Element,
    app::{ContextDrawer, context_drawer},
    cosmic_theme::Layer,
    iced::{Alignment, Length, core::text::Wrapping},
    widget::{self, button, icon, pane_grid},
};
use lemmy_api_common::lemmy_db_schema::SubscribedType;

impl App {
    pub(super) fn sidebar(&self) -> Option<Element<'_, cosmic::Action<Message>>> {
        if !self.core.nav_bar_active() {
            return None;
        }

        let theme = cosmic::theme::active();
        let space = theme.cosmic().spacing;
        let radius = theme.cosmic().corner_radii.radius_s;
        let mut entries =
            widget::column::with_capacity(self.nav.iter().count()).spacing(space.space_xxs);
        for id in self.nav.iter() {
            if self.nav.divider_above(id) == Some(true) {
                entries = entries.push(widget::divider::horizontal::light());
            }
            let mut label = widget::column::with_capacity(2).width(Length::Fill).push(
                widget::text::body(self.nav.text(id).unwrap_or_default())
                    .wrapping(Wrapping::WordOrGlyph),
            );
            if let Some(server) = self.nav.data::<String>(id) {
                label = label.push(widget::text::caption(server).wrapping(Wrapping::WordOrGlyph));
            }
            let row = widget::row::with_capacity(2)
                .spacing(space.space_xxs)
                .align_y(Alignment::Center)
                .push_maybe(self.nav.icon(id).cloned())
                .push(label);
            entries = entries.push(
                button::custom(row)
                    .class(button::ButtonClass::ListItem(radius))
                    .selected(self.nav.is_active(id))
                    .padding([space.space_xxs, space.space_s])
                    .width(Length::Fill)
                    .on_press(cosmic::Action::Cosmic(cosmic::app::Action::NavBar(id))),
            );
        }
        let width = if self.core.is_condensed() {
            Length::Fill
        } else {
            Length::Fixed(240.0)
        };
        Some(
            widget::container(
                widget::scrollable(widget::container(entries).padding(space.space_xxs))
                    .class(cosmic::style::iced::Scrollable::Minimal)
                    .height(Length::Fill),
            )
            .width(width)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(
                widget::nav_bar::nav_bar_style,
            ))
            .into(),
        )
    }

    pub(super) fn search_control(&self) -> Vec<Element<'_, Message>> {
        if self.feed.api().is_none() {
            return Vec::new();
        }
        let width = (self.width - 200.0).clamp(110.0, 330.0);
        let search = widget::search_input("Search or paste a link", &self.search)
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

    pub(super) fn new_post_control(&self) -> Vec<Element<'_, Message>> {
        if !self.feed.signed_in() {
            return Vec::new();
        }
        let action = (!self.accounts.busy).then_some(Message::NewPost);
        let control = if self.width >= 680.0 {
            button::suggested("New post")
                .leading_icon(icon::from_name("list-add-symbolic"))
                .on_press_maybe(action)
                .into()
        } else {
            button::icon(icon::from_name("list-add-symbolic"))
                .tooltip("New post")
                .on_press_maybe(action)
                .into()
        };
        vec![control]
    }

    pub(super) fn account_control(&self) -> Element<'_, Message> {
        let account = self.accounts.active();
        let avatar = self.media.avatar(
            account.and_then(|account| account.avatar.as_deref()),
            "avatar-default-symbolic",
        );
        let label = account.map_or("Accounts", |account| account.name.as_str());
        if self.width >= 680.0 {
            button::standard(label)
                .leading_icon(avatar)
                .trailing_icon(icon::from_name("pan-down-symbolic"))
                .on_press(Message::OpenAccounts)
                .into()
        } else {
            button::icon(avatar)
                .tooltip(label)
                .on_press(Message::OpenAccounts)
                .into()
        }
    }

    pub(super) fn workspace(&self) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        if self.feed.api().is_none() && !self.feed.has_startup_preview() {
            let mut welcome = widget::column::with_capacity(3)
                .spacing(space.space_s)
                .push(widget::text::title1(crate::i18n::app_title()));
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

        let body = if self.can_split() && !self.focused {
            pane_grid::PaneGrid::new(&self.columns, |_, column, _| {
                let content = match column {
                    Column::Feed => self.feed_column(),
                    Column::Discussion => cosmic::iced::widget::responsive(|size| {
                        self.discussion_column(false, size.width < 420.0)
                    })
                    .into(),
                };
                pane_grid::Content::new(content)
            })
            .spacing(space.space_xxs)
            .min_size(270)
            .on_resize(8, Message::ResizeColumns)
            .into()
        } else if self.post.selected.is_some() {
            self.discussion_column(true, self.width < 420.0)
        } else {
            self.feed_column()
        };
        widget::container(body)
            .padding([0, space.space_xxs, space.space_xxs, space.space_xxs])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn feed_column(&self) -> Element<'_, Message> {
        let list = match self.destination {
            Destination::Feed => self
                .feed
                .view(
                    self.feed_title(),
                    self.community_scope.as_ref().filter(|community| {
                        self.feed.scope() == feed::Scope::Community(community.id)
                    }),
                    self.post.selected.as_ref().map(|post| post.post.id),
                    self.compact,
                    &self.media,
                )
                .map(Message::Feed),
            Destination::Inbox => self
                .inbox
                .view(self.post.focused_comment, self.accounts.busy, &self.media)
                .map(Message::Inbox),
        };
        widget::layer_container(list)
            .layer(Layer::Primary)
            .padding(cosmic::theme::active().cosmic().spacing.space_xxs)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn discussion_column(&self, show_back: bool, narrow: bool) -> Element<'_, Message> {
        widget::layer_container(
            self.post
                .view(
                    self.accounts.busy,
                    show_back,
                    self.can_split(),
                    self.focused,
                    narrow,
                    &self.media,
                )
                .map(Message::Post),
        )
        .layer(Layer::Background)
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
            feed::Scope::Community(_) => self.community_scope.as_ref().map_or_else(
                || "Community".into(),
                |community| community_title(community).to_owned(),
            ),
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
                content = content.push(self.accounts.view(&self.media).map(Message::Accounts));
                content = content.push(widget::divider::horizontal::light()).push(
                    widget::toggler(self.media.automatic())
                        .label("Load images automatically".to_owned())
                        .spacing(space.space_xxs)
                        .on_toggle(|enabled| Message::Media(media::Message::Automatic(enabled))),
                );
                if let Some(error) = self.media.preference_error() {
                    content = content.push(widget::text::caption(error));
                }
                Some(context_drawer(content, Message::CloseDrawer).title("Accounts and instances"))
            }
            Drawer::Community(details) => {
                let mut content = widget::column::with_capacity(6).spacing(space.space_s);
                match &details.result {
                    Some(Ok(view)) => {
                        let community = &view.community;
                        content = content
                            .push(
                                widget::row::with_capacity(2)
                                    .align_y(cosmic::iced::Alignment::Center)
                                    .spacing(space.space_xxs)
                                    .push(self.media.community_avatar(community).icon().size(24))
                                    .push(widget::text::title3(community_label(community))),
                            )
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
