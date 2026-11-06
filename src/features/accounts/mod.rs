mod storage;
use crate::shared::{api::Api, media::Media};
use cosmic::{
    Element,
    iced::{Length, Task},
    widget,
};
use lemmy_api_common::{
    lemmy_db_schema::{
        SubscribedType,
        newtypes::PersonId,
        source::{community::Community, person::Person},
    },
    lemmy_db_views_actor::structs::CommunityView,
    person::{Login, LoginResponse},
    site::GetSiteResponse,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub instance: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Registry {
    pub accounts: Vec<Account>,
    pub active: Option<String>,
}
#[derive(Clone)]
pub enum Message {
    Username(String),
    Password(String),
    Code(String),
    Login,
    Switch(usize),
    Anonymous,
    Remove,
    Ready(Result<Box<Session>, String>),
    Removed(Result<Option<String>, String>),
}
#[derive(Clone)]
pub struct Session {
    account: Account,
    api: Api,
    follows: Vec<Community>,
    person_id: PersonId,
}

pub struct Accounts {
    registry: Registry,
    username: String,
    password: String,
    code: String,
    pub busy: bool,
    error: Option<String>,
    http: reqwest::Client,
    session: Option<Api>,
    follows: Vec<Community>,
    person_id: Option<PersonId>,
}
impl Accounts {
    pub fn new(http: reqwest::Client) -> Self {
        let (registry, error) = match storage::load() {
            Ok(r) => (r, None),
            Err(e) => (Registry::default(), Some(e)),
        };
        Self {
            registry,
            username: String::new(),
            password: String::new(),
            code: String::new(),
            busy: false,
            error,
            http,
            session: None,
            follows: Vec::new(),
            person_id: None,
        }
    }
    pub fn anonymous(&mut self) {
        self.registry.active = None;
        self.session = None;
        self.follows.clear();
        self.person_id = None;
        self.error = storage::save(&self.registry).err();
    }
    pub fn restore(&mut self) -> Task<Message> {
        if let Some(index) = self
            .registry
            .accounts
            .iter()
            .position(|a| Some(&a.id) == self.registry.active.as_ref())
        {
            self.switch(index)
        } else {
            Task::none()
        }
    }
    fn switch(&mut self, index: usize) -> Task<Message> {
        let Some(mut account) = self.registry.accounts.get(index).cloned() else {
            return Task::none();
        };
        self.busy = true;
        self.error = None;
        let http = self.http.clone();
        Task::perform(
            async move {
                let id = account.id.clone();
                let token = tokio::task::spawn_blocking(move || storage::read_token(&id))
                    .await
                    .map_err(|_| "Credential worker failed".to_string())??;
                let api = Api::new(http, &account.instance)?.authenticated(token);
                let (person, follows) = validate(&api).await?;
                let person_id = person.id;
                account.name = person.name;
                account.avatar = person.avatar.map(|url| url.to_string());
                Ok(Session {
                    account,
                    api,
                    follows,
                    person_id,
                })
            },
            |result| Message::Ready(result.map(Box::new)),
        )
    }
    pub fn update(&mut self, message: Message, instance: &str) -> (Task<Message>, Option<Api>) {
        match message {
            Message::Username(v) => self.username = v,
            Message::Password(v) => self.password = v,
            Message::Code(v) => self.code = v,
            Message::Login if !self.busy => {
                let api = match Api::new(self.http.clone(), instance) {
                    Ok(api) => api,
                    Err(e) => {
                        self.error = Some(e);
                        return (Task::none(), None);
                    }
                };
                if api.base.scheme() != "https"
                    && !matches!(
                        api.base.host_str(),
                        Some("localhost" | "127.0.0.1" | "[::1]")
                    )
                {
                    self.error = Some("Login requires HTTPS (except localhost).".into());
                    return (Task::none(), None);
                }
                self.busy = true;
                self.error = None;
                let login = Login {
                    username_or_email: self.username.clone().into(),
                    password: std::mem::take(&mut self.password).into(),
                    totp_2fa_token: if self.code.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut self.code))
                    },
                };
                let existing = self.registry.accounts.clone();
                return (
                    Task::perform(
                        async move {
                            let response: LoginResponse = api.post("user/login", &login).await?;
                            let token = response
                                .jwt
                                .ok_or("No session token returned")?
                                .into_inner();
                            let api = api.authenticated(token.clone());
                            let (person, follows) = validate(&api).await?;
                            let person_id = person.id;
                            let id = existing
                                .iter()
                                .find(|a| a.instance == api.base.as_str() && a.name == person.name)
                                .map_or_else(
                                    || uuid::Uuid::new_v4().to_string(),
                                    |account| account.id.clone(),
                                );
                            let account = Account {
                                id,
                                instance: api.base.to_string(),
                                name: person.name,
                                avatar: person.avatar.map(|url| url.to_string()),
                            };
                            let id = account.id.clone();
                            tokio::task::spawn_blocking(move || storage::write_token(&id, &token))
                                .await
                                .map_err(|_| "Credential worker failed".to_string())??;
                            Ok(Session {
                                account,
                                api,
                                follows,
                                person_id,
                            })
                        },
                        |result| Message::Ready(result.map(Box::new)),
                    ),
                    None,
                );
            }
            Message::Switch(index) if !self.busy => return (self.switch(index), None),
            Message::Ready(result) => {
                self.busy = false;
                match result.map(|session| *session) {
                    Ok(Session {
                        account,
                        api,
                        follows,
                        person_id,
                    }) => {
                        self.session = Some(api.clone());
                        self.follows = follows;
                        self.person_id = Some(person_id);
                        self.registry.active = Some(account.id.clone());
                        if let Some(saved) = self
                            .registry
                            .accounts
                            .iter_mut()
                            .find(|item| item.id == account.id)
                        {
                            *saved = account;
                        } else {
                            self.registry.accounts.push(account);
                        }
                        self.error = storage::save(&self.registry).err();
                        return (Task::none(), Some(api));
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Message::Anonymous if !self.busy => {
                self.registry.active = None;
                self.error = storage::save(&self.registry).err();
                return (Task::none(), self.leave_session(instance));
            }
            Message::Remove if !self.busy => {
                if let Some(id) = self.registry.active.clone() {
                    self.busy = true;
                    let session = self.session.clone();
                    return (
                        Task::perform(
                            async move {
                                let warning = if let Some(api) = session {
                                    api.post::<_, lemmy_api_common::SuccessResponse>("user/logout", &serde_json::json!({})).await.err().map(|_| "Removed locally; server session could not be revoked. Revoke it from Lemmy settings.".to_owned())
                                } else {
                                    None
                                };
                                tokio::task::spawn_blocking(move || storage::delete_token(&id))
                                    .await
                                    .map_err(|_| "Credential worker failed".to_string())??;
                                Ok(warning)
                            },
                            Message::Removed,
                        ),
                        None,
                    );
                }
            }
            Message::Removed(result) => {
                self.busy = false;
                match result {
                    Ok(warning) => {
                        let id = self.registry.active.take();
                        self.registry
                            .accounts
                            .retain(|a| Some(&a.id) != id.as_ref());
                        self.error = storage::save(&self.registry).err().or(warning);
                        return (Task::none(), self.leave_session(instance));
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            _ => {}
        }
        (Task::none(), None)
    }

    fn leave_session(&mut self, instance: &str) -> Option<Api> {
        let previous = self.session.take();
        self.follows.clear();
        self.person_id = None;
        Api::new(self.http.clone(), instance)
            .ok()
            .or_else(|| previous.map(Api::into_anonymous))
    }

    pub fn active(&self) -> Option<&Account> {
        self.session.as_ref()?;
        self.registry
            .accounts
            .iter()
            .find(|account| Some(&account.id) == self.registry.active.as_ref())
    }

    pub fn person_id(&self) -> Option<PersonId> {
        self.person_id
    }

    pub fn follows(&self) -> &[Community] {
        &self.follows
    }

    pub fn update_follow(&mut self, community: &CommunityView) {
        self.follows
            .retain(|item| item.id != community.community.id);
        if community.subscribed == SubscribedType::Subscribed {
            self.follows.push(community.community.clone());
        }
    }

    pub fn avatars(&self) -> impl Iterator<Item = &str> {
        self.registry
            .accounts
            .iter()
            .filter_map(|account| account.avatar.as_deref())
    }

    pub fn view(&self, media: &Media) -> Element<'_, Message> {
        let mut content = widget::column::with_capacity(5).spacing(8);
        let mut picker = widget::column::with_capacity(self.registry.accounts.len() + 2).spacing(4);
        for (index, a) in self.registry.accounts.iter().enumerate() {
            let mut b = widget::button::text(format!(
                "{}@{}{}",
                a.name,
                a.instance
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/'),
                if self.registry.active.as_ref() == Some(&a.id) {
                    " ✓"
                } else {
                    ""
                }
            ))
            .leading_icon(media.avatar(a.avatar.as_deref(), "avatar-default-symbolic"));
            if !self.busy {
                b = b.on_press(Message::Switch(index))
            }
            picker = picker.push(b.width(Length::Fill));
        }
        if !self.busy {
            picker = picker.push(widget::button::text("Anonymous").on_press(Message::Anonymous));
            if self.registry.active.is_some() {
                picker =
                    picker.push(widget::button::text("Remove account").on_press(Message::Remove));
            }
        }
        content = content.push(picker);
        let mut login = widget::button::suggested(if self.busy {
            "Working…"
        } else {
            "Add account"
        });
        if !self.busy {
            login = login.on_press(Message::Login)
        }
        content = content.push(
            widget::column::with_capacity(4)
                .spacing(8)
                .push(
                    widget::text_input("Username or email", &self.username)
                        .on_input(Message::Username),
                )
                .push(
                    widget::text_input::secure_input("Password", &self.password, None, true)
                        .on_input(Message::Password),
                )
                .push(widget::text_input("2FA code (optional)", &self.code).on_input(Message::Code))
                .push(login),
        );
        if let Some(e) = &self.error {
            content = content.push(widget::text::body(e))
        }
        content.into()
    }
}
async fn validate(api: &Api) -> Result<(Person, Vec<Community>), String> {
    let site: GetSiteResponse = api.get("site", &()).await?;
    if !site.version.starts_with("0.19.") {
        return Err("Requires Lemmy 0.19.x".into());
    }
    site.my_user
        .map(|u| {
            (
                u.local_user_view.person,
                u.follows
                    .into_iter()
                    .map(|follow| follow.community)
                    .collect(),
            )
        })
        .ok_or("Session expired; log in again.".into())
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccountsMessage([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_redacted() {
        assert!(
            !format!("{:?}", Message::Password("secret-password".into()))
                .contains("secret-password")
        );
    }
    #[test]
    fn registry_contains_only_metadata() {
        let r = Registry {
            accounts: vec![Account {
                id: "a".into(),
                instance: "https://example.com/".into(),
                name: "alice".into(),
                avatar: None,
            }],
            active: Some("a".into()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("token"));
        assert_eq!(
            serde_json::from_str::<Registry>(&json)
                .unwrap()
                .accounts
                .len(),
            1
        );
    }
}
