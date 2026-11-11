<p align="center">
  <img src="resources/icons/hicolor/scalable/apps/icon.svg" width="112" height="112" alt="Lumen app icon">
</p>

<h1 align="center">Lumen</h1>

<p align="center"><strong>Find your people. Follow the thread.</strong></p>

A native Lemmy client for the COSMIC desktop, built with Rust and libcosmic.

Lumen targets Lemmy 0.19.x (API v3). It supports All, Local, Subscribed, and
Saved feeds; threaded comments; search across posts, comments, communities, and
people; multiple accounts with keyring-backed sessions; voting and saving; inbox
replies and mentions; and text, link, and image posts with local drafts.

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

## Configuration

The selected instance and account metadata are stored in the platform
configuration directory. Session tokens are stored in the desktop keyring, not
in the settings file.

## Architecture

- `src/app.rs`: window shell and cross-feature navigation.
- `src/features/`: accounts, instance, feed, post, inbox, publish, community.
- `src/shared/`: API transport, caching, media, links, and presentation helpers.

Each feature owns its state, messages, view, and service calls. Features do not
depend on one another; the app shell coordinates them.

## Localization

UI strings live in `i18n/en/cosmic_ext_lumen.ftl`. Add a language by creating
`i18n/<language-tag>/cosmic_ext_lumen.ftl`; missing messages fall back to
English. The build also localizes the desktop entry and AppStream metadata from
the catalog.

## Testing

```sh
cargo test --locked
```

The optional live test performs public GET requests only:

```sh
LEMMY_TEST_INSTANCE=https://lemmy.world cargo test --locked --test live -- --ignored --nocapture
```

## Status

Supports Lemmy 0.19.x only. API v4, moderation tools, private messages, and
desktop notifications are not implemented yet.
