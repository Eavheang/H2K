use gpui::{App, MouseButton, Window, div, prelude::*, px, rgb, svg};

use crate::theme;

const HEIGHT: f32 = 32.;
const BUTTON_WIDTH: f32 = 46.;
const ICON_SIZE: f32 = 14.;

pub fn titlebar(window: &Window) -> impl IntoElement {
    let maximize_icon = if window.is_maximized() {
        "icons/copy.svg"
    } else {
        "icons/square.svg"
    };

    div()
        .id("titlebar")
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .h(px(HEIGHT))
        .pl_3()
        .bg(rgb(theme::TITLEBAR_BACKGROUND))
        .text_sm()
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            if event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
        .on_mouse_down(MouseButton::Right, |event, window, _| {
            window.show_window_menu(event.position);
        })
        .child("H2K")
        .child(
            div()
                .flex()
                .flex_row()
                .h_full()
                .child(control_button(
                    "minimize",
                    "icons/minus.svg",
                    |window, _| window.minimize_window(),
                ))
                .child(control_button("maximize", maximize_icon, |window, _| {
                    window.zoom_window()
                }))
                .child(control_button("close", "icons/x.svg", |window, _| {
                    window.remove_window()
                })),
        )
}

fn control_button(
    id: &'static str,
    icon: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .justify_center()
        .items_center()
        .w(px(BUTTON_WIDTH))
        .h_full()
        .hover(|style| style.bg(rgb(theme::TITLEBAR_BUTTON_HOVER)))
        // Keep the titlebar's mouse-down (window move) from swallowing the click.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(
            svg()
                .path(icon)
                .size(px(ICON_SIZE))
                .text_color(rgb(theme::TEXT)),
        )
}
