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
- Native COSMIC navigation, a side-by-side feed and discussion on wide windows, and single-pane reading on narrow windows.
- All and Local feeds, community feeds, and post search. Use **Ctrl+K** to focus search and **Enter** to submit it.
- Hot, New, Active, and Top today feed sorting, cursor pagination, and duplicate post suppression.
- Markdown discussions with collapsible reply cards and Top, New, and Old comment sorting.
- Vote on comments, write Markdown replies, and create text and link posts while signed in.
- Request timeouts, retryable errors, and stale-response protection when navigating.
- System theme, native widgets, browser links, and a saved instance preference.

Use the account button in the header to connect to an instance or manage saved accounts. The app supports account switching, optional two-factor login, native credential storage, and automatic session restoration. **Subscribed**, **Saved**, and **Inbox** are scoped to the active account. The inbox shows replies and mentions and opens the relevant comment in its discussion. Select the community name on a post to browse its feed, or open **Community details** to manage the subscription. Select your current vote again to clear it. Image and link posts can be opened in the browser, and feeds exclude NSFW posts. Inline media, private messaging, and API v4 support are future work.

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
