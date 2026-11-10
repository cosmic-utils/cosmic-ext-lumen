use std::sync::LazyLock;

use i18n_embed::{
    DesktopLanguageRequester, LanguageLoader, Localizer,
    fluent::{FluentLanguageLoader, fluent_language_loader},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("load embedded English translations");
    loader
});

pub fn init() {
    let localizer = i18n_embed::DefaultLocalizer::new(&*LOADER, &Localizations);
    if let Err(error) = localizer.select(&DesktopLanguageRequester::requested_languages()) {
        eprintln!("Could not load translations: {error}");
    }
}

pub fn app_title() -> String {
    LOADER.get("app-title")
}
