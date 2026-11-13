//! Client-side window decorations: titlebar and resize handles.
//!
//! Some compositors (e.g. GNOME on Wayland) do not draw window decorations,
//! so the app draws its own. When the compositor does draw them
//! (`Decorations::Server`), only the content is rendered.

mod resize_handles;
mod titlebar;

use gpui_kit::{Decorations, IntoElement, Window, div, prelude::*, rgb};

use crate::theme;

/// Wraps `content` with the titlebar and resize handles when needed.
pub fn frame(window: &Window, content: impl IntoElement) -> impl IntoElement {
    let (client_decorated, tiled) = match window.window_decorations() {
        Decorations::Client { tiling } => (true, tiling.is_tiled()),
        Decorations::Server => (false, false),
    };
    let resizable = client_decorated && !tiled && !window.is_maximized() && !window.is_fullscreen();

    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .bg(rgb(theme::BACKGROUND))
        .text_color(rgb(theme::TEXT))
        .when(client_decorated, |frame| {
            frame.child(titlebar::titlebar(window))
        })
        .child(div().flex_1().child(content))
        .when(resizable, |frame| {
            frame.children(resize_handles::resize_handles())
        })
}
