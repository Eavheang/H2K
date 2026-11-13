//! System tray icon (freedesktop StatusNotifierItem, shown by GNOME's
//! AppIndicator extension, KDE, etc.). Menu clicks run on ksni's own thread,
//! so they are forwarded to the GPUI main thread over a channel.

use futures::channel::mpsc::UnboundedSender;
use ksni::blocking::{Handle, TrayMethods};

pub enum TrayEvent {
    Show,
    Quit,
}

struct Tray {
    events: UnboundedSender<TrayEvent>,
}

impl ksni::Tray for Tray {
    // Left click opens the window instead of the menu.
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "H2K".into()
    }

    fn icon_name(&self) -> String {
        "application-x-executable".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.events.unbounded_send(TrayEvent::Show).ok();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Open H2K".into(),
                activate: Box::new(|this: &mut Self| {
                    this.events.unbounded_send(TrayEvent::Show).ok();
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|this: &mut Self| {
                    this.events.unbounded_send(TrayEvent::Quit).ok();
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Fails when the desktop has no tray host (e.g. stock GNOME without the
/// AppIndicator extension).
pub fn spawn(events: UnboundedSender<TrayEvent>) -> Result<Handle<impl ksni::Tray>, ksni::Error> {
    Tray { events }.spawn()
}
