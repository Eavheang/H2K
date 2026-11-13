//! Project shelf, new-project dialog, and the open project's file tree.

use std::cell::Cell;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::base::Selectable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tree::{Tree, TreeEntry, TreeEvent, TreeItem, TreeState};
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};
use gpui_kit::{
    App, Context, Entity, Focusable, IntoElement, PathPromptOptions, SharedString, Subscription,
    Window, div, prelude::*, rems,
};

use crate::project::{self, CreateError, FileNode};

/// How often the open tree re-reads expanded directories.
///
/// ponytail: poll on the UI thread. Switch to a watcher if an expanded repo hitches.
const REFRESH: Duration = Duration::from_millis(500);

pub struct Workspace {
    project: Option<PathBuf>,
    project_name: SharedString,
    projects: Vec<String>,
    search: Entity<InputState>,
    _search_events: Subscription,
    tree: Entity<TreeState>,
    expanded: HashSet<String>,
    fingerprint: Option<u64>,
    error: Option<SharedString>,
    _events: Subscription,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects"));
        let search_events = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let tree = cx.new(|cx| TreeState::new(cx));
        let events = cx.subscribe(&tree, |this, _, event, cx| {
            this.on_tree_event(event, cx);
        });
        let mut workspace = Self {
            project: None,
            project_name: SharedString::default(),
            projects: Vec::new(),
            search,
            _search_events: search_events,
            tree,
            expanded: HashSet::new(),
            fingerprint: None,
            error: None,
            _events: events,
        };
        workspace.refresh_projects();
        workspace.watch(cx);
        workspace
    }

    fn refresh_projects(&mut self) {
        match project::workspace_root() {
            Ok(root) => match project::list_projects(&root) {
                Ok(names) => self.projects = names,
                Err(err) => {
                    self.projects.clear();
                    self.error = Some(format!("Couldn’t list projects. {err}").into());
                }
            },
            Err(err) => {
                self.projects.clear();
                self.error = Some(err.to_string().into());
            }
        }
    }

    fn watch(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                if this
                    .update(cx, |this, cx| this.reload(None, false, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_tree_event(&mut self, event: &TreeEvent, cx: &mut Context<Self>) {
        let id = match event {
            TreeEvent::Expanded(id) => {
                self.expanded.insert(id.to_string());
                id.clone()
            }
            TreeEvent::Collapsed(id) => {
                self.expanded.remove(id.as_ref());
                id.clone()
            }
        };
        // `set_items` from this callback would re-enter `TreeState`.
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            this.update(cx, |this, cx| this.reload(Some(id), true, cx))
                .ok();
        });
    }

    fn reload(&mut self, select: Option<SharedString>, force: bool, cx: &mut Context<Self>) {
        let Some(root) = self.project.clone() else {
            return;
        };
        let nodes = match project::read_project_tree(&root, &self.expanded) {
            Ok(nodes) => nodes,
            Err(err) => {
                self.error = Some(format!("Couldn’t read the project. {err}").into());
                cx.notify();
                return;
            }
        };
        let fingerprint = fingerprint(&nodes);
        let error_cleared = self.error.take().is_some();
        if !force && self.fingerprint == Some(fingerprint) {
            if error_cleared {
                cx.notify();
            }
            return;
        }
        self.fingerprint = Some(fingerprint);
        let items = tree_items(&nodes, &self.expanded);
        self.tree.update(cx, |tree, cx| {
            let keep = select.or_else(|| tree.selected_item().map(|item| item.id.clone()));
            tree.set_items(items, cx);
            if let Some(id) = keep {
                tree.set_selected_index(tree.index_of(&id), cx);
            }
        });
        if error_cleared {
            cx.notify();
        }
    }

    fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.project_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into();
        self.project = Some(path);
        self.expanded.clear();
        self.expanded.insert(project::DOCS_ID.into());
        self.fingerprint = None;
        self.error = None;
        self.refresh_projects();
        self.reload(None, true, cx);
        // Dialog close restores the trigger. Focus the tree after that.
        cx.on_next_frame(window, |this, window, cx| {
            this.tree.update(cx, |tree, cx| tree.focus(window, cx));
        });
        cx.notify();
    }

    fn open_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let root = match project::workspace_root() {
            Ok(root) => root,
            Err(err) => {
                self.error = Some(err.to_string().into());
                cx.notify();
                return;
            }
        };
        self.open_project(root.join(name), window, cx);
    }

    fn open_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = cx.entity();
        let name = cx.new(|cx| InputState::new(window, cx));
        let form = cx.new(|_| NewProjectForm {
            name: name.clone(),
            error: None,
        });
        let initial_focus = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if initial_focus.replace(false) {
                name.read(cx).focus_handle(cx).focus(window, cx);
            }
            let form_ok = form.clone();
            let workspace = workspace.clone();
            dialog
                .title("New project")
                .child(form.clone())
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                        .child(
                            DialogAction::new()
                                .child(Button::new("create-project").primary().label("Create")),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    let value = form_ok.read(cx).name.read(cx).value();
                    let root = match project::workspace_root() {
                        Ok(root) => root,
                        Err(err) => {
                            show_error(&form_ok, &err, cx);
                            return false;
                        }
                    };
                    match project::create_project(&root, &value) {
                        Ok(path) => {
                            workspace.update(cx, |this, cx| this.open_project(path, window, cx));
                            true
                        }
                        Err(err) => {
                            show_error(&form_ok, &err, cx);
                            false
                        }
                    }
                })
        });
    }

    fn on_picked(
        &mut self,
        path: PathBuf,
        replace: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !project::is_git_repo(&path) {
            let workspace = cx.entity();
            window.open_alert_dialog(cx, move |alert, _, _| {
                let path = path.clone();
                let replace = replace.clone();
                let workspace = workspace.clone();
                alert
                    .title("Not a git repository")
                    .description(
                        "This folder can still be linked. Git will not be initialized here.",
                    )
                    .confirm()
                    .ok_text("Link anyway")
                    .on_ok(move |_, _, cx| {
                        let path = path.clone();
                        let replace = replace.clone();
                        workspace.update(cx, |this, cx| {
                            this.apply_link(path, replace, cx);
                        });
                        true
                    })
            });
            return;
        }
        self.apply_link(path, replace, cx);
    }

    fn apply_link(&mut self, path: PathBuf, replace: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let result = if let Some(old) = replace {
            project::relink_repo(&project, &old, &path)
        } else {
            project::link_repo(&project, &path)
        };
        match result {
            Ok(()) => {
                self.error = None;
                self.reload(None, true, cx);
            }
            Err(err) => self.error = Some(err.to_string().into()),
        }
        cx.notify();
    }

    fn confirm_unlink(
        &mut self,
        name: SharedString,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let path = path.clone();
            let workspace = workspace.clone();
            alert
                .confirm()
                .title(format!("Unlink “{name}”?"))
                .description("Removes the link. The folder stays on disk.")
                .ok_text("Unlink")
                .on_ok(move |_, _, cx| {
                    let path = path.clone();
                    workspace.update(cx, |this, cx| this.unlink(path, cx));
                    true
                })
        });
    }

    fn unlink(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        if let Err(err) = project::unlink_repo(&project, &path) {
            self.error = Some(err.to_string().into());
            cx.notify();
            return;
        }
        self.error = None;
        self.reload(None, true, cx);
        cx.notify();
    }

    fn import_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Import project folder".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.open_project(path, window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = cx.entity();
        let query = self.search.read(cx).value();
        let visible_projects = self
            .projects
            .iter()
            .filter(|name| project_matches_query(name, &query))
            .collect::<Vec<_>>();
        let selected_project = self.project.as_ref().and_then(|path| {
            let root = project::workspace_root().ok()?;
            if path.parent() != Some(root.as_path()) {
                return None;
            }
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        v_flex()
            .id("projects")
            .w(rems(16.))
            .h_full()
            .flex_shrink_0()
            .min_h_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_r_1()
            .border_color(cx.theme().border)
            .px_2()
            .py_2()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.search)
                                .small()
                                .cleanable(true)
                                .prefix(Icon::new(AssetIcon::Search).small()),
                        ),
                    )
                    .child(
                        Button::new("refresh-projects")
                            .small()
                            .ghost()
                            .icon(Icon::new(IconName::RefreshCw).small())
                            .tooltip("Refresh projects")
                            .accessibility_label("Refresh projects")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_projects();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("import-project")
                            .small()
                            .ghost()
                            .icon(Icon::new(AssetIcon::FolderInput).small())
                            .tooltip("Import project folder")
                            .accessibility_label("Import project folder")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.import_project(window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .mt_2()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .px_2()
                            .pb_1()
                            .child(div().text_sm().font_medium().child("Projects"))
                            .child(
                                Button::new("new-project-folder")
                                    .small()
                                    .ghost()
                                    .icon(Icon::new(AssetIcon::FolderPlus).small())
                                    .label("New project")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_new_project(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        v_flex()
                            .id("project-list")
                            .flex_1()
                            .min_h_0()
                            .gap_1()
                            .overflow_y_scrollbar()
                            .when(self.projects.is_empty(), |list| {
                                list.child(
                                    div()
                                        .px_2()
                                        .py_2()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No projects yet"),
                                )
                            })
                            .when(
                                !self.projects.is_empty() && visible_projects.is_empty(),
                                |list| {
                                    list.child(
                                        div()
                                            .px_2()
                                            .py_2()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("No matching projects"),
                                    )
                                },
                            )
                            .children(visible_projects.into_iter().map(|name| {
                                let name = name.clone();
                                let open_name = name.clone();
                                let workspace = workspace.clone();
                                let selected = selected_project.as_deref() == Some(name.as_str());
                                Button::new(format!("project:{name}"))
                                    .small()
                                    .ghost()
                                    .selected(selected)
                                    .w_full()
                                    .accessibility_label(format!("Open project {name}"))
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .min_w_0()
                                            .items_center()
                                            .justify_start()
                                            .gap_2()
                                            .child(Icon::new(IconName::Folder).small())
                                            .child(div().min_w_0().flex_1().truncate().child(name)),
                                    )
                                    .on_click(move |_, window, cx| {
                                        workspace.update(cx, |this, cx| {
                                            this.open_named(&open_name, window, cx);
                                        });
                                    })
                            })),
                    ),
            )
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self
            .project
            .as_ref()
            .map(|path| project::display_path(path))
            .unwrap_or_default();
        h_flex()
            .w_full()
            .items_center()
            .px_4()
            .py_3()
            .gap_3()
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_medium()
                            .truncate()
                            .child(self.project_name.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(path),
                    ),
            )
            .child(
                Button::new("link-repo")
                    .small()
                    .ghost()
                    .label("Link repo…")
                    .on_click(cx.listener(|_, _, window, cx| pick_folder(window, cx, None))),
            )
    }

    fn render_tree(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = cx.entity();
        Tree::new(&self.tree, |_, entry, selected, _, cx| {
            tree_row(entry, selected, cx)
        })
        .context_menu(move |_ix, entry, menu, _window, _cx| {
            let Some((missing, path)) = project::repo_root(&entry.item().id) else {
                return menu;
            };
            let path = PathBuf::from(path);
            let name = entry.item().label.clone();
            let menu = if missing {
                let workspace = workspace.clone();
                let path = path.clone();
                menu.item(
                    PopupMenuItem::new("Relink…").on_click(move |_, window, cx| {
                        let path = path.clone();
                        workspace.update(cx, |_, cx| pick_folder(window, cx, Some(path)));
                    }),
                )
            } else {
                menu
            };
            let workspace = workspace.clone();
            menu.item(PopupMenuItem::new("Unlink").on_click(move |_, window, cx| {
                let path = path.clone();
                let name = name.clone();
                workspace.update(cx, |this, cx| {
                    this.confirm_unlink(name, path, window, cx);
                });
            }))
        })
    }

    fn render_inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.tree.read(cx).selected_item().and_then(|item| {
            if item.is_disabled() {
                None
            } else {
                Some((item.id.clone(), item.label.clone()))
            }
        });
        let mut panel = v_flex()
            .id("inspector")
            .w_64()
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .p_3()
            .gap_3()
            .overflow_y_scrollbar();
        let Some((id, label)) = selected else {
            return panel.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Select a file."),
            );
        };
        let Some(root) = self.project.as_ref() else {
            return panel;
        };
        let path = project::display_path(&project::node_path(root, &id));
        let scope = project::node_scope(&id);
        let repo = project::repo_root(&id).map(|(missing, path)| (missing, PathBuf::from(path)));
        let show_scope = repo.is_none();
        panel = panel.child(div().truncate().child(label.clone()));
        if show_scope {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(scope),
            );
        }
        panel = panel.child(
            div()
                .text_sm()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().muted_foreground)
                .child(path),
        );
        let Some((missing, path)) = repo else {
            return panel;
        };
        if missing {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().warning)
                    .child("This folder is missing."),
            );
        }
        let mut actions = h_flex().gap_2();
        if missing {
            let path = path.clone();
            actions = actions.child(
                Button::new("relink-repo")
                    .small()
                    .label("Relink…")
                    .on_click(cx.listener(move |_, _, window, cx| {
                        pick_folder(window, cx, Some(path.clone()));
                    })),
            );
        }
        panel.child(
            actions.child(
                Button::new("unlink-repo")
                    .small()
                    .ghost()
                    .label("Unlink")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_unlink(label.clone(), path.clone(), window, cx);
                    })),
            ),
        )
    }

    fn render_open(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .min_h_0()
            .child(self.render_toolbar(cx))
            .when_some(self.error.clone(), |column, error| {
                column.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .items_stretch()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .min_h_0()
                            .child(self.render_tree(cx)),
                    )
                    .when(
                        self.tree
                            .read(cx)
                            .selected_item()
                            .is_some_and(|item| !item.is_disabled()),
                        |row| row.child(self.render_inspector(cx)),
                    ),
            )
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let message = if self.projects.is_empty() {
            "A project is a folder of notes and decisions. Linked code stays where it already is."
        } else {
            "Choose a project from the sidebar."
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .p_4()
            .gap_2()
            .child(
                div()
                    .text_lg()
                    .font_medium()
                    .child(if self.projects.is_empty() {
                        "H2K"
                    } else {
                        "Projects"
                    }),
            )
            .child(
                div()
                    .max_w(rems(28.))
                    .text_color(cx.theme().muted_foreground)
                    .child(message),
            )
    }
}

fn show_error(form: &Entity<NewProjectForm>, err: &CreateError, cx: &mut App) {
    form.update(cx, |form, cx| {
        form.error = Some(err.to_string().into());
        cx.notify();
    });
}

fn project_matches_query(name: &str, query: &str) -> bool {
    name.to_lowercase().contains(&query.to_lowercase())
}

fn fingerprint(nodes: &[FileNode]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fn walk(nodes: &[FileNode], hasher: &mut impl Hasher) {
        for node in nodes {
            node.name.hash(hasher);
            node.rel.hash(hasher);
            node.is_dir.hash(hasher);
            walk(&node.children, hasher);
        }
    }
    walk(nodes, &mut hasher);
    hasher.finish()
}

fn pick_folder(window: &mut Window, cx: &mut Context<Workspace>, replace: Option<PathBuf>) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(
            if replace.is_some() {
                "Relink repo"
            } else {
                "Link repo"
            }
            .into(),
        ),
    });
    cx.spawn_in(window, async move |this, cx| {
        let picked = match receiver.await {
            Ok(Ok(Some(paths))) => paths.into_iter().next(),
            Ok(Err(err)) => {
                this.update(cx, |this, cx| {
                    this.error = Some(format!("Couldn’t open the folder picker. {err}").into());
                    cx.notify();
                })
                .ok();
                return;
            }
            _ => return,
        };
        let Some(path) = picked else { return };
        this.update_in(cx, |this, window, cx| {
            this.on_picked(path, replace, window, cx);
        })
        .ok();
    })
    .detach();
}

/// `Tree` treats an item with no children as a file, so a directory always
/// gets one child: a stand-in while collapsed, or "Empty" once opened.
fn tree_items(nodes: &[FileNode], expanded: &HashSet<String>) -> Vec<TreeItem> {
    nodes
        .iter()
        .map(|node| {
            let mut item = TreeItem::new(node.rel.clone(), node.name.clone());
            if node.is_dir {
                let open = expanded.contains(&node.rel);
                let mut children = if open {
                    tree_items(&node.children, expanded)
                } else {
                    Vec::new()
                };
                if children.is_empty() {
                    let label = if open { "Empty" } else { "" };
                    children.push(TreeItem::new(format!("{}\0", node.rel), label).disabled(true));
                }
                item = item.children(children).expanded(open);
            }
            item
        })
        .collect()
}

fn tree_row(entry: &TreeEntry, selected: bool, cx: &App) -> ListItem {
    let item = entry.item();
    let placeholder = item.label.is_empty() || item.label.as_ref() == "Empty";
    let missing = project::repo_root(&item.id).is_some_and(|(missing, _)| missing);
    let mut chevron = div()
        .w_4()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center();
    if item.is_folder() {
        let icon = if item.is_expanded() {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        chevron = chevron.child(Icon::new(icon).xsmall());
    }
    let mut icon_slot = div()
        .w_4()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center();
    if !placeholder {
        let mut icon = Icon::new(row_icon(item)).small();
        if missing {
            icon = icon.text_color(cx.theme().warning);
        }
        icon_slot = icon_slot.child(icon);
    }
    let mut row = h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1()
        .child(chevron)
        .child(icon_slot)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .child(item.label.clone()),
        );
    if missing {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_sm()
                .text_color(cx.theme().warning)
                .child("Missing"),
        );
    }
    ListItem::new(item.id.clone())
        .selected(selected)
        .pl(rems(0.75 + entry.depth() as f32 * 0.75))
        .child(row)
}

fn row_icon(item: &TreeItem) -> IconName {
    if project::repo_root(&item.id).is_some_and(|(missing, _)| missing) {
        return IconName::CircleAlert;
    }
    if item.id.as_ref() == project::DOCS_ID {
        return IconName::BookOpen;
    }
    if item.is_folder() {
        IconName::Folder
    } else {
        IconName::File
    }
}

struct NewProjectForm {
    name: Entity<InputState>,
    error: Option<SharedString>,
}

impl Render for NewProjectForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_2()
            .child("Name")
            .child(Input::new(&self.name).id("project-name"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Documents/h2k-projects"),
            )
            .when_some(self.error.clone(), |form, error| {
                form.child(div().text_sm().text_color(cx.theme().danger).child(error))
            })
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main = if self.project.is_none() {
            self.render_empty(cx).into_any_element()
        } else {
            self.render_open(cx).into_any_element()
        };
        let radii = gpui_kit::component::window_corner_radii(window, cx);
        h_flex()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .rounded_bl(radii.bottom_left)
            .rounded_br(radii.bottom_right)
            .child(self.render_sidebar(cx))
            .child(main)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(rel: &str, name: &str) -> FileNode {
        FileNode {
            name: name.into(),
            rel: rel.into(),
            is_dir: true,
            children: Vec::new(),
        }
    }

    #[test]
    fn tree_items_keep_empty_directories_expandable() {
        let nodes = vec![dir("notes", "notes")];
        let collapsed = tree_items(&nodes, &HashSet::new());
        assert!(collapsed[0].is_folder());
        assert!(!collapsed[0].is_expanded());
        assert_eq!(collapsed[0].children.len(), 1);
        assert!(collapsed[0].children[0].is_disabled());

        let mut expanded = HashSet::new();
        expanded.insert("notes".into());
        let open = tree_items(&nodes, &expanded);
        assert!(open[0].is_expanded());
        assert_eq!(open[0].children[0].label.as_ref(), "Empty");
        assert!(open[0].children[0].is_disabled());
    }

    #[test]
    fn project_search_matches_names_case_insensitively() {
        assert!(project_matches_query("Dev Notes", "dev"));
        assert!(project_matches_query("Dev Notes", "NOTES"));
        assert!(project_matches_query("Dev Notes", ""));
        assert!(!project_matches_query("Dev Notes", "api"));
    }
}
