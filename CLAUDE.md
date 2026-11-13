# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

H2K is a Rust desktop application built on [GPUI](https://www.gpui.rs/) (Zed's GPU-accelerated UI framework). Single binary crate, Rust edition 2024. Currently a hello-world scaffold: everything lives in `src/main.rs`.

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

GPUI app structure as used here:

- `main()` calls `Application::new().run(|cx: &mut App| ...)`, which owns the event loop.
- Windows are opened with `cx.open_window(WindowOptions, |window, cx| cx.new(|cx| View { .. }))`; the closure returns the root view entity.
- A view is a plain struct implementing `Render`; `render()` returns an element tree built with `div()` and Tailwind-style builder methods (`.flex()`, `.size_full()`, `.bg(rgb(..))`, ...).
- View state lives in `Entity<T>`s created via `cx.new`; mutate through `Context<T>` and call `cx.notify()` to re-render.

## GPUI version caveat

`gpui` is pinned to the crates.io release `0.2.2`. Zed's GPUI on GitHub `main` moves fast and differs from this release. Before using an API, confirm it exists in 0.2.2 (check `~/.cargo/registry/src/*/gpui-0.2.2/` or docs.rs/gpui/0.2.2) rather than trusting examples from Zed's repo or training data.

## Project skills

Installed under `.agents/skills/` (symlinked into `.claude/skills/`, versions in `skills-lock.json`):

- `gpui-kit`, `gpui-kit-design-guides`: from longbridge/gpui-kit. The GPUI mechanics sections apply here; the component sections (`gpui_kit::component::*`) only apply if the `gpui-kit` crate is added as a dependency, which it currently is not.
- `gpui-test`: from Zed; covers `#[gpui::test]`, `TestAppContext`, seed reproduction. Written against Zed `main`, so verify against 0.2.2.
- `rust-skills`: general Rust guidance.
