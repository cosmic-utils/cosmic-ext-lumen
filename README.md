<p align="center">
  <img src="resources/icons/hicolor/scalable/apps/icon.svg" width="112" height="112" alt="Lumen app icon">
</p>

<h1 align="center">Lumen</h1>

<p align="center"><strong>Find your people. Follow the thread.</strong></p>

<img width="1428" height="939" alt="image" src="https://github.com/user-attachments/assets/6d132930-fbf1-4cd1-90b8-39c676b064da" />

A native Lemmy client for the COSMIC desktop, built with Rust and libcosmic.

## Build

Requires Rust 1.93 or newer and a Linux desktop session. On Pop!_OS or Ubuntu:

```sh
sudo apt install build-essential cmake pkgconf libexpat1-dev libfontconfig-dev libfreetype-dev libxkbcommon-dev
cargo run --locked
```

`just run`, `just check`, and `just test` wrap the Cargo commands. `just install`
installs the binary, desktop entry, AppStream metadata, and icon under
`~/.local` by default; pass a prefix to install elsewhere. On Linux x86_64,
`just appimage` builds the release assets in `dist/` and requires `curl`,
`desktop-file-utils`, and `zsync`.
