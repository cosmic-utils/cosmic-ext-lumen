default: run

run:
    cargo run --locked

check:
    cargo fmt --all -- --check
    cargo clippy --locked --all-targets -- -D warnings

test:
    cargo test --locked

build:
    cargo build --locked --release

install prefix=(env_var("HOME") / ".local"): build
    install -Dm755 target/release/cosmic-ext-lemmy "{{prefix}}/bin/cosmic-ext-lemmy"
    install -Dm644 resources/io.github.cosmic_utils.cosmic_ext_lemmy.desktop "{{prefix}}/share/applications/io.github.cosmic_utils.cosmic_ext_lemmy.desktop"
    install -Dm644 resources/icon.svg "{{prefix}}/share/icons/hicolor/scalable/apps/io.github.cosmic_utils.cosmic_ext_lemmy.svg"
