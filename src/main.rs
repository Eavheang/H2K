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
                Ok(_) => {
                    cx.spawn(async move |cx| {
                        while let Some(event) = rx.next().await {
                            match event {
                                TrayEvent::Show => {
                                    if cx.update(show_main_window).is_err() {
                                        return;
                                    }
                                }
                                TrayEvent::Quit => {
                                    cx.update(|cx| cx.quit()).ok();
                                    return;
                                }
                                TrayEvent::Gone => break,
                            }
                        }
                        // Tray lost mid-session: bring back a window hidden in the
                        // tray, since nothing else can reach it now.
                        cx.update(|cx| {
                            eprintln!("tray host went away, app will quit on window close");
                            quit_when_last_window_closes(cx);
                            if cx.windows().is_empty() {
                                show_main_window(cx);
                            }
                        })
                        .ok();
                    })
                    .detach();
                }
                Err(err) => {
                    eprintln!("tray unavailable ({err}), app will quit on window close");
                    quit_when_last_window_closes(cx);
                }
            }

            show_main_window(cx);
        });
}

/// Without a tray, the app would otherwise keep running with no way to reach it.
fn quit_when_last_window_closes(cx: &mut App) {
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
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
