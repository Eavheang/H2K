# Patched gpui 0.2.2

Copy of the crates.io `gpui` 0.2.2 release, wired in via `[patch.crates-io]` in the
root `Cargo.toml`. `examples/` was removed to keep the repo small.

## Change

Stock gpui on Linux stops the event loop as soon as the last window closes, so the
app exits even if it wants to keep running in the system tray. Removed that check
(`if state.windows.is_empty() { state.common.signal.stop(); }`) from:

- `src/platform/linux/wayland/client.rs`
- `src/platform/linux/x11/client.rs`

Quitting is now the app's job (`cx.quit()`), see `src/main.rs`.

Also suffixed two float literals in `src/taffy.rs` (`length(0.0_f32), fr(1.0_f32)`).
A path dependency's warnings are not silenced like a crates.io dependency's, and
newer rustc flags these as `float_literal_f32_fallback` (a future hard error).

## Upgrading gpui

Copy the new release over this directory, reapply the change above, bump the
version in the root `Cargo.toml`.
