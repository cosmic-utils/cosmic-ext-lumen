use crate::fl;
mod settings;
mod view;

use crate::shared::api::Api;
use cosmic::iced::Task;
use lemmy_api_common::site::GetSiteResponse;

pub struct Instance {
    pub input: String,
    pub name: Option<String>,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    request: u64,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub enum Message {
    Input(String),
    Connect,
    Connected(u64, Result<(Api, String), String>),
}

impl Instance {
    pub fn new(http: reqwest::Client) -> Self {
        let (input, notice) = settings::load();
        Self {
            input,
            name: None,
            busy: false,
            error: None,
            notice,
            request: 0,
            http,
        }
    }

    pub fn update(&mut self, message: Message) -> (Task<Message>, Option<Api>) {
        match message {
            Message::Input(input) => {
                self.input = input;
            }
            Message::Connect => {
                self.request += 1;
                self.error = None;
                match Api::new(self.http.clone(), &self.input) {
                    Err(error) => {
                        self.busy = false;
                        self.error = Some(error);
                    }
                    Ok(api) => {
                        self.busy = true;
                        let request = self.request;
                        return (
                            Task::perform(
                                async move {
                                    let response: GetSiteResponse = api.get("site", &()).await?;
                                    if !response.version.starts_with("0.19.") {
                                        return Err(fl!(
                                            "instance-version-unsupported",
                                            version = response.version
                                        ));
                                    }
                                    Ok((api, response.site_view.site.name))
                                },
                                move |result| Message::Connected(request, result),
                            ),
                            None,
                        );
                    }
                }
            }
            Message::Connected(request, result) if request == self.request => {
                self.busy = false;
                match result {
                    Ok((api, name)) => {
                        self.input = api.base.to_string();
                        self.name = Some(name);
                        self.notice = settings::save(&self.input).err();
                        return (Task::none(), Some(api));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Connected(..) => {}
        }
        (Task::none(), None)
    }
}
