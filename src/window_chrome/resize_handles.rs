use gpui::{AnyElement, CursorStyle, Div, MouseButton, ResizeEdge, Stateful, div, prelude::*, px};

const BORDER: f32 = 6.;

/// Invisible strips along each edge and corner that start a window resize.
pub fn resize_handles() -> Vec<AnyElement> {
    let b = px(BORDER);
    vec![
        handle("resize-top", ResizeEdge::Top)
            .top_0()
            .left(b)
            .right(b)
            .h(b),
        handle("resize-bottom", ResizeEdge::Bottom)
            .bottom_0()
            .left(b)
            .right(b)
            .h(b),
        handle("resize-left", ResizeEdge::Left)
            .left_0()
            .top(b)
            .bottom(b)
            .w(b),
        handle("resize-right", ResizeEdge::Right)
            .right_0()
            .top(b)
            .bottom(b)
            .w(b),
        handle("resize-top-left", ResizeEdge::TopLeft)
            .top_0()
            .left_0()
            .size(b),
        handle("resize-top-right", ResizeEdge::TopRight)
            .top_0()
            .right_0()
            .size(b),
        handle("resize-bottom-left", ResizeEdge::BottomLeft)
            .bottom_0()
            .left_0()
            .size(b),
        handle("resize-bottom-right", ResizeEdge::BottomRight)
            .bottom_0()
            .right_0()
            .size(b),
    ]
    .into_iter()
    .map(IntoElement::into_any_element)
    .collect()
}

fn handle(id: &'static str, edge: ResizeEdge) -> Stateful<Div> {
    let cursor = match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    };

    div()
        .id(id)
        .absolute()
        .cursor(cursor)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            window.start_window_resize(edge);
        })
}
