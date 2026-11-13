# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

H2K is a Rust desktop application built on [GPUI Kit](https://gpui-kit.com/) (`gpui-kit` 0.9, which pins `gpui-pre` 0.3). Single binary crate, Rust edition 2024. The window is a project workspace: create a docs folder under `~/Documents/h2k-projects/`, link local code repos without moving them, and browse both in one tree. `gpui-kit` is the dependency (rather than plain GPUI).

Layout:

- `src/main.rs`: entry point. Opens the window. With a tray, closing the window keeps the app running and the tray's Quit exits; without one (or if the tray host goes away), it quits when the last window closes.
- `src/project.rs`: project folders, repo links in `project.md`, and the file tree read from disk.
- `src/tray.rs`: system tray icon (StatusNotifierItem via `ksni`), forwards menu clicks to the main thread.
- `src/views/`: one file per view. `Root` is the window content: the kit `TitleBar` plus `Workspace`. Add new screens here.

Modules use `foo.rs` + `foo/` (no `mod.rs`).

## Commands

```bash
cargo run                         # build and launch the app window
cargo check                       # fast type-check (first run compiles GPUI deps, ~1 min)
cargo clippy --all-targets        # lint
cargo fmt                         # format
cargo test                        # run all tests
cargo test <name>                 # run tests whose name contains <name>
```

## Architecture

GPUI Kit app structure as used here. Depend on `gpui-kit` only; GPUI types are `gpui_kit::*`, and the layers are `gpui_kit::component`, `gpui_kit::base`, `gpui_kit::assets`, `gpui_kit::platform`.

- `main()` calls `gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(|cx| ...)`, which owns the event loop. `gpui_kit::init(cx)` runs first.
- Windows are opened with `gpui_kit::open_window`, which mounts `gpui_base::Root` so dialogs, sheets, and notifications work. The window asks for client decorations (GNOME on Wayland draws none otherwise). On Linux that root draws the kit window border (20px shadow plus a 1px frame), and `Root` draws `TitleBar` for drag and min/max/close.
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
