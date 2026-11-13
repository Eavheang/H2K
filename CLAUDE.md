# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

H2K is a Rust desktop application built on [GPUI Kit](https://gpui-kit.com/) (`gpui-kit` 0.9, which pins `gpui-pre` 0.3). Single binary crate, Rust edition 2024. Currently a hello-world scaffold with a custom window frame. `gpui-kit` is the dependency (rather than plain GPUI) for its editor, Markdown, tree, tabs and panel components, which upcoming screens use; no screen uses one yet.

Layout:

- `src/main.rs`: entry point. Opens the window. With a tray, closing the window keeps the app running and the tray's Quit exits; without one (or if the tray host goes away), it quits when the last window closes.
- `src/tray.rs`: system tray icon (StatusNotifierItem via `ksni`), forwards menu clicks to the main thread.
- `src/assets.rs`: icons compiled into the binary from `assets/icons/`.
- `src/theme.rs`: shared colors (hex `u32`, pass to `rgb(..)`).
- `src/views/`: one file per view. `Root` is the window's root entity and wraps the current screen (`Home`) in the window chrome. Add new screens here.
- `src/window_chrome/`: client-side decorations (titlebar with min/max/close, resize handles). Drawn only when `window.window_decorations()` is `Decorations::Client` (e.g. GNOME on Wayland).

Modules use `foo.rs` + `foo/` (no `mod.rs`).

## Commands

```bash
cargo run                         # build and launch the app window
cargo check                       # fast type-check (first run compiles GPUI deps, ~1 min)
cargo clippy --all-targets        # lint
cargo fmt                         # format
cargo test                        # run all tests (none exist yet)
cargo test <name>                 # run tests whose name contains <name>
```

## Architecture

GPUI Kit app structure as used here. Depend on `gpui-kit` only; GPUI types are `gpui_kit::*`, and the layers are `gpui_kit::component`, `gpui_kit::base`, `gpui_kit::assets`, `gpui_kit::platform`.

- `main()` calls `gpui_kit::application().with_assets(..).run(|cx| ...)`, which owns the event loop. `gpui_kit::init(cx)` runs first.
- Windows are opened with `cx.open_window(WindowOptions, |window, cx| cx.new(|cx| View { .. }))`; the closure returns the root view entity. This is our chrome `Root`, not `gpui_kit::open_window`.
- `gpui_kit::open_window` mounts `gpui_base::Root` so dialogs, sheets and other overlays work. On Linux client decorations that root also draws gpui-component's window border (20px shadow plus a 1px frame). Switching to it changes this window's frame; restyle the chrome in the same change.
- A view is a plain struct implementing `Render`; `render()` returns an element tree built with `div()` and Tailwind-style builder methods (`.flex()`, `.size_full()`, `.bg(rgb(..))`, ...).
- View state lives in `Entity<T>`s created via `cx.new`; mutate through `Context<T>` and call `cx.notify()` to re-render.
- With a tray, `QuitMode::Explicit` keeps the process alive after the last window closes. Without a tray, or after the tray host disappears, `QuitMode::LastWindowClosed` quits. `gpui-pre` does this itself; there is no vendored GPUI patch.

## GPUI version caveat

`gpui-kit` 0.9.1 pins `gpui-pre` 0.3.12 exactly. That is not crates.io `gpui` 0.2.2 and not Zed's GitHub `main`. Before using an API, confirm it on the pinned `gpui-pre` (docs.rs/gpui-pre/0.3.12, or the crate source) or on gpui-kit's docs, rather than trusting Zed `main` examples or older training data.

## Project skills

Installed under `.agents/skills/` (symlinked into `.claude/skills/`, versions in `skills-lock.json`):

- `gpui-kit`, `gpui-kit-design-guides`: from longbridge/gpui-kit. Mechanics and `gpui_kit::component::*` both apply; the crate is a dependency.
- `gpui-test`: from Zed; covers `#[gpui::test]`, `TestAppContext`, seed reproduction. Written against Zed `main`. This repo's UI tests, when added, use `#[gpui_kit::test]` and should be checked against `gpui-pre` 0.3.12.
- `rust-skills`: general Rust guidance.
