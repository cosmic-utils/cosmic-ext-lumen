<p align="center">
  <img src="resources/icon.svg" width="112" height="112" alt="Lumen app icon">
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

## Get started

Lumen works with **Lemmy 0.19.x** instances. Enter your instance hostname or root URL and choose **Connect** to browse, or sign in to vote, save, and publish. Press **Ctrl+K** to jump to search.

### Build and run

You’ll need Rust 1.93 or newer and a Linux desktop session. On Pop!_OS or Ubuntu, install the build dependencies from the project directory and run Lumen:

```sh
sudo apt install build-essential cmake pkgconf libexpat1-dev libfontconfig-dev libfreetype-dev libxkbcommon-dev
cargo run --locked
```

To install the launcher and icon into your user applications, run `just install`. With a custom prefix, run `just install /path/to/prefix`.
