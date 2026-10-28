use super::{Feed, Message, Scope, Sort};
use cosmic::{Element, iced::Length, widget};

impl Feed {
    pub fn view(&self) -> Element<'_, Message> {
        let mut controls = widget::row::with_capacity(7)
            .spacing(8)
            .push(widget::text::title2("Discover"))
            .push(widget::dropdown(
                Sort::LABELS.as_slice(),
                Some(self.sort.index()),
                Message::Sort,
            ));
        for (scope, label) in [
            (Scope::Local, "Local"),
            (Scope::All, "All"),
            (Scope::Subscribed, "Subscribed"),
            (Scope::Saved, "Saved"),
        ] {
            if scope != Scope::Saved || self.can_view_saved() {
                let label = if self.scope == scope {
                    format!("{label} ✓")
                } else {
                    label.to_owned()
                };
                controls =
                    controls.push(widget::button::text(label).on_press(Message::Scope(scope)));
            }
        }
        let mut refresh = widget::button::standard("Refresh");
        if !self.busy {
            refresh = refresh.on_press(Message::Refresh);
        }
        controls = controls.push(refresh);
        let mut content = widget::column::with_capacity(self.posts.len() + 4).spacing(12);
        if let Some(error) = &self.error {
            content = content.push(widget::text::body(error));
        }
        if self.posts.is_empty() {
            content = content.push(widget::text::body(if self.busy {
                "Loading posts…"
            } else if self.loaded && self.scope == Scope::Saved {
                "No saved posts."
            } else if self.loaded {
                "No posts found in this feed."
            } else {
                "Connect to an instance to start reading."
            }));
        }
        for post in &self.posts {
            let card = widget::column::with_capacity(3)
                .spacing(6)
                .push(widget::text::caption(format!(
                    "!{} · by {}",
                    post.community.name, post.creator.name
                )))
                .push(widget::text::title3(&post.post.name))
                .push(widget::text::caption(format!(
                    "{} points · {} comments{}",
                    post.counts.score,
                    post.counts.comments,
                    if post.post.nsfw { " · NSFW" } else { "" }
                )));
            content = content.push(
                widget::button::custom(widget::container(card).padding(16).width(Length::Fill))
                    .on_press(Message::Open(post.post.id.0))
                    .width(Length::Fill),
            );
        }
        if self.cursor.is_some() {
            let mut more = widget::button::standard(if self.busy {
                "Loading…"
            } else {
                "Load more posts"
            });
            if !self.busy {
                more = more.on_press(Message::More);
            }
            content = content.push(more);
        }
        widget::column::with_capacity(2)
            .spacing(16)
            .push(controls)
            .push(widget::scrollable(content).id("feed").height(Length::Fill))
            .height(Length::Fill)
            .into()
    }
}
