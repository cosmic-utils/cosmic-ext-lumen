use super::{Feed, Message, Scope, SearchKind, Sort};
use crate::fl;
use crate::{
    features::post::render_markdown,
    shared::{
        media::{self, Media},
        presentation::{
            age, community_handle, community_label, person_handle, selectable_card,
            toggle_button_class,
        },
    },
};
use cosmic::{
    Element,
    iced::{
        Alignment, Length,
        core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping},
    },
    widget::{self, button, icon},
};
use lemmy_api_common::lemmy_db_schema::{newtypes::PostId, source::community::Community};

pub const SCROLL_ID: &str = "lumen-feed";

impl Feed {
    pub fn view<'a>(
        &'a self,
        title: String,
        community: Option<&'a Community>,
        selected: Option<PostId>,
        compact: bool,
        media: &'a Media,
    ) -> Element<'a, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let comments = self.viewing_comments();
        let reference = self.viewing_reference();
        let searching = self.scope == Scope::Search && !reference;
        let communities = searching && self.search_type == SearchKind::Communities;
        let people = searching && self.search_type == SearchKind::People;
        let header = self.header(title, community, media, compact);
        let mut content =
            widget::column::with_capacity(self.posts.len() + 3).spacing(space.space_xxs);
        if self
            .profile()
            .and_then(|person| person.bio.as_deref())
            .is_some_and(|bio| !bio.is_empty())
        {
            content = content
                .push(render_markdown(&self.profile_bio, Message::OpenLink))
                .push(widget::divider::horizontal::light());
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
                content = content.push(button::standard(fl!("retry")).on_press(Message::Refresh));
                if reference {
                    content = content.push(
                        button::text(fl!("search-as-keywords")).on_press(Message::SearchKeywords),
                    );
                }
            }
        }
        if empty && self.error.is_none() {
            let message = if self.busy {
                if reference {
                    fl!("link-opening")
                } else if comments {
                    fl!("loading-comments")
                } else if communities {
                    fl!("loading-communities")
                } else if people {
                    fl!("loading-people")
                } else {
                    fl!("loading-posts")
                }
            } else if self.loaded {
                match self.scope {
                    Scope::Saved if comments => fl!("saved-no-comments"),
                    Scope::Saved => fl!("saved-no-posts"),
                    Scope::Search if comments => fl!("search-no-comments"),
                    Scope::Search if communities => fl!("search-no-communities"),
                    Scope::Search if people => fl!("search-no-people"),
                    Scope::Profile(_) if comments => fl!("profile-no-comments"),
                    Scope::Profile(_) => fl!("profile-no-posts"),
                    Scope::Search => fl!("search-no-posts"),
                    _ => fl!("feed-empty"),
                }
            } else {
                fl!("instance-connect-prompt")
            };
            content = content
                .push(widget::container(widget::text::body(message)).padding(space.space_xs));
        }
        for post in &self.posts {
            let metadata = widget::row::with_capacity(3)
                .spacing(space.space_xxxs)
                .align_y(Alignment::Center)
                .push(media.community_avatar(&post.community).icon().size(16))
                .push(
                    widget::text::caption(community_label(&post.community))
                        .width(Length::Fill)
                        .wrapping(Wrapping::None)
                        .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
                )
                .push(widget::text::caption(age(post.post.published.timestamp())));
            let stats = widget::row::with_capacity(4)
                .align_y(Alignment::Center)
                .spacing(space.space_xxs)
                .push(icon::from_name("go-up-symbolic").size(12).icon())
                .push(widget::text::caption(post.counts.score.to_string()))
                .push(icon::from_name("chat-symbolic").size(12).icon())
                .push(widget::text::caption(post.counts.comments.to_string()));
            let text = widget::column::with_capacity(2)
                .spacing(space.space_xxxs)
                .push(
                    widget::text::body(&post.post.name)
                        .font(cosmic::font::semibold())
                        .wrapping(Wrapping::WordOrGlyph),
                )
                .push(stats)
                .width(Length::Fill);
            let sources = media::feed_thumbnail_sources(post);
            let picture = media.feed_picture(&sources);
            let mut body = widget::row::with_capacity(2)
                .spacing(space.space_xs)
                .align_y(Alignment::Start)
                .push(text);
            if !sources.is_empty() && (media.automatic() || picture.is_some()) {
                let side = Length::Fixed(if compact { 44.0 } else { 64.0 });
                let thumbnail: Element<'_, Message> = if let Some(picture) = picture {
                    media::image_view(picture, side, side).into()
                } else {
                    widget::space().width(side).height(side).into()
                };
                body = body.push(thumbnail);
            }
            let card = widget::column::with_capacity(2)
                .spacing(space.space_xxxs)
                .push(metadata)
                .push(body)
                .width(Length::Fill);
            content = content.push(selectable_card(
                card,
                if compact {
                    space.space_xxs
                } else {
                    space.space_xs
                },
                selected == Some(post.post.id),
                self.api.is_some().then_some(Message::Open(post.post.id.0)),
            ));
        }
        for comment in &self.comments {
            let label = if comment.comment.deleted {
                fl!("comment-deleted").into()
            } else if comment.comment.removed {
                fl!("comment-removed").into()
            } else {
                std::borrow::Cow::Borrowed(comment.comment.content.as_str())
            };
            let card = widget::column::with_capacity(3)
                .spacing(space.space_xxxs)
                .push(
                    widget::row::with_capacity(2)
                        .align_y(Alignment::Center)
                        .spacing(space.space_xxxs)
                        .push(media.person_avatar(&comment.creator).icon().size(16))
                        .push(
                            widget::text::caption(fl!(
                                "feed-comment-metadata",
                                author = comment.creator.name.as_str(),
                                community = community_label(&comment.community)
                            ))
                            .wrapping(Wrapping::WordOrGlyph),
                        ),
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
            let label = widget::text::body(community_label(community))
                .font(cosmic::font::semibold())
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill);
            let card = widget::row::with_capacity(2)
                .align_y(Alignment::Center)
                .spacing(space.space_xs)
                .push(media.community_avatar(community).icon().size(24))
                .push(label);
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
            let card = widget::row::with_capacity(2)
                .align_y(Alignment::Center)
                .spacing(space.space_xs)
                .push(media.person_avatar(person).icon().size(24))
                .push(card);
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
                    fl!("loading")
                } else if comments {
                    fl!("load-more-comments")
                } else if communities {
                    fl!("load-more-communities")
                } else if people {
                    fl!("load-more-people")
                } else {
                    fl!("load-more-posts")
                })
                .on_press_maybe((!self.busy).then_some(Message::More)),
            );
        }
        let mut layout = widget::column::with_capacity(3)
            .spacing(space.space_xs)
            .push(header);
        if searching {
            let mut filters =
                widget::row::with_capacity(2)
                    .spacing(space.space_xxs)
                    .push(widget::dropdown(
                        vec![
                            fl!("posts"),
                            fl!("comments"),
                            fl!("communities"),
                            fl!("people"),
                        ],
                        Some(self.search_type.index()),
                        Message::SearchKind,
                    ));
            if self.search_type.supports_community() && !self.search_communities.is_empty() {
                let mut labels = vec![fl!("all-communities")];
                labels.extend(self.search_communities.iter().map(community_label));
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
    fn header(
        &self,
        title: String,
        community: Option<&Community>,
        media: &Media,
        compact: bool,
    ) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let profile = matches!(self.scope, Scope::Profile(_));
        let identity_page = profile || matches!(self.scope, Scope::Community(_));
        let title = widget::text::title3(title).width(Length::Fill);
        let title = if identity_page {
            title
                .wrapping(Wrapping::None)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
        } else {
            title.wrapping(Wrapping::WordOrGlyph)
        };
        let mut labels = widget::column::with_capacity(2)
            .width(Length::Fill)
            .push(title);
        let (picture, handle) = community
            .map(|community| {
                (
                    media.community_avatar(community),
                    community_handle(community),
                )
            })
            .or_else(|| {
                self.profile()
                    .map(|person| (media.person_avatar(person), person_handle(person)))
            })
            .unzip();
        if let Some(handle) = handle {
            labels = labels.push(
                widget::text::caption(handle)
                    .wrapping(Wrapping::None)
                    .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
            );
        }
        let heading = widget::row::with_capacity(2)
            .spacing(space.space_xxs)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .push_maybe(picture.map(|picture| picture.icon().size(28)))
            .push(labels);
        let mut dropdowns = widget::row::with_capacity(2)
            .spacing(space.space_xxs)
            .align_y(Alignment::Center);
        let reference = self.viewing_reference();
        let show_sort = !self.viewing_saved_comments()
            && !reference
            && (self.scope != Scope::Search || self.search_type.supports_community());
        if self.scope == Scope::Saved {
            dropdowns = dropdowns.push(widget::dropdown(
                vec![fl!("posts"), fl!("comments")],
                Some(usize::from(self.viewing_saved_comments())),
                Message::SavedTab,
            ));
        }
        if profile {
            dropdowns = dropdowns.push(widget::dropdown(
                vec![fl!("posts"), fl!("comments")],
                Some(usize::from(self.profile_comments)),
                Message::ProfileTab,
            ));
        }
        if show_sort {
            dropdowns = dropdowns.push(widget::dropdown(
                Sort::labels(),
                Some(self.sort.index()),
                Message::Sort,
            ));
        }
        let mut controls = widget::row::with_capacity(3)
            .spacing(space.space_xxs)
            .align_y(Alignment::Center);
        controls = controls.push(
            button::icon(icon::from_name("view-refresh-symbolic"))
                .tooltip(fl!("refresh-feed"))
                .on_press_maybe((!self.busy && self.api.is_some()).then_some(Message::Refresh)),
        );
        controls = controls.push(
            button::icon(icon::from_name("view-list-symbolic"))
                .tooltip(if compact {
                    fl!("feed-spacing-comfortable")
                } else {
                    fl!("feed-spacing-compact")
                })
                .class(toggle_button_class(compact, button::ButtonClass::Icon))
                .on_press(Message::ToggleDensity),
        );
        if let Scope::Community(id) = self.scope {
            controls = controls.push(
                button::icon(icon::from_name("dialog-information-symbolic"))
                    .tooltip(fl!("community-details"))
                    .on_press(Message::CommunityDetails(id)),
            );
        }
        if let Some(person) = self.profile() {
            controls = controls.push(
                button::icon(icon::from_name("web-browser-symbolic"))
                    .tooltip(fl!("profile-open-browser"))
                    .on_press(Message::OpenLink(person.actor_id.to_string())),
            );
        }
        let header = widget::row::with_capacity(3)
            .spacing(space.space_xxs)
            .align_y(Alignment::Center)
            .push(heading);
        if identity_page {
            widget::column::with_capacity(2)
                .spacing(space.space_xxs)
                .push(header.push(controls))
                .push(dropdowns.wrap())
                .into()
        } else {
            header
                .push_maybe((show_sort || self.scope == Scope::Saved).then_some(dropdowns))
                .push(controls)
                .into()
        }
    }
}
