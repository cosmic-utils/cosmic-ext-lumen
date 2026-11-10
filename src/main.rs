fn main() -> Result<(), Box<dyn std::error::Error>> {
    cosmic_ext_lumen::i18n::init();
    let http = cosmic_ext_lumen::shared::api::http_client()?;
    cosmic::app::run::<cosmic_ext_lumen::app::App>(
        cosmic::app::Settings::default().size(cosmic::iced::Size::new(1100.0, 800.0)),
        http,
    )?;
    Ok(())
}
