use super::{Feed, Message, Scope, Sort};
use crate::shared::presentation::{age, community_handle, selectable_card};
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
            )
            .push(widget::dropdown(
                Sort::LABELS.as_slice(),
                Some(self.sort.index()),
                Message::Sort,
            ))
            .push(
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
        if let Some(error) = &self.error {
            content = content.push(widget::text::body(error));
            if self.posts.is_empty() && !self.has_more() {
                content = content.push(button::standard("Retry").on_press(Message::Refresh));
            }
        }
        if self.posts.is_empty() && self.error.is_none() {
            let message = if self.busy {
                "Loading posts…"
            } else if self.loaded {
                match self.scope {
                    Scope::Saved => "No saved posts.",
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
        if self.has_more() {
            content = content.push(
                button::standard(if self.busy {
                    "Loading…"
                } else {
                    "Load more posts"
                })
                .on_press_maybe((!self.busy).then_some(Message::More)),
            );
        }
        widget::column::with_capacity(2)
            .spacing(space.space_xs)
            .push(controls)
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
