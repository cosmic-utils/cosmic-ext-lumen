use cosmic::{
    Element,
    iced::{Border, Color, Length},
    prelude::ColorExt,
    theme::Container,
    widget::{self, button, text_editor},
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

/// A multi-line editor styled to match a COSMIC text input.
pub fn text_editor<'a, Message: Clone + 'static>(
    content: &'a text_editor::Content<cosmic::Renderer>,
    placeholder: &'a str,
) -> text_editor::TextEditor<'a, Message> {
    let spacing = cosmic::theme::active().cosmic().spacing;
    text_editor::TextEditor::new(content)
        .padding(spacing.space_xxs)
        .placeholder(placeholder)
        .style(text_input_style)
}

/// The appearance of a COSMIC text input, adapted to a text editor.
fn text_input_style(theme: &cosmic::Theme, status: text_editor::Status) -> text_editor::Style {
    let palette = theme.cosmic();
    let container = theme.current_container();
    let mut background: Color = container.small_widget.into();
    background.a = 0.25;
    text_editor::Style {
        background: background.into(),
        border: Border {
            radius: palette.corner_radii.radius_s.into(),
            width: 2.0,
            color: match status {
                text_editor::Status::Hovered | text_editor::Status::Focused { .. } => {
                    palette.accent.base.into()
                }
                text_editor::Status::Active | text_editor::Status::Disabled => {
                    container.component.divider.into()
                }
            },
        },
        placeholder: {
            let color: Color = container.on.into();
            color.blend_alpha(background, 0.7)
        },
        value: palette.on_bg_color().into(),
        selection: palette.accent_color().into(),
    }
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
