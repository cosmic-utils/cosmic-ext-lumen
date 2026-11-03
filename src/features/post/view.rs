use super::{Action, Comment, CommentTarget, Composer, Message, Post, thread};
use crate::shared::{
    api::Api,
    presentation::{age, community_handle, text_editor, toggle_button_class},
};
use cosmic::{
    Element,
    iced::{Alignment, Length, Padding},
    theme::Container,
    widget::{self, button, icon, markdown},
};

pub(crate) fn render_markdown<Message: Clone + 'static>(
    content: &markdown::Content,
    on_link: impl Fn(String) -> Message + 'static,
) -> Element<'_, Message> {
    let theme = cosmic::theme::active();
    let color = theme.cosmic().accent_color();
    let settings = markdown::Settings::with_style(markdown::Style::from_palette(
        cosmic::iced::theme::Palette {
            primary: cosmic::iced::Color::from_rgb(color.red, color.green, color.blue),
            ..if theme.theme_type.is_dark() {
                cosmic::iced::Theme::Dark
            } else {
                cosmic::iced::Theme::Light
            }
            .palette()
        },
    ));
    markdown::view(content.items(), settings).map(on_link)
}

impl Post {
    fn comment_composer<'a>(
        &self,
        composer: &'a Composer,
        target: CommentTarget,
        account_busy: bool,
    ) -> Element<'a, Message> {
        let generation = self.generation;
        let submitting = self.submitting_target() == Some(target);
        let (placeholder, label) = match target {
            CommentTarget::Post => ("Write a comment", "Post comment"),
            CommentTarget::Reply(_) => ("Write a reply", "Post reply"),
            CommentTarget::Edit(_) => ("Edit comment", "Save changes"),
        };
        let mut editor = text_editor(&composer.draft, placeholder).height(Length::Fixed(100.0));
        if !submitting {
            editor =
                editor.on_action(move |action| Message::EditComment(generation, target, action));
        }
        let can_submit = !account_busy
            && self.submitting.is_none()
            && self.pending_action.is_none()
            && match target {
                CommentTarget::Edit(_) => self.comment_action.is_none(),
                CommentTarget::Reply(id) => self
                    .comment_action
                    .as_ref()
                    .is_none_or(|pending| pending.comment != id),
                CommentTarget::Post => true,
            }
            && !composer.draft.text().trim().is_empty();
        let label = match (submitting, target) {
            (true, CommentTarget::Edit(_)) => "Saving…",
            (true, _) => "Posting…",
            (false, _) => label,
        };
        let cancel = match target {
            CommentTarget::Post => Message::ToggleComposer,
            CommentTarget::Reply(id) => Message::CancelReply(generation, id),
            CommentTarget::Edit(id) => Message::CancelEdit(generation, id),
        };
        let buttons =
            widget::row::with_capacity(2)
                .spacing(8)
                .push(widget::button::suggested(label).on_press_maybe(
                    can_submit.then_some(Message::SubmitComment(generation, target)),
                ))
                .push(
                    widget::button::standard("Cancel")
                        .on_press_maybe(self.submitting.is_none().then_some(cancel)),
                );
        let mut content = widget::column::with_capacity(3)
            .spacing(8)
            .push(editor)
            .push(buttons);
        if let Some(error) = &composer.error {
            content = content.push(widget::text::body(error));
        }
        content.into()
    }

    pub fn view(
        &self,
        account_busy: bool,
        show_back: bool,
        can_focus: bool,
        focused: bool,
        narrow: bool,
    ) -> Element<'_, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let Some(post) = &self.selected else {
            return widget::container(widget::text::body("Select a post to open its discussion."))
                .padding(space.space_m)
                .into();
        };
        let signed_in = self.api.as_ref().is_some_and(Api::is_authenticated);
        let can_write = signed_in && !post.post.locked && !post.post.removed && !post.post.deleted;
        let can_act = signed_in
            && !account_busy
            && self.pending_action.is_none()
            && self.submitting.is_none();
        let mut toolbar = widget::row::with_capacity(6)
            .align_y(Alignment::Center)
            .spacing(space.space_xxxs);
        if show_back {
            toolbar = toolbar.push(
                button::icon(icon::from_name("go-previous-symbolic"))
                    .tooltip("Back to posts")
                    .on_press(Message::Back),
            );
        }
        toolbar = toolbar
            .push(widget::text::body("Discussion").font(cosmic::font::semibold()))
            .push(widget::space().width(Length::Fill));
        if can_focus {
            toolbar = toolbar.push(
                button::icon(icon::from_name(if focused {
                    "view-restore-symbolic"
                } else {
                    "view-fullscreen-symbolic"
                }))
                .tooltip(if focused {
                    "Show feed beside discussion"
                } else {
                    "Focus reading"
                })
                .class(toggle_button_class(focused, button::ButtonClass::Icon))
                .on_press(Message::ToggleFocus),
            );
        }
        toolbar = toolbar.push(
            button::icon(icon::from_name("dialog-information-symbolic"))
                .tooltip("Community details")
                .on_press(Message::CommunityDetails),
        );
        if let Some(url) = self
            .api
            .as_ref()
            .and_then(|api| api.base.join(&format!("post/{}", post.post.id.0)).ok())
        {
            toolbar = toolbar.push(
                button::icon(icon::from_name("web-browser-symbolic"))
                    .tooltip("Open discussion in browser")
                    .on_press(Message::Link(url.to_string())),
            );
        }

        let metadata = widget::row::with_capacity(3)
            .align_y(Alignment::Center)
            .spacing(space.space_xxxs)
            .push(
                button::text(format!("!{}", community_handle(&post.community)))
                    .padding([0, space.space_xxxs])
                    .on_press(Message::OpenCommunity(post.community.id)),
            )
            .push(widget::text::caption(format!(
                "{} · {}",
                post.creator.name,
                age(post.post.published.timestamp())
            )))
            .wrap();
        let mut content = widget::column::with_capacity(self.comments.len() + 10)
            .spacing(space.space_xs)
            .push(metadata)
            .push(if narrow {
                widget::text::title2(&post.post.name)
            } else {
                widget::text::title1(&post.post.name)
            });
        if let Some(id) = self.focused_comment {
            if let Some(comment) = self
                .comments
                .iter()
                .find(|comment| comment.data.comment.id == id)
            {
                content = content
                    .push(widget::text::caption("Comment in context"))
                    .push(self.comment_card(comment, can_write, signed_in, account_busy, 0));
            } else if let Some(error) = &self.focus_error {
                content = content.push(widget::text::body(error));
            } else {
                content = content.push(widget::text::caption("Loading comment…"));
            }
        }
        if let Some(url) = &post.post.url {
            content = content
                .push(button::link(url.to_string()).on_press(Message::Link(url.to_string())));
        }
        if post.post.body.as_ref().is_some_and(|body| !body.is_empty()) {
            content = content.push(render_markdown(&self.body, Message::Link));
        }

        let mut actions = widget::row::with_capacity(2)
            .spacing(space.space_xxs)
            .align_y(Alignment::Center)
            .push(votes(
                post.counts.score,
                post.my_vote,
                can_act.then_some((
                    Message::Act(self.generation, Action::Upvote),
                    Message::Act(self.generation, Action::Downvote),
                )),
            ));
        if signed_in {
            actions = actions.push(
                button::standard(if post.saved { "Saved" } else { "Save" })
                    .class(toggle_button_class(
                        post.saved,
                        button::ButtonClass::Standard,
                    ))
                    .leading_icon(icon::from_name("bookmark-new-symbolic"))
                    .on_press_maybe(
                        can_act.then_some(Message::Act(self.generation, Action::ToggleSaved)),
                    ),
            );
        }
        content = content.push(actions);
        if let Some(error) = &self.action_error {
            content = content.push(widget::text::body(error));
        }
        if let Some(error) = &self.link_error {
            content = content.push(widget::text::body(error));
        }
        if self.pending_action.is_some() {
            content = content.push(widget::text::caption("Updating post…"));
        }

        let comment_heading = widget::row::with_capacity(2)
            .spacing(space.space_xxs)
            .align_y(Alignment::Center)
            .push(widget::text::title3("Comments"))
            .push(widget::text::caption(post.counts.comments.to_string()));
        let mut comment_controls = widget::row::with_capacity(2)
            .spacing(space.space_xxxs)
            .align_y(Alignment::Center)
            .push(widget::dropdown(
                &["Top", "New", "Old"][..],
                Some(self.sort),
                Message::Sort,
            ));
        if can_write {
            comment_controls = comment_controls.push(
                button::suggested(if self.composer_open {
                    "Close"
                } else {
                    "Add comment"
                })
                .padding([0, space.space_xxs])
                .on_press_maybe(
                    (!account_busy && self.submitting.is_none()).then_some(Message::ToggleComposer),
                ),
            );
        }
        let comment_header: Element<'_, Message> = if narrow {
            widget::column::with_capacity(2)
                .spacing(space.space_xxs)
                .push(comment_heading)
                .push(comment_controls)
                .into()
        } else {
            widget::row::with_capacity(3)
                .align_y(Alignment::Center)
                .push(comment_heading)
                .push(widget::space().width(Length::Fill))
                .push(comment_controls)
                .into()
        };
        content = content
            .push(widget::divider::horizontal::light())
            .push(comment_header);
        if post.post.locked {
            content = content.push(widget::text::caption("This discussion is locked."));
        }
        if self.composer_open && can_write {
            content = content.push(self.comment_composer(
                &self.composer,
                CommentTarget::Post,
                account_busy,
            ));
        }

        if let Some(error) = &self.error {
            content = content
                .push(widget::text::body(error))
                .push(button::standard("Retry comments").on_press(Message::Retry));
        }
        if self.busy && self.comments.is_empty() {
            content = content.push(widget::text::body("Loading comments…"));
        } else if self.comments.is_empty() && self.error.is_none() {
            content = content.push(widget::text::body("No comments yet."));
        }
        for comment in &self.comments {
            if Some(comment.data.comment.id) != self.focused_comment
                && !thread::hidden(&comment.path, &self.collapsed)
            {
                content = content.push(self.comment_card(
                    comment,
                    can_write,
                    signed_in,
                    account_busy,
                    comment.path.len().saturating_sub(1).min(4),
                ));
            }
        }
        if self.busy && !self.comments.is_empty() {
            content = content.push(widget::text::caption("Loading comments…"));
        }
        if self.more && !self.busy && self.error.is_none() {
            content = content.push(button::standard("Load more comments").on_press(Message::More));
        }
        let article = widget::container(content)
            .padding([
                space.space_s,
                if narrow { space.space_s } else { space.space_m },
            ])
            .max_width(760)
            .width(Length::Fill);
        widget::column::with_capacity(3)
            .push(widget::container(toolbar).padding([space.space_xxxs, space.space_xs]))
            .push(widget::divider::horizontal::light())
            .push(
                widget::scrollable(widget::container(article).center_x(Length::Fill))
                    .id(super::SCROLL_ID)
                    .height(Length::Fill),
            )
            .height(Length::Fill)
            .into()
    }

    fn comment_card<'a>(
        &'a self,
        comment: &'a Comment,
        can_write: bool,
        signed_in: bool,
        account_busy: bool,
        depth: usize,
    ) -> Element<'a, Message> {
        let space = cosmic::theme::active().cosmic().spacing;
        let id = comment.data.comment.id;
        let collapsed = self.collapsed.contains(&id.0);
        let header = widget::row::with_capacity(5)
            .align_y(Alignment::Center)
            .spacing(space.space_xxs)
            .push(icon::from_name("avatar-default-symbolic").size(16).icon())
            .push(widget::text::caption(&comment.data.creator.name).font(cosmic::font::semibold()))
            .push(widget::text::caption(age(comment
                .data
                .comment
                .published
                .timestamp())))
            .push(widget::space().width(Length::Fill))
            .push(
                button::icon(icon::from_name(if collapsed {
                    "list-add-symbolic"
                } else {
                    "list-remove-symbolic"
                }))
                .padding(space.space_xxxs)
                .tooltip(if collapsed {
                    "Expand comment"
                } else {
                    "Collapse comment"
                })
                .on_press(Message::Collapse(id.0)),
            );
        let mut row = widget::column::with_capacity(4)
            .spacing(space.space_xxxs)
            .push(header);
        if !collapsed {
            row = row.push(render_markdown(&comment.body, Message::Link));
            let can_act = signed_in
                && !account_busy
                && self.comment_action.is_none()
                && self.submitting_target() != Some(CommentTarget::Edit(id));
            let can_vote =
                can_act && !comment.data.comment.deleted && !comment.data.comment.removed;
            let mut actions = widget::row::with_capacity(2)
                .spacing(space.space_xxxs)
                .align_y(Alignment::Center)
                .push(votes(
                    comment.data.counts.score,
                    comment.data.my_vote,
                    can_vote.then_some((
                        Message::Vote(self.generation, id, 1),
                        Message::Vote(self.generation, id, -1),
                    )),
                ));
            if signed_in {
                let saved = comment.data.saved;
                actions = actions.push(
                    button::icon(icon::from_name("user-bookmarks-symbolic"))
                        .class(toggle_button_class(saved, button::ButtonClass::Icon))
                        .tooltip(if saved {
                            "Unsave comment"
                        } else {
                            "Save comment"
                        })
                        .on_press_maybe(
                            (can_vote || (saved && can_act))
                                .then_some(Message::SaveComment(self.generation, id)),
                        ),
                );
            }
            if can_write && !comment.data.comment.deleted && !comment.data.comment.removed {
                actions = actions.push(
                    button::text("Reply")
                        .leading_icon(icon::from_name("mail-reply-all-symbolic"))
                        .padding([0, space.space_xxs])
                        .on_press_maybe(
                            (!account_busy && self.submitting.is_none())
                                .then_some(Message::OpenReply(self.generation, id)),
                        ),
                );
            }
            if signed_in && self.owns_comment(comment) {
                let can_change =
                    !account_busy && self.comment_action.is_none() && self.submitting.is_none();
                let deleted = comment.data.comment.deleted;
                if !deleted {
                    actions = actions.push(
                        button::icon(icon::from_name("document-edit-symbolic"))
                            .tooltip("Edit comment")
                            .on_press_maybe(
                                (can_change && self.edit.is_none())
                                    .then_some(Message::OpenEdit(self.generation, id)),
                            ),
                    );
                }
                actions = actions.push(
                    button::icon(icon::from_name(if deleted {
                        "edit-undo-symbolic"
                    } else {
                        "edit-delete-symbolic"
                    }))
                    .tooltip(if deleted {
                        "Restore comment"
                    } else {
                        "Delete comment"
                    })
                    .on_press_maybe(
                        can_change.then_some(Message::DeleteComment(self.generation, id)),
                    ),
                );
            }
            if comment.parent().is_some() {
                actions = actions.push(
                    button::icon(icon::from_name("go-top-symbolic"))
                        .tooltip("Jump to parent comment")
                        .on_press(Message::JumpToParent(self.generation, id)),
                );
            }
            actions = actions.push(
                button::icon(icon::from_name("edit-copy-symbolic"))
                    .tooltip("Copy comment permalink")
                    .on_press(Message::CopyPermalink(
                        comment.data.comment.ap_id.to_string(),
                    )),
            );
            row = row.push(actions.wrap());
            if let Some((_, error)) = self
                .comment_action_error
                .as_ref()
                .filter(|(voted, _)| *voted == id)
            {
                row = row.push(widget::text::caption(error));
            }
            if let Some((_, composer)) = self.edit.as_ref().filter(|(editing, _)| *editing == id)
                && self.owns_comment(comment)
                && !comment.data.comment.deleted
            {
                row = row.push(self.comment_composer(
                    composer,
                    CommentTarget::Edit(id),
                    account_busy,
                ));
            }
            if can_write
                && !comment.data.comment.deleted
                && !comment.data.comment.removed
                && self.active_reply == Some(id)
                && let Some(composer) = self.replies.get(&id)
            {
                row = row.push(self.comment_composer(
                    composer,
                    CommentTarget::Reply(id),
                    account_busy,
                ));
            }
        }
        let card = widget::container(row)
            .class(Container::Card)
            .padding([space.space_xxs, space.space_xs])
            .width(Length::Fill);
        if depth == 0 {
            card.into()
        } else {
            widget::container(
                widget::row::with_capacity(2)
                    .spacing(space.space_xxs)
                    .push(
                        cosmic::iced::widget::rule::vertical(2)
                            .class(cosmic::theme::Rule::LightDivider),
                    )
                    .push(card),
            )
            .padding(Padding {
                left: depth as f32 * f32::from(space.space_xs),
                ..Padding::ZERO
            })
            .into()
        }
    }
}

fn votes(
    score: i64,
    current: Option<i16>,
    actions: Option<(Message, Message)>,
) -> Element<'static, Message> {
    let (up, down) = actions.unzip();
    widget::row::with_capacity(3)
        .spacing(4)
        .align_y(Alignment::Center)
        .push(
            button::icon(icon::from_name("go-up-symbolic"))
                .padding(4)
                .tooltip(if current == Some(1) {
                    "Clear upvote"
                } else {
                    "Upvote"
                })
                .class(toggle_button_class(
                    current == Some(1),
                    button::ButtonClass::Icon,
                ))
                .on_press_maybe(up),
        )
        .push(widget::text::caption(score.to_string()).font(cosmic::font::semibold()))
        .push(
            button::icon(icon::from_name("go-down-symbolic"))
                .padding(4)
                .tooltip(if current == Some(-1) {
                    "Clear downvote"
                } else {
                    "Downvote"
                })
                .class(toggle_button_class(
                    current == Some(-1),
                    button::ButtonClass::Icon,
                ))
                .on_press_maybe(down),
        )
        .into()
}
