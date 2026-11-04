use super::{Feed, Message, Scope, SearchKind, Sort};
use crate::{
    features::post::render_markdown,
    shared::presentation::{age, community_handle, person_handle, selectable_card},
};
use cosmic::{
    Element,
    iced::{Alignment, Length, core::text::Wrapping},
    widget::{self, button, icon},
};
use lemmy_api_common::lemmy_db_schema::newtypes::PostId;

pub const SCROLL_ID: &str = "lemmy-feed";

impl Feed {
    pub fn view(
        &self,
        title: String,
        selected: Option<PostId>,
        compact: bool,
    ) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let mut controls = widget::row::with_capacity(5)
            .spacing(space.space_xxs)
            .align_y(Alignment::Center)
            .push(
                widget::text::title3(title)
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph),
            );
        let comments = self.viewing_comments();
        let reference = self.viewing_reference();
        let searching = self.scope == Scope::Search && !reference;
        let communities = searching && self.search_type == SearchKind::Communities;
        let people = searching && self.search_type == SearchKind::People;
        let profile = matches!(self.scope, Scope::Profile(_));
        if self.scope == Scope::Saved {
            controls = controls.push(widget::dropdown(
                &["Posts", "Comments"][..],
                Some(usize::from(comments)),
                Message::SavedTab,
            ));
        }
        if profile {
            controls = controls.push(widget::dropdown(
                &["Posts", "Comments"][..],
                Some(usize::from(self.profile_comments)),
                Message::ProfileTab,
            ));
        }
        if !self.viewing_saved_comments() && !communities && !people && !reference {
            controls = controls.push(widget::dropdown(
                Sort::LABELS.as_slice(),
                Some(self.sort.index()),
                Message::Sort,
            ));
        }
        controls = controls.push(
            button::icon(icon::from_name("view-refresh-symbolic"))
                .tooltip("Refresh feed")
                .on_press_maybe((!self.busy && self.api.is_some()).then_some(Message::Refresh)),
        );
        if let Scope::Community(id) = self.scope {
            controls = controls.push(
                button::icon(icon::from_name("dialog-information-symbolic"))
                    .tooltip("Community details")
                    .on_press(Message::CommunityDetails(id)),
            );
        }
        let mut content =
            widget::column::with_capacity(self.posts.len() + 3).spacing(space.space_xxs);
        if let Some(person) = self.profile() {
            let identity = widget::row::with_capacity(2)
                .spacing(space.space_xxs)
                .align_y(Alignment::Center)
                .push(widget::text::body(person_handle(person)).wrapping(Wrapping::WordOrGlyph))
                .push(
                    button::icon(icon::from_name("web-browser-symbolic"))
                        .tooltip("Open profile in browser")
                        .on_press(Message::OpenLink(person.actor_id.to_string())),
                );
            content = content.push(identity);
            if person.bio.as_ref().is_some_and(|bio| !bio.is_empty()) {
                content = content.push(render_markdown(&self.profile_bio, Message::OpenLink));
            }
            content = content.push(widget::divider::horizontal::light());
        }
        if let Some(error) = &self.open_error {
            content = content.push(widget::text::body(error));
        }
        let empty = self.posts.is_empty()
            && self.comments.is_empty()
            && self.communities.is_empty()
            && self.people.is_empty();
        if let Some(error) = &self.error {
            content = content.push(widget::text::body(error));
            if empty && !self.has_more() {
                content = content.push(button::standard("Retry").on_press(Message::Refresh));
                if reference {
                    content = content
                        .push(button::text("Search as keywords").on_press(Message::SearchKeywords));
                }
            }
        }
        if empty && self.error.is_none() {
            let message = if self.busy {
                if reference {
                    "Opening link or handle…"
                } else if comments {
                    "Loading comments…"
                } else if communities {
                    "Loading communities…"
                } else if people {
                    "Loading people…"
                } else {
                    "Loading posts…"
                }
            } else if self.loaded {
                match self.scope {
                    Scope::Saved if comments => "No saved comments.",
                    Scope::Saved => "No saved posts.",
                    Scope::Search if comments => "No matching comments.",
                    Scope::Search if communities => "No matching communities.",
                    Scope::Search if people => "No matching people.",
                    Scope::Profile(_) if comments => "No comments on this profile.",
                    Scope::Profile(_) => "No posts on this profile.",
                    Scope::Search => "No matching posts.",
                    _ => "No posts in this feed.",
                }
            } else {
                "Connect to an instance to start reading."
            };
            content = content
                .push(widget::container(widget::text::body(message)).padding(space.space_xs));
        }
        for post in &self.posts {
            let metadata = widget::row::with_capacity(3)
                .spacing(space.space_xxxs)
                .align_y(Alignment::Center)
                .push(icon::from_name("system-users-symbolic").size(16).icon())
                .push(
                    widget::text::caption(format!("!{}", community_handle(&post.community)))
                        .width(Length::Fill)
                        .wrapping(Wrapping::WordOrGlyph),
                )
                .push(widget::text::caption(age(post.post.published.timestamp())));
            let stats = widget::row::with_capacity(4)
                .align_y(Alignment::Center)
                .spacing(space.space_xxs)
                .push(icon::from_name("go-up-symbolic").size(12).icon())
                .push(widget::text::caption(post.counts.score.to_string()))
                .push(icon::from_name("chat-symbolic").size(12).icon())
                .push(widget::text::caption(post.counts.comments.to_string()));
            let card = widget::column::with_capacity(3)
                .spacing(space.space_xxxs)
                .push(metadata)
                .push(widget::text::body(&post.post.name).font(cosmic::font::semibold()))
                .push(stats)
                .width(Length::Fill);
            content = content.push(selectable_card(
                card,
                if compact {
                    space.space_xxs
                } else {
                    space.space_xs
                },
                selected == Some(post.post.id),
                Some(Message::Open(post.post.id.0)),
            ));
        }
        for comment in &self.comments {
            let label = if comment.comment.deleted {
                "[deleted]"
            } else if comment.comment.removed {
                "[removed]"
            } else {
                &comment.comment.content
            };
            let card = widget::column::with_capacity(3)
                .spacing(space.space_xxxs)
                .push(
                    widget::text::caption(format!(
                        "{} · !{}",
                        comment.creator.name,
                        community_handle(&comment.community)
                    ))
                    .wrapping(Wrapping::WordOrGlyph),
                )
                .push(widget::text::body(&comment.post.name).font(cosmic::font::semibold()))
                .push(widget::text::body(label).wrapping(Wrapping::WordOrGlyph))
                .width(Length::Fill);
            content = content.push(selectable_card(
                card,
                if compact {
                    space.space_xxs
                } else {
                    space.space_xs
                },
                self.opening_comment == Some(comment.comment.id),
                self.opening_comment
                    .is_none()
                    .then_some(Message::OpenComment(comment.comment.id)),
            ));
        }
        for result in &self.communities {
            let community = &result.community;
            let card = widget::column::with_capacity(2)
                .spacing(space.space_xxxs)
                .push(widget::text::body(&community.title).font(cosmic::font::semibold()))
                .push(widget::text::caption(format!(
                    "!{}",
                    community_handle(community)
                )))
                .width(Length::Fill);
            content = content.push(selectable_card(
                card,
                if compact {
                    space.space_xxs
                } else {
                    space.space_xs
                },
                false,
                Some(Message::OpenCommunity(community.id)),
            ));
        }
        for result in &self.people {
            let person = &result.person;
            let card = widget::column::with_capacity(2)
                .spacing(space.space_xxxs)
                .push(
                    widget::text::body(person.display_name.as_deref().unwrap_or(&person.name))
                        .font(cosmic::font::semibold())
                        .wrapping(Wrapping::WordOrGlyph),
                )
                .push(widget::text::caption(person_handle(person)).wrapping(Wrapping::WordOrGlyph))
                .width(Length::Fill);
            content = content.push(selectable_card(
                card,
                if compact {
                    space.space_xxs
                } else {
                    space.space_xs
                },
                false,
                Some(Message::OpenPerson(person.id)),
            ));
        }
        if self.has_more() {
            content = content.push(
                button::standard(if self.busy {
                    "Loading…"
                } else if comments {
                    "Load more comments"
                } else if communities {
                    "Load more communities"
                } else if people {
                    "Load more people"
                } else {
                    "Load more posts"
                })
                .on_press_maybe((!self.busy).then_some(Message::More)),
            );
        }
        let mut layout = widget::column::with_capacity(3)
            .spacing(space.space_xs)
            .push(controls);
        if searching {
            let mut filters =
                widget::row::with_capacity(2)
                    .spacing(space.space_xxs)
                    .push(widget::dropdown(
                        &["Posts", "Comments", "Communities", "People"][..],
                        Some(self.search_type.index()),
                        Message::SearchKind,
                    ));
            if self.search_type.supports_community() && !self.search_communities.is_empty() {
                let mut labels = vec!["All communities".to_owned()];
                labels.extend(
                    self.search_communities
                        .iter()
                        .map(|community| format!("!{}", community_handle(community))),
                );
                let selected = self
                    .search_communities
                    .iter()
                    .position(|community| Some(community.id) == self.search_community)
                    .map_or(0, |index| index + 1);
                filters = filters.push(widget::dropdown(
                    labels,
                    Some(selected),
                    Message::SearchCommunity,
                ));
            }
            layout = layout.push(filters.wrap());
        }
        layout
            .push(
                widget::scrollable(widget::container(content).padding([0, space.space_s, 0, 0]))
                    .id(SCROLL_ID)
                    .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
                    .height(Length::Fill),
            )
            .height(Length::Fill)
            .into()
    }
}
