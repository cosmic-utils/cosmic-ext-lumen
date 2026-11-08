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
    install -Dm755 target/release/cosmic-ext-lumen "{{prefix}}/bin/cosmic-ext-lumen"
    install -Dm644 resources/dev.edfloreshz.Lumen.desktop "{{prefix}}/share/applications/dev.edfloreshz.Lumen.desktop"
    install -Dm644 resources/icon.svg "{{prefix}}/share/icons/hicolor/scalable/apps/dev.edfloreshz.Lumen.svg"
