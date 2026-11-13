mod assets;
mod theme;
mod tray;
mod views;
mod window_chrome;

use futures::StreamExt;
use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBounds, WindowDecorations, WindowOptions,
    prelude::*, px, size,
};

use tray::TrayEvent;
use views::Root;

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            let (tx, mut rx) = futures::channel::mpsc::unbounded();
            match tray::spawn(tx) {
                // Tray available: closing the window leaves the app running in the
                // tray; only the tray's "Quit" exits.
                Ok(handle) => {
                    cx.spawn(async move |cx| {
                        let _handle = handle; // dropping it would remove the icon
                        while let Some(event) = rx.next().await {
                            let quit = cx
                                .update(|cx| match event {
                                    TrayEvent::Show => show_main_window(cx),
                                    TrayEvent::Quit => cx.quit(),
                                })
                                .is_err();
                            if quit {
                                break;
                            }
                        }
                    })
                    .detach();
                }
                // No tray host: quit when the last window closes, otherwise the app
                // would keep running with no way to reach it.
                Err(err) => {
                    eprintln!("tray unavailable ({err}), app will quit on window close");
                    cx.on_window_closed(|cx| {
                        if cx.windows().is_empty() {
                            cx.quit();
                        }
                    })
                    .detach();
                }
            }

            show_main_window(cx);
        });
}

/// Focus the main window, or open it if it was closed.
fn show_main_window(cx: &mut App) {
    if let Some(window) = cx.windows().first() {
        window
            .update(cx, |_, window, _| window.activate_window())
            .ok();
        return;
    }

    let bounds = Bounds::centered(None, size(px(500.), px(500.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_decorations: Some(WindowDecorations::Client),
            // Shown by the shell (dock, alt-tab) instead of "Unknown".
            titlebar: Some(TitlebarOptions {
                title: Some("H2K".into()),
                ..Default::default()
            }),
            app_id: Some("h2k".into()),
            ..Default::default()
        },
        |_, cx| cx.new(Root::new),
    )
    .unwrap();
    cx.activate(true);
}
