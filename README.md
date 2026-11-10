<p align="center">
  <img src="resources/icons/hicolor/scalable/apps/icon.svg" width="112" height="112" alt="Lumen app icon">
</p>

<h1 align="center">Lumen</h1>

<p align="center"><strong>Find your people. Follow the thread.</strong></p>

Lumen is a native Lemmy client for the COSMIC desktop. Discover communities across the Fediverse, follow conversations, and publish from a single window.

## What you can do

- **Find your communities.** Search posts, comments, communities, and people. Paste a federated link or handle into search to open it in Lumen.
- **Read at your own pace.** Resize the side-by-side feed and discussion, collapse comment threads, and click images to enlarge or zoom them.
- **Keep up with replies.** Switch between accounts, revisit saved posts and comments, and open replies and mentions in context.
- **Join the discussion.** Vote, reply, publish image or link posts, and cross-post to other communities. Preview a post before publishing and pick up local drafts after restarting.
- **Pick up where you left off.** Lumen shows your cached feed and sidebar at startup while it checks for updates. Automatic image loading can be turned off in **Accounts and instances**.

## Feature roadmap

### Comment actions and thread navigation

- [x] Vote on comments and clear votes.
- [x] Save comments and revisit them in the Saved view.
- [x] Edit, delete, and restore your own comments.
- [x] Copy comment permalinks, open comments in context, and jump to parent comments.
- [x] Sort comments by Top, New, or Old.

### Search, communities, and profiles

- [x] Search posts, comments, communities, and people by keyword. Filter post and comment searches by community.
- [x] Open communities from posts and browse their feeds.
- [x] Subscribe and unsubscribe, and see pending remote subscriptions.
- [ ] Show dedicated community rules.
- [x] Open author profiles with post and comment history.
- [x] Resolve pasted federated URLs and handles inside the app.

### Inbox and notifications

- [x] See replies and mentions, mark them read or unread, and open comments in context.
- [x] Show an unread count for the selected account.
- [ ] Send and receive private messages.
- [ ] Enable opt-in desktop notifications for the selected account.

### Publishing and media

- [x] Create text and link posts, upload images, and cross-post.
- [x] Edit and delete your own posts.
- [x] Preview Markdown and resume local drafts after leaving the composer.
- [x] Show thumbnails and inline images, enlarge images on click, cache media, and let users toggle automatic media loading.

### Reading preferences and content controls

- [ ] Mark posts read or unread and browse an unread-only feed.
- [ ] Hide individual posts.
- [ ] Remember the preferred feed and sort per account.
- [ ] Add NSFW, language, and bot account controls.
- [ ] Block people, communities, and instances.
- [ ] Report posts and comments.

### Desktop polish and compatibility

- [x] Restore feed scroll position when returning from a thread.
- [x] Open search with **Ctrl+K**.
- [ ] Add keyboard shortcuts for navigation, refresh, and composing.
- [ ] Support Lemmy API v4 alongside 0.19 API v3, including authentication, inbox, saved items, and pagination. [API v4 upgrade guide](https://join-lemmy.org/docs/contributors/09-api-v4.html)
- [ ] Add moderator queues, moderation logs, community creation, and instance administration in a later milestone.

## Get started

Lumen works with **Lemmy 0.19.x** instances. Enter your instance hostname or root URL and choose **Connect** to browse, or sign in to vote, save, and publish. Press **Ctrl+K** to jump to search.

### Build and run

You’ll need Rust 1.93 or newer and a Linux desktop session. On Pop!_OS or Ubuntu, install the build dependencies from the project directory and run Lumen:

```sh
sudo apt install build-essential cmake pkgconf libexpat1-dev libfontconfig-dev libfreetype-dev libxkbcommon-dev
cargo run --locked
```

To install the launcher and icon into your user applications, run `just install`. With a custom prefix, run `just install /path/to/prefix`.

Use `just uninstall` with the same prefix to remove the installed files. Cargo
generates the localized desktop entry and AppStream metadata in `target/xdgen/`
from the templates in `resources/` and the Fluent strings in `i18n/`.

`just vendor` creates `target/vendor.tar`. Use `just build-vendored` to build
offline from that archive. On Linux x86_64, `just appimage` builds the release
assets in `dist/` with `curl`, `desktop-file-utils`, and `zsync` installed.
