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

install prefix=(env("HOME") / ".local"): build
    install -Dm755 target/release/cosmic-ext-lumen "{{prefix}}/bin/cosmic-ext-lumen"
    install -Dm644 resources/dev.edfloreshz.Lumen.desktop "{{prefix}}/share/applications/dev.edfloreshz.Lumen.desktop"
    install -Dm644 resources/icon.svg "{{prefix}}/share/icons/hicolor/scalable/apps/dev.edfloreshz.Lumen.svg"

# Build the AppImage, zsync metadata, and checksums under dist.
appimage:
    #!/usr/bin/env bash
    set -euo pipefail
    export ARCH=x86_64
    APPIMAGE=Lumen-x86_64.AppImage
    app_id=dev.edfloreshz.Lumen
    repository="${GITHUB_REPOSITORY:-cosmic-utils/cosmic-ext-lemmy}"

    if [[ "$(uname -sm)" != "Linux x86_64" ]]; then
        echo "AppImage packaging requires Linux x86_64." >&2
        exit 1
    fi
    for dependency in curl desktop-file-validate zsyncmake; do
        if ! command -v "$dependency" >/dev/null; then
            echo "Missing packaging dependency: $dependency" >&2
            exit 1
        fi
    done

    output="$PWD/dist"
    mkdir -p "$output"
    tools=$(mktemp -d "$output/.build.XXXXXX")
    trap 'rm -rf -- "$tools"' EXIT
    curl --fail --location --retry 3 \
      https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20250213-2/linuxdeploy-x86_64.AppImage \
      --output "$tools/linuxdeploy"
    curl --fail --location --retry 3 \
      https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
      --output "$tools/appimagetool"
    echo "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  $tools/appimagetool" | sha256sum --check
    chmod +x "$tools/linuxdeploy" "$tools/appimagetool"

    target_dir="${CARGO_TARGET_DIR:-target}"
    cargo build --locked --release --target-dir "$target_dir"
    install -Dm644 resources/icon.svg "$tools/$app_id.svg"
    desktop-file-validate "resources/$app_id.desktop"
    APPIMAGE_EXTRACT_AND_RUN=1 "$tools/linuxdeploy" --appdir "$tools/AppDir" \
      --executable "$target_dir/release/cosmic-ext-lumen" \
      --desktop-file "resources/$app_id.desktop" \
      --icon-file "$tools/$app_id.svg"

    cd "$tools"
    update_info="gh-releases-zsync|${repository%/*}|${repository#*/}|latest|${APPIMAGE}.zsync"
    APPIMAGE_EXTRACT_AND_RUN=1 "$tools/appimagetool" --updateinformation "$update_info" \
      --file-url "$APPIMAGE" AppDir "$APPIMAGE"
    test -s "$APPIMAGE.zsync"
    LC_ALL=C grep -aFxm1 "URL: $APPIMAGE" "$APPIMAGE.zsync"
    test "$(./"$APPIMAGE" --appimage-updateinformation)" = "$update_info"
    sha256sum "$APPIMAGE" "$APPIMAGE.zsync" > SHA256SUMS
    sha256sum --check SHA256SUMS
    mv -- "$APPIMAGE" "$APPIMAGE.zsync" SHA256SUMS "$output/"
    printf 'AppImage: %s/%s\n' "$output" "$APPIMAGE"
