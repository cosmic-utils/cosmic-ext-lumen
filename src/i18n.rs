use std::sync::LazyLock;

use i18n_embed::{
    DesktopLanguageRequester, LanguageLoader, Localizer,
    fluent::{FluentLanguageLoader, fluent_language_loader},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

#[doc(hidden)]
pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
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
    crate::fl!("app-title")
}

#[macro_export]
macro_rules! fl {
    ($($message:tt)*) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $($message)*)
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn subscriber_plural() {
        let text = |count: i64| {
            crate::fl!("community-subscribers", count = count).replace(['\u{2068}', '\u{2069}'], "")
        };
        assert_eq!(text(0), "0 subscribers");
        assert_eq!(text(1), "1 subscriber");
        assert_eq!(text(2), "2 subscribers");
    }
}
