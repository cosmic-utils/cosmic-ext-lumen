name := "cosmic-ext-lumen"
appid := "dev.edfloreshz.Lumen"
cargo-target-dir := env_var_or_default("CARGO_TARGET_DIR", "target")
desktop := "target/xdgen" / (appid + ".desktop")
metainfo := "target/xdgen" / (appid + ".metainfo.xml")
icon := "resources/icons/hicolor/scalable/apps/icon.svg"

default: run

run:
    cargo run --locked

check:
    cargo fmt --all -- --check
    cargo clippy --locked --all-targets -- -D warnings

test:
    cargo test --locked

build: build-release

build-debug *args:
    cargo build --locked --target-dir "{{cargo-target-dir}}" {{args}}

build-release *args: (build-debug "--release" args)

check-json:
    cargo clippy --locked --all-targets --message-format=json -- -D warnings

clean:
    cargo clean

install prefix=(env_var("HOME") / ".local"): build-release
    install -Dm755 "{{cargo-target-dir}}/release/{{name}}" "{{prefix}}/bin/{{name}}"
    install -Dm644 "{{desktop}}" "{{prefix}}/share/applications/{{appid}}.desktop"
    install -Dm644 "{{metainfo}}" "{{prefix}}/share/metainfo/{{appid}}.metainfo.xml"
    install -Dm644 "{{icon}}" "{{prefix}}/share/icons/hicolor/scalable/apps/{{appid}}.svg"

uninstall prefix=(env_var("HOME") / ".local"):
    rm -f -- "{{prefix}}/bin/{{name}}" \
      "{{prefix}}/share/applications/{{appid}}.desktop" \
      "{{prefix}}/share/metainfo/{{appid}}.metainfo.xml" \
      "{{prefix}}/share/icons/hicolor/scalable/apps/{{appid}}.svg"

# Save vendored sources and their Cargo configuration together.
vendor:
    #!/usr/bin/env bash
    set -euo pipefail
    project="$PWD"
    mkdir -p target
    work=$(mktemp -d "$project/target/vendor.XXXXXX")
    trap 'rm -rf -- "$work"' EXIT
    cd "$work"
    mkdir .cargo
    # Keep the xml crate directory distinct from the XML specs used by atspi.
    cargo vendor --locked --versioned-dirs --manifest-path "$project/Cargo.toml" vendor > .cargo/config.toml
    tar -cf vendor.tar .cargo/config.toml vendor
    mv vendor.tar "$project/target/vendor.tar"

# Use the bundle without installing a Cargo configuration in the checkout.
build-vendored:
    #!/usr/bin/env bash
    set -euo pipefail
    project="$PWD"
    target_dir=$(realpath -m -- "{{cargo-target-dir}}")
    work=$(mktemp -d "$project/target/vendor.XXXXXX")
    trap 'rm -rf -- "$work"' EXIT
    tar -xf "$project/target/vendor.tar" -C "$work"
    cd "$work"
    cargo build --manifest-path "$project/Cargo.toml" --target-dir "$target_dir" --release --frozen

clean-vendor:
    rm -f -- target/vendor.tar

# Build the AppImage, zsync metadata, and checksums under dist.
appimage:
    #!/usr/bin/env bash
    set -euo pipefail
    export ARCH=x86_64
    APPIMAGE=Lumen-x86_64.AppImage
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

    target_dir="{{cargo-target-dir}}"
    cargo build --locked --release --target-dir "$target_dir"
    install -Dm644 "{{metainfo}}" "$tools/AppDir/usr/share/metainfo/{{appid}}.metainfo.xml"
    install -Dm644 "{{icon}}" "$tools/{{appid}}.svg"
    desktop-file-validate "{{desktop}}"
    APPIMAGE_EXTRACT_AND_RUN=1 "$tools/linuxdeploy" --appdir "$tools/AppDir" \
      --executable "$target_dir/release/{{name}}" \
      --desktop-file "{{desktop}}" \
      --icon-file "$tools/{{appid}}.svg"

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
