mod assets;
mod theme;
mod tray;
mod views;
mod window_chrome;

use futures::StreamExt;
use gpui_kit::{
    App, Bounds, QuitMode, TitlebarOptions, WindowBounds, WindowDecorations, WindowOptions,
    prelude::*, px, size,
};

use tray::TrayEvent;
use views::Root;

fn main() {
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);

            let (tx, mut rx) = futures::channel::mpsc::unbounded();
            match tray::spawn(tx) {
                // Tray available: closing the window leaves the app running in the
                // tray; only the tray's "Quit" exits.
                Ok(()) => {
                    cx.set_quit_mode(QuitMode::Explicit);
                    cx.spawn(async move |cx| {
                        while let Some(event) = rx.next().await {
                            match event {
                                TrayEvent::Show => {
                                    cx.update(show_main_window);
                                }
                                TrayEvent::Quit => {
                                    cx.update(|cx| cx.quit());
                                    return;
                                }
                                TrayEvent::Gone => break,
                            }
                        }
                        // Tray lost mid-session: bring back a window hidden in the
                        // tray, since nothing else can reach it now.
                        cx.update(|cx| {
                            eprintln!("tray host went away, app will quit on window close");
                            cx.set_quit_mode(QuitMode::LastWindowClosed);
                            if cx.windows().is_empty() {
                                show_main_window(cx);
                            }
                        });
                    })
                    .detach();
                }
                Err(err) => {
                    eprintln!("tray unavailable ({err}), app will quit on window close");
                    cx.set_quit_mode(QuitMode::LastWindowClosed);
                }
            }

            show_main_window(cx);
        });
}

/// Focus the main window, or open it if it was closed.
fn show_main_window(cx: &mut App) {
    if let Some(window) = cx.windows().first().copied() {
        window
            .update(cx, |_, window, _| window.activate_window())
            .ok();
        return;
    }

    let bounds = Bounds::centered(None, size(px(500.), px(500.)), cx);
    // Our chrome Root, not gpui_kit::open_window: see CLAUDE.md, Architecture.
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
    .expect("failed to open window");
    cx.activate(true);
}
