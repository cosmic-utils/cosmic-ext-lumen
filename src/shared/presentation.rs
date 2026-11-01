use cosmic::{
    Element,
    iced::Length,
    theme::Container,
    widget::{self, button},
};
use lemmy_api_common::lemmy_db_schema::source::community::Community;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn age(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64);
    let minutes = (now - timestamp).max(0) / 60;
    match minutes {
        0 => "now".into(),
        1..60 => format!("{minutes}m"),
        60..1440 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

pub fn community_handle(community: &Community) -> String {
    let host = community.actor_id.host_str().unwrap_or_default();
    format!("{}@{host}", community.name)
}

pub fn selectable_card<'a, Message: Clone + 'static>(
    content: impl Into<Element<'a, Message>>,
    padding: u16,
    selected: bool,
    action: Option<Message>,
) -> Element<'a, Message> {
    let radius = cosmic::theme::active().cosmic().corner_radii.radius_s;
    let card = button::custom(content)
        .class(button::ButtonClass::ListItem(radius))
        .padding(padding)
        .width(Length::Fill)
        .selected(selected)
        .on_press_maybe(action);
    widget::container(card)
        .class(Container::Card)
        .width(Length::Fill)
        .into()
}
