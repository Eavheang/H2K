use gpui_kit::{Context, Entity, Window, prelude::*};

use super::home::Home;
use crate::window_chrome;

/// The window's root view: wraps the current screen in the window chrome.
pub struct Root {
    home: Entity<Home>,
}

impl Root {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            home: cx.new(|_| Home::new("H2K")),
        }
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        window_chrome::frame(window, self.home.clone())
    }
}
