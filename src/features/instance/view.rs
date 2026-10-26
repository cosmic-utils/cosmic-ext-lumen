use super::{Instance, Message};
use cosmic::{Element, iced::Length, widget};

impl Instance {
    pub fn view(&self) -> Element<'_, Message> {
        let mut connect = widget::button::suggested(if self.busy {
            "Connecting…"
        } else {
            "Connect"
        });
        if !self.busy {
            connect = connect.on_press(Message::Connect);
        }
        let mut content = widget::column::with_capacity(5)
            .spacing(8)
            .push(widget::text::title3("Instance"))
            .push(
                widget::row::with_capacity(2)
                    .spacing(8)
                    .push(
                        widget::text_input("lemmy.world or https://your.instance", &self.input)
                            .on_input(Message::Input)
                            .on_submit(|_| Message::Connect)
                            .width(Length::Fill),
                    )
                    .push(connect),
            );
        if let Some(name) = &self.name {
            content = content.push(widget::text::caption(format!(
                "Connected to {name} · anonymous browsing"
            )));
        }
        if let Some(error) = &self.error {
            content = content.push(widget::text::body(error));
        }
        if let Some(notice) = &self.notice {
            content = content.push(widget::text::caption(notice));
        }
        content.into()
    }
}
