//! Thin app shell. Features own state, messages, views, and endpoint calls.
use crate::features::{accounts, feed, instance, post};
use cosmic::{ApplicationExt, Element, app::Task, iced::Length, widget};

pub struct App {
    accounts: accounts::Accounts,
    core: cosmic::Core,
    instance: instance::Instance,
    feed: feed::Feed,
    post: post::Post,
}

#[derive(Clone, Debug)]
pub enum Message {
    Accounts(accounts::Message),
    Instance(instance::Message),
    Feed(feed::Message),
    Post(post::Message),
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = reqwest::Client;
    type Message = Message;
    const APP_ID: &'static str = "io.github.cosmic_utils.cosmic_ext_lemmy";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, http: Self::Flags) -> (Self, Task<Message>) {
        let mut app = Self {
            accounts: accounts::Accounts::new(http.clone()),
            core,
            instance: instance::Instance::new(http),
            feed: feed::Feed::default(),
            post: post::Post::default(),
        };
        app.set_header_title("Lemmy".into());
        let task = if let Some(id) = app.core.main_window_id() {
            app.set_window_title("Lemmy".into(), id)
        } else {
            Task::none()
        };
        let restore = app
            .accounts
            .restore()
            .map(|m| cosmic::Action::App(Message::Accounts(m)));
        (app, cosmic::iced::Task::batch([task, restore]))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Accounts(message) => {
                let (task, session) = self.accounts.update(message, &self.instance.input);
                let mut tasks = vec![task.map(|m| cosmic::Action::App(Message::Accounts(m)))];
                if let Some(api) = session {
                    self.instance.input = api.base.to_string();
                    self.post.reset();
                    tasks.push(
                        self.feed
                            .connect(api)
                            .map(|m| cosmic::Action::App(Message::Feed(m))),
                    );
                }
                cosmic::iced::Task::batch(tasks)
            }

            Message::Instance(message) => {
                let (task, connected) = self.instance.update(message);
                let mut tasks = vec![task.map(|m| cosmic::Action::App(Message::Instance(m)))];
                if let Some(api) = connected {
                    self.accounts.anonymous();
                    self.post.reset();
                    tasks.push(
                        self.feed
                            .connect(api)
                            .map(|m| cosmic::Action::App(Message::Feed(m))),
                    );
                }
                cosmic::iced::Task::batch(tasks)
            }
            Message::Feed(message) => {
                let (task, selected) = self.feed.update(message);
                let mut tasks = vec![task.map(|m| cosmic::Action::App(Message::Feed(m)))];
                if let (Some(post), Some(api)) = (selected, self.feed.api()) {
                    tasks.push(
                        self.post
                            .open(api, post)
                            .map(|m| cosmic::Action::App(Message::Post(m))),
                    );
                }
                cosmic::iced::Task::batch(tasks)
            }
            Message::Post(post::Message::Back) => {
                self.post.close();
                Task::none()
            }
            Message::Post(post::Message::Act(..) | post::Message::SubmitComment(..))
                if self.accounts.busy =>
            {
                Task::none()
            }
            Message::Post(post::Message::CommentSubmitted(generation, result)) => {
                if let Some(id) = self.post.finish_comment(generation, result) {
                    self.feed.comment_added(id);
                }
                Task::none()
            }
            Message::Post(post::Message::ActionFinished(generation, result)) => {
                if let Some(updated) = self.post.finish_action(generation, result) {
                    self.feed
                        .apply_post_update(updated)
                        .map(|m| cosmic::Action::App(Message::Feed(m)))
                } else {
                    Task::none()
                }
            }
            Message::Post(message) => self
                .post
                .update(message)
                .map(|m| cosmic::Action::App(Message::Post(m))),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content = if self.post.selected.is_some() {
            self.post.view(self.accounts.busy).map(Message::Post)
        } else {
            self.feed.view().map(Message::Feed)
        };
        widget::container(
            widget::column::with_capacity(3)
                .spacing(20)
                .push(if self.accounts.busy {
                    widget::text::body("Restoring account…").into()
                } else {
                    self.instance.view().map(Message::Instance)
                })
                .push(self.accounts.view().map(Message::Accounts))
                .push(widget::text::caption("Lemmy for COSMIC · API v3 preview"))
                .push(content)
                .height(Length::Fill),
        )
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
