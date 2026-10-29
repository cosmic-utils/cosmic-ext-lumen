# Lemmy for COSMIC

A native Lemmy reader built with Rust and libcosmic.

## Run

Requires Rust 1.93 or newer, a Linux desktop session (Wayland or X11), and the native libcosmic build dependencies. On Pop!_OS / Ubuntu:

```sh
sudo apt install build-essential cmake pkgconf libexpat1-dev libfontconfig-dev libfreetype-dev libxkbcommon-dev
cargo run --locked
```

Enter a Lemmy instance hostname or root URL and select **Connect**. The last successful instance is saved in the platform configuration directory. Connection is explicit on each launch.

## Features

- Lemmy **0.19.x / API v3** instance connection and version validation.
- All and Local public feeds, with Hot, New, Active, and Top today sorting.
- Cursor pagination and duplicate post suppression.
- Post bodies and comments rendered as Markdown.
- Numerically ordered, collapsible comment threads; load additional comments in batches of 50.
- Write top-level comments with Markdown while signed in.
- Request timeouts, retryable errors, and stale-response protection when navigating.
- System theme, native widgets, browser links, and a saved instance preference.

Supports saved accounts on one or multiple instances, account switching, optional two-factor login, native credential storage (Linux Secret Service, macOS Keychain, Windows Credential Manager), automatic session restoration, and the Subscribed feed. Open a post while signed in to upvote, downvote, or save it. Open the Saved feed to find bookmarks for the selected account. Select your current vote again to clear it, or choose Unsave to remove the bookmark. Use Anonymous to browse without a session. Remove account deletes its locally saved token. Posting, search, inbox, inline image downloads/cache, and API v4 support are future work. Image/link posts can be opened in the browser. NSFW posts are excluded from feed requests. Comment order follows the thread path; siblings appear by numeric ID.

## Feature-based architecture

```text
src/
  app.rs                    # Window and feature coordination
  features/
    instance/               # Connection state, view, settings
    feed/                   # Feed state, view, endpoint calls
    post/                   # Post state, view, comments, threading
  shared/
    api.rs                  # HTTP transport and URL validation
    links.rs                # Browser opening
```

Each feature owns its messages, state transitions, view, and endpoint calls. The app shell maps messages and coordinates transitions; features do not depend on one another. Shared code is limited to transport and platform helpers. Feature tasks use the runtime executor rather than blocking the UI.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

An optional live integration test performs only public GET requests:

```sh
LEMMY_TEST_INSTANCE=https://lemmy.world cargo test --locked --test live -- --ignored --nocapture
```

Use `just run`, `just check`, or `just test` if `just` is installed. `just install` installs the release executable, desktop launcher, and icon into `~/.local` (override with `just install /your/prefix`).

libcosmic is pinned to a Git revision and the application lockfile is included for reproducible dependency resolution. No Lemmy server, database, API key, or federation implementation is required.
