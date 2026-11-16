use gpui_kit::component::{TitleBar, v_flex};
use gpui_kit::{Context, Entity, Window, div, prelude::*};

use super::workspace::Workspace;

/// Window content: kit title bar, then the project workspace.
pub struct Root {
    workspace: Entity<Workspace>,
}

impl Root {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            workspace: cx.new(|cx| Workspace::new(window, cx)),
        }
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(TitleBar::new().child("H2K"))
            .child(div().flex_1().min_h_0().child(self.workspace.clone()))
    }
}
