use std::{env, error::Error, fs, path::Path};
use xdgen::{App, Context, FluentString};

fn main() -> Result<(), Box<dyn Error>> {
    let desktop = "resources/app.desktop";
    let metainfo = "resources/app.metainfo.xml";
    // Install and AppImage recipes use this path with any Cargo target directory.
    let output = Path::new("target/xdgen");
    let desktop_output = output.join("dev.edfloreshz.Lumen.desktop");
    let metainfo_output = output.join("dev.edfloreshz.Lumen.metainfo.xml");

    for path in [
        Path::new("i18n"),
        Path::new(desktop),
        Path::new(metainfo),
        &desktop_output,
        &metainfo_output,
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let context = Context::new("i18n", env::var("CARGO_PKG_NAME")?)?;
    let app = App::new(FluentString("app-title"))
        .comment(FluentString("app-comment"))
        .keywords(FluentString("app-keywords"));

    fs::create_dir_all(output)?;
    fs::write(desktop_output, app.expand_desktop(desktop, &context)?)?;
    fs::write(metainfo_output, app.expand_metainfo(metainfo, &context)?)?;
    Ok(())
}
