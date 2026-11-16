mod project;
mod tray;
mod views;

use futures::StreamExt;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Bounds, QuitMode, WindowBounds, WindowDecorations, prelude::*, px, size};

use tray::TrayEvent;
use views::Root;

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx: &mut App| {
            gpui_kit::init(cx);
            // Kit default is light, which paints the window white. The old
            // frame was a flat gray fill. Dark is the kit theme those panels use.
            Theme::change(ThemeMode::Dark, None, cx);

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

    // Platform window size, not layout padding.
    let bounds = Bounds::centered(None, size(px(960.), px(640.)), cx);
    let mut options = gpui_kit::component::TitleBar::window_options();
    options.window_bounds = Some(WindowBounds::Windowed(bounds));
    options.window_min_size = Some(size(px(720.), px(480.)));
    // GNOME on Wayland draws no frame unless the window asks for client
    // decorations. TitleBar and the kit window border both key off that mode.
    options.window_decorations = Some(WindowDecorations::Client);
    options.app_id = Some("h2k".into());
    if let Some(titlebar) = options.titlebar.as_mut() {
        // Shown by the shell (dock, alt-tab) instead of "Unknown".
        titlebar.title = Some("H2K".into());
    }
    gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Root::new(window, cx)))
        .expect("failed to open window");
    cx.activate(true);
}
