use gpui_kit::{Context, SharedString, Window, div, prelude::*};

pub struct Home {
    text: SharedString,
}

impl Home {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl Render for Home {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .text_xl()
            .child(format!("Hello worlds, {}!", self.text))
    }
}
