use super::{Action, Message, Post, thread};
use crate::shared::api::Api;
use cosmic::{
    Element,
    iced::{Length, Padding},
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
        if self.api.as_ref().is_some_and(Api::is_authenticated) {
            let enabled = !account_busy && self.pending_action.is_none() && !self.submitting;
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
        if let Some(url) = self
            .api
            .as_ref()
            .and_then(|api| api.base.join(&format!("post/{}", post.post.id.0)).ok())
        {
            content = content.push(
                widget::button::text("Open discussion in browser")
                    .on_press(Message::Link(url.to_string())),
            );
        }
        content = content.push(widget::text::title2("Comments"));
        if self.api.as_ref().is_some_and(Api::is_authenticated) {
            let generation = self.generation;
            let mut editor = widget::text_editor::TextEditor::new(&self.draft)
                .placeholder("Write a comment")
                .height(Length::Fixed(100.0));
            if !self.submitting {
                editor = editor.on_action(move |action| Message::EditComment(generation, action));
            }
            let can_submit = !account_busy
                && !self.submitting
                && self.pending_action.is_none()
                && !self.draft.text().trim().is_empty();
            content = content.push(editor).push(
                widget::button::suggested(if self.submitting {
                    "Posting…"
                } else {
                    "Post comment"
                })
                .on_press_maybe(can_submit.then_some(Message::SubmitComment(generation))),
            );
            if let Some(error) = &self.submit_error {
                content = content.push(widget::text::body(error));
            }
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
            }
            content = content.push(
                widget::container(row)
                    .padding(Padding {
                        top: 8.0,
                        right: 8.0,
                        bottom: 8.0,
                        left: 12.0 + comment.path.len().saturating_sub(1).min(8) as f32 * 16.0,
                    })
                    .width(Length::Fill),
            );
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
