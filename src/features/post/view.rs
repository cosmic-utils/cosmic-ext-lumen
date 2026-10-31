use super::{Action, CommentTarget, Composer, Message, Post, thread};
use crate::shared::api::Api;
use cosmic::{
    Element,
    iced::{Length, Padding},
    theme::Container,
    widget::{self, markdown},
};

fn render_markdown(content: &markdown::Content) -> Element<'_, Message> {
    let theme = cosmic::theme::active();
    let color = theme.cosmic().accent_color();
    let settings = markdown::Settings::with_style(markdown::Style::from_palette(
        cosmic::iced::theme::Palette {
            primary: cosmic::iced::Color::from_rgb(color.red, color.green, color.blue),
            ..cosmic::iced::Theme::Dark.palette()
        },
    ));
    markdown::view(content.items(), settings).map(Message::Link)
}

impl Post {
    fn comment_composer<'a>(
        &self,
        composer: &'a Composer,
        target: CommentTarget,
        account_busy: bool,
    ) -> Element<'a, Message> {
        let generation = self.generation;
        let submitting = self.submitting == Some(target);
        let (placeholder, label) = match target {
            CommentTarget::Post => ("Write a comment", "Post comment"),
            CommentTarget::Reply(_) => ("Write a reply", "Post reply"),
        };
        let mut editor = widget::text_editor::TextEditor::new(&composer.draft)
            .placeholder(placeholder)
            .height(Length::Fixed(100.0));
        if !submitting {
            editor =
                editor.on_action(move |action| Message::EditComment(generation, target, action));
        }
        let can_submit = !account_busy
            && self.submitting.is_none()
            && self.pending_action.is_none()
            && !composer.draft.text().trim().is_empty();
        let mut buttons = widget::row::with_capacity(2).spacing(8).push(
            widget::button::suggested(if submitting { "Posting…" } else { label })
                .on_press_maybe(can_submit.then_some(Message::SubmitComment(generation, target))),
        );
        if let CommentTarget::Reply(id) = target {
            buttons = buttons.push(
                widget::button::standard("Cancel").on_press_maybe(
                    self.submitting
                        .is_none()
                        .then_some(Message::CancelReply(generation, id)),
                ),
            );
        }
        let mut content = widget::column::with_capacity(3)
            .spacing(8)
            .push(editor)
            .push(buttons);
        if let Some(error) = &composer.error {
            content = content.push(widget::text::body(error));
        }
        content.into()
    }

    pub fn view(&self, account_busy: bool) -> Element<'_, Message> {
        let Some(post) = &self.selected else {
            return widget::text::body("Select a post to read.").into();
        };
        let mut content = widget::column::with_capacity(self.comments.len() + 10)
            .spacing(16)
            .push(widget::text::title1(&post.post.name))
            .push(widget::text::caption(format!(
                "!{} · by {} · {} points · {} comments",
                post.community.name, post.creator.name, post.counts.score, post.counts.comments
            )));
        let signed_in = self.api.as_ref().is_some_and(Api::is_authenticated);
        if signed_in {
            let enabled =
                !account_busy && self.pending_action.is_none() && self.submitting.is_none();
            let message = |action| enabled.then_some(Message::Act(self.generation, action));
            let upvote = if post.my_vote == Some(1) {
                "Upvoted ✓"
            } else {
                "Upvote"
            };
            let downvote = if post.my_vote == Some(-1) {
                "Downvoted ✓"
            } else {
                "Downvote"
            };
            let save = if post.saved { "Unsave" } else { "Save" };
            content = content.push(
                widget::row::with_capacity(3)
                    .spacing(8)
                    .push(widget::button::standard(upvote).on_press_maybe(message(Action::Upvote)))
                    .push(
                        widget::button::standard(downvote)
                            .on_press_maybe(message(Action::Downvote)),
                    )
                    .push(
                        widget::button::standard(save).on_press_maybe(message(Action::ToggleSaved)),
                    ),
            );
            if self.pending_action.is_some() {
                content = content.push(widget::text::caption("Updating post…"));
            }
            if let Some(error) = &self.action_error {
                content = content.push(widget::text::body(error));
            }
        }
        if let Some(url) = &post.post.url {
            content = content.push(
                widget::button::link(url.to_string()).on_press(Message::Link(url.to_string())),
            );
        }
        content = content.push(render_markdown(&self.body));
        let mut discussion = widget::row::with_capacity(2)
            .spacing(12)
            .align_y(cosmic::iced::Alignment::Center)
            .push(widget::text::title2("Comments"));
        if let Some(url) = self
            .api
            .as_ref()
            .and_then(|api| api.base.join(&format!("post/{}", post.post.id.0)).ok())
        {
            discussion = discussion.push(
                widget::button::text("Open discussion in browser")
                    .on_press(Message::Link(url.to_string())),
            );
        }
        content = content.push(discussion);
        if signed_in {
            content = content.push(self.comment_composer(
                &self.composer,
                CommentTarget::Post,
                account_busy,
            ));
        }
        if let Some(error) = &self.error {
            content = content
                .push(widget::text::body(error))
                .push(widget::button::standard("Retry comments").on_press(Message::Retry));
        }
        if self.comments.is_empty() && !self.busy && self.error.is_none() {
            content = content.push(widget::text::body("No comments yet."));
        }
        for comment in &self.comments {
            if thread::hidden(&comment.path, &self.collapsed) {
                continue;
            }
            let collapsed = self.collapsed.contains(&comment.data.comment.id.0);
            let mut row = widget::column::with_capacity(2).spacing(8).push(
                widget::button::text(format!(
                    "{} {} · {} points",
                    if collapsed { "+" } else { "−" },
                    comment.data.creator.name,
                    comment.data.counts.score
                ))
                .on_press(Message::Collapse(comment.data.comment.id.0)),
            );
            if !collapsed {
                row = row.push(render_markdown(&comment.body));
                if signed_in {
                    let id = comment.data.comment.id;
                    if let Some(composer) = self
                        .replies
                        .get(&id)
                        .filter(|_| self.active_reply == Some(id))
                    {
                        row = row.push(self.comment_composer(
                            composer,
                            CommentTarget::Reply(id),
                            account_busy,
                        ));
                    } else {
                        row = row.push(
                            widget::button::text("Reply").on_press_maybe(
                                (!account_busy && self.submitting.is_none())
                                    .then_some(Message::OpenReply(self.generation, id)),
                            ),
                        );
                    }
                }
            }
            let indent = comment.path.len().saturating_sub(1).min(8) as f32 * 16.0;
            let card = widget::container(row)
                .class(Container::Card)
                .padding([12, 16])
                .width(Length::Fill);
            content = content.push(widget::container(card).padding(Padding {
                right: 8.0,
                left: indent,
                ..Padding::ZERO
            }));
        }
        if self.busy {
            content = content.push(widget::text::body("Loading comments…"));
        }
        if self.more && !self.busy {
            content = content
                .push(widget::button::standard("Load more comments").on_press(Message::More));
        }
        widget::column::with_capacity(2)
            .spacing(12)
            .push(widget::button::standard("← Back to feed").on_press(Message::Back))
            .push(widget::scrollable(content).id("post").height(Length::Fill))
            .height(Length::Fill)
            .into()
    }
}
