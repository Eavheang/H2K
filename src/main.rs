use gpui::{
    App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

struct HelloWorld {
    text: SharedString,
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .bg(rgb(0x505050))
            .text_xl()
            .text_color(rgb(0xffffff))
            .child(format!("Hello worlds, {}!", self.text))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| HelloWorld { text: "H2K".into() }),
        )
        .unwrap();
        cx.activate(true);
    });
}
