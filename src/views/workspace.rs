//! Three panes: projects sidebar with the open project's file tree, a
//! read-only file viewer, and a details panel for the selected node.

use std::cell::Cell;
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gpui_kit::assets::IconName as AssetIcon;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::breadcrumb::Breadcrumb;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::empty::{
    Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle,
};
use gpui_kit::component::form::{field, v_form};
use gpui_kit::component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_kit::component::label::Label;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::toolbar::Toolbar;
use gpui_kit::component::tree::{Tree, TreeEntry, TreeEvent, TreeItem, TreeState};
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, StyledExt, WindowExt, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, App, Context, Entity, Focusable, IntoElement, PathPromptOptions, SharedString,
    Subscription, Window, div, prelude::*, px, relative, rems,
};

use crate::project::{self, CreateError, FileNode, FileText};

/// How often the projects list, the open tree, and the open file are re-read.
///
/// ponytail: poll on the UI thread. Switch to a watcher if an expanded repo hitches.
const REFRESH: Duration = Duration::from_millis(500);

pub struct Workspace {
    project: Option<PathBuf>,
    project_name: SharedString,
    projects: Vec<String>,
    /// Why the projects list could not be read.
    list_error: Option<SharedString>,
    search: Entity<InputState>,
    tree: Entity<TreeState>,
    expanded: HashSet<String>,
    fingerprint: Option<u64>,
    /// Why the open tree could not be read. Cleared by the next good read.
    tree_error: Option<SharedString>,
    /// Result of the last create, link, or unlink. Stays until the next one.
    error: Option<SharedString>,
    selection: Option<Selection>,
    _subscriptions: Vec<Subscription>,
}

/// The selected tree node, with what the viewer and details panel show.
struct Selection {
    id: SharedString,
    /// Modified time and length of a file, to notice edits made outside H2K.
    stamp: Option<(Option<SystemTime>, u64)>,
    branch: Option<String>,
    body: Body,
}

enum Body {
    /// A folder or a missing repo: nothing to view.
    None,
    Text(Entity<EditorState>),
    Message(SharedString),
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects"));
        let tree = cx.new(|cx| TreeState::new(cx));
        let subscriptions = vec![
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.reload(None, false, cx);
                    cx.notify();
                }
            }),
            cx.subscribe_in(&tree, window, |this, _, event, window, cx| {
                this.on_tree_event(event, window, cx);
            }),
            cx.observe_in(&tree, window, |this, _, window, cx| {
                this.sync_selection(window, cx);
            }),
        ];
        let mut workspace = Self {
            project: None,
            project_name: SharedString::default(),
            projects: Vec::new(),
            list_error: None,
            search,
            tree,
            expanded: HashSet::new(),
            fingerprint: None,
            tree_error: None,
            error: None,
            selection: None,
            _subscriptions: subscriptions,
        };
        workspace.refresh_projects();
        workspace.reload(None, true, cx);
        workspace.watch(window, cx);
        workspace
    }

    /// `true` when the list changed.
    fn refresh_projects(&mut self) -> bool {
        let listed = project::workspace_root()
            .map_err(|err| err.to_string())
            .and_then(|root| {
                project::list_projects(&root)
                    .map_err(|err| format!("Couldn’t list projects. {err}"))
            });
        let (projects, error) = match listed {
            Ok(names) => (names, None),
            Err(err) => (Vec::new(), Some(SharedString::from(err))),
        };
        let changed = projects != self.projects || error != self.list_error;
        self.projects = projects;
        self.list_error = error;
        changed
    }

    fn watch(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                let polled = this.update_in(cx, |this, window, cx| {
                    if this.refresh_projects() {
                        cx.notify();
                    }
                    this.reload(None, false, cx);
                    this.sync_selection(window, cx);
                });
                if polled.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_tree_event(&mut self, event: &TreeEvent, window: &mut Window, cx: &mut Context<Self>) {
        let (id, open) = match event {
            TreeEvent::Expanded(id) => (id.clone(), true),
            TreeEvent::Collapsed(id) => (id.clone(), false),
        };
        // `set_items` from this callback would re-enter `TreeState`.
        cx.defer_in(window, move |this, window, cx| {
            if let Some(path) = project_path_of(&id) {
                if open {
                    this.open_project(path, window, cx);
                } else {
                    this.close_project(cx);
                }
                return;
            }
            if open {
                this.expanded.insert(id.to_string());
            } else {
                this.expanded.remove(id.as_ref());
            }
            this.reload(Some(id), true, cx);
        });
    }

    /// Projects in the sidebar: the workspace's folders that match the
    /// search, plus the open project when it is filtered out or imported
    /// from elsewhere.
    fn visible_projects(&self, cx: &App) -> Vec<PathBuf> {
        let query = self.search.read(cx).value();
        let mut paths: Vec<PathBuf> = match project::workspace_root() {
            Ok(root) => self
                .projects
                .iter()
                .filter(|name| project_matches_query(name, &query))
                .map(|name| root.join(name))
                .collect(),
            Err(_) => Vec::new(),
        };
        if let Some(open) = &self.project
            && !paths.contains(open)
        {
            paths.insert(0, open.clone());
        }
        paths
    }

    /// Rebuild the sidebar tree: one row per project, with the open
    /// project's docs folder and linked repos under it.
    fn reload(&mut self, select: Option<SharedString>, force: bool, cx: &mut Context<Self>) {
        let roots = match self.project.clone() {
            None => Vec::new(),
            Some(root) => match project::read_project_tree(&root, &self.expanded) {
                Ok(nodes) => {
                    if self.tree_error.take().is_some() {
                        cx.notify();
                    }
                    nodes
                }
                Err(err) => {
                    let error = SharedString::from(format!("Couldn’t read the project. {err}"));
                    if self.tree_error.as_ref() != Some(&error) {
                        self.tree_error = Some(error);
                        cx.notify();
                    }
                    Vec::new()
                }
            },
        };
        let projects = self.visible_projects(cx);
        let fingerprint = fingerprint(&projects, self.project.as_deref(), &roots);
        if !force && self.fingerprint == Some(fingerprint) {
            return;
        }
        self.fingerprint = Some(fingerprint);
        let items: Vec<TreeItem> = projects
            .iter()
            .map(|path| {
                let open = self.project.as_ref() == Some(path);
                project_item(path, open, &roots, &self.expanded)
            })
            .collect();
        self.tree.update(cx, |tree, cx| {
            let keep = select.or_else(|| tree.selected_item().map(|item| item.id.clone()));
            tree.set_items(items, cx);
            if let Some(id) = keep {
                tree.set_selected_index(tree.index_of(&id), cx);
            }
        });
    }

    /// Bring the viewer and details panel in line with the tree's selection,
    /// and re-read the open file when it changed on disk.
    fn sync_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self.tree.read(cx).selected_item().and_then(|item| {
            (!item.is_disabled() && project_path_of(&item.id).is_none())
                .then(|| (item.id.clone(), item.is_folder()))
        });
        let (Some(root), Some((id, is_folder))) = (self.project.clone(), picked) else {
            if self.selection.take().is_some() {
                cx.notify();
            }
            return;
        };
        let missing = project::repo_root(&id).is_some_and(|(missing, _)| missing);
        let path = project::node_path(&root, &id);
        let viewable = !is_folder && !missing;
        let stamp = if viewable { file_stamp(&path) } else { None };
        let previous = self.selection.take().filter(|selection| selection.id == id);
        // Rebuild below unless the same node is unchanged.
        let previous = match previous {
            Some(previous) if previous.stamp == stamp => {
                self.selection = Some(previous);
                return;
            }
            other => other,
        };
        let branch = match &previous {
            Some(previous) => previous.branch.clone(),
            None if missing => None,
            None => project::git_branch(&project::scope_path(&root, &id)),
        };
        let body = if viewable {
            let editor = previous.and_then(|previous| match previous.body {
                Body::Text(editor) => Some(editor),
                _ => None,
            });
            load_body(&path, editor, window, cx)
        } else {
            Body::None
        };
        self.selection = Some(Selection {
            id,
            stamp,
            branch,
            body,
        });
        cx.notify();
    }

    fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.project_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into();
        let id = project_id(&path);
        self.project = Some(path);
        self.expanded.clear();
        self.expanded.insert(project::DOCS_ID.into());
        self.fingerprint = None;
        self.tree_error = None;
        self.error = None;
        self.selection = None;
        self.refresh_projects();
        self.reload(Some(id), true, cx);
        // Dialog close restores the trigger. Focus the tree after that.
        cx.on_next_frame(window, |this, window, cx| {
            this.tree.update(cx, |tree, cx| tree.focus(window, cx));
        });
        cx.notify();
    }

    fn close_project(&mut self, cx: &mut Context<Self>) {
        self.project = None;
        self.project_name = SharedString::default();
        self.selection = None;
        self.tree_error = None;
        self.error = None;
        self.reload(None, true, cx);
        cx.notify();
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

    // Sidebar

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().colors;
        let message = if self.list_error.is_some() {
            None
        } else if self.projects.is_empty() && self.project.is_none() {
            Some("No projects yet")
        } else if self.visible_projects(cx).is_empty() {
            Some("No matching projects")
        } else {
            None
        };
        let list = v_flex()
            .flex_1()
            .min_h_0()
            .px_2()
            .pb_2()
            .when_some(self.list_error.clone(), |list, error| {
                list.child(Alert::error("list-error", error))
            })
            .when_some(message, |list, message| {
                list.child(
                    Label::new(message)
                        .px_2()
                        .py_1()
                        .text_sm()
                        .text_color(theme.muted_foreground),
                )
            })
            .child(div().flex_1().min_h_0().child(self.render_tree(cx)));

        v_flex()
            .id("sidebar")
            .size_full()
            .min_h_0()
            .bg(theme.sidebar)
            .text_color(theme.sidebar_foreground)
            .child(
                h_flex()
                    .w_full()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .p_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.search)
                                .cleanable(true)
                                .prefix(Icon::new(AssetIcon::Search)),
                        ),
                    )
                    .child(
                        Button::new("new-project")
                            .ghost()
                            .icon(Icon::new(AssetIcon::FolderPlus))
                            .tooltip("New project…")
                            .accessibility_label("New project…")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_new_project(window, cx);
                            })),
                    )
                    .child(
                        Button::new("import-project")
                            .ghost()
                            .icon(Icon::new(AssetIcon::FolderInput))
                            .tooltip("Import project folder…")
                            .accessibility_label("Import project folder…")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.import_project(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .pl_4()
                    .pr_2()
                    .pb_1()
                    .text_xs()
                    .font_medium()
                    .text_color(theme.muted_foreground)
                    .child("Projects"),
            )
            .child(list)
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
            menu.item(
                PopupMenuItem::new("Unlink…").on_click(move |_, window, cx| {
                    let path = path.clone();
                    let name = name.clone();
                    workspace.update(cx, |this, cx| {
                        this.confirm_unlink(name, path, window, cx);
                    });
                }),
            )
        })
    }

    // Main pane

    fn render_main(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.error.clone().or_else(|| self.tree_error.clone());
        let body: AnyElement = if self.project.is_none() {
            self.render_empty(cx)
        } else {
            match self.selection.as_ref().map(|selection| &selection.body) {
                Some(Body::Text(editor)) => Editor::new(editor)
                    .readonly(true)
                    .bordered(false)
                    .h(relative(1.))
                    .into_any_element(),
                Some(Body::Message(message)) => notice(message.clone()),
                _ => notice("Select a file to view it.".into()),
            }
        };
        let mut toolbar = pane_toolbar("main-toolbar", cx).content(self.render_breadcrumb());
        if self.project.is_some() {
            toolbar = toolbar.child(
                Button::new("link-repo")
                    .ghost()
                    .icon(Icon::new(AssetIcon::Link))
                    .label("Link repo…")
                    .on_click(cx.listener(|_, _, window, cx| {
                        pick_folder(window, cx, None);
                    })),
            );
        }
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().background)
            .child(toolbar)
            .when_some(error, |column, error| {
                column.child(Alert::error("workspace-error", error).banner())
            })
            .child(div().flex_1().min_h_0().min_w_0().child(body))
    }

    fn render_breadcrumb(&self) -> Breadcrumb {
        let crumbs = Breadcrumb::new().flex_1().min_w_0();
        if self.project.is_none() {
            return crumbs.child("H2K");
        }
        let crumbs = crumbs.child(self.project_name.clone());
        let Some(id) = self.selection.as_ref().map(|selection| &selection.id) else {
            return crumbs;
        };
        let crumbs = crumbs.child(project::node_scope(id));
        match project::node_rel(id) {
            Some(rel) => crumbs.child(rel.to_string()),
            None => crumbs,
        }
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let (title, message) = if self.projects.is_empty() {
            (
                "No projects yet",
                "A project is a folder of notes and decisions. Linked code stays where it already is.",
            )
        } else {
            ("No project open", "Choose a project in the sidebar.")
        };
        Empty::new()
            .size_full()
            .header(
                EmptyHeader::new()
                    .media(EmptyMedia::new().child(Icon::new(AssetIcon::FolderOpen)))
                    .title(EmptyTitle::new().child(title))
                    .description(EmptyDescription::new().child(message)),
            )
            .content(
                EmptyContent::new().child(
                    Button::new("empty-new-project")
                        .icon(Icon::new(AssetIcon::FolderPlus))
                        .label("New project…")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_new_project(window, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    // Details pane

    fn render_details(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = v_flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().background)
            .child(pane_toolbar("details-toolbar", cx).content("Details"));
        let (Some(root), Some(selection)) = (self.project.as_ref(), self.selection.as_ref()) else {
            return frame.child(notice("Select a file or folder.".into()));
        };
        let id = selection.id.clone();
        let label = self
            .tree
            .read(cx)
            .selected_item()
            .map(|item| item.label.clone())
            .unwrap_or_default();
        let repo = project::repo_root(&id).map(|(missing, path)| (missing, PathBuf::from(path)));
        let path = project::display_path(&project::node_path(root, &id));

        let mut facts = DescriptionList::vertical()
            .columns(1)
            .bordered(false)
            .item("Name", label.clone(), 1)
            .item("Location", project::node_scope(&id), 1)
            .item("Path", path, 1);
        if let Some(branch) = selection.branch.clone() {
            facts = facts.item("Branch", branch, 1);
        }
        let mut body = v_flex()
            .id("details")
            .flex_1()
            .min_h_0()
            .p_4()
            .gap_6()
            .child(facts);

        if let Some((missing, path)) = repo {
            let mut section = v_flex().gap_2();
            if missing {
                section = section.child(Alert::warning("repo-missing", "This folder is missing."));
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
            actions = actions.child(
                Button::new("unlink-repo")
                    .small()
                    .outline()
                    .label("Unlink…")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_unlink(label.clone(), path.clone(), window, cx);
                    })),
            );
            body = body.child(section.child(actions));
        }
        frame.child(body.overflow_y_scrollbar())
    }
}

/// Fixed-width lane so labels line up whether or not an icon is present.
fn slot() -> gpui_kit::Div {
    div()
        .w_4()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
}

/// Toolbar shared by the main and details panes, so their bottom edges form
/// one line.
fn pane_toolbar(id: &'static str, cx: &App) -> Toolbar {
    Toolbar::new(id)
        .w_full()
        .flex_shrink_0()
        .px_4()
        .border_b_1()
        .border_color(cx.theme().border)
}

/// A one-line message in place of content.
fn notice(message: SharedString) -> AnyElement {
    Empty::new()
        .size_full()
        .header(EmptyHeader::new().description(EmptyDescription::new().child(message)))
        .into_any_element()
}

fn file_stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// Read `path` into `editor` (or a new one), or explain why it can't be shown.
fn load_body(
    path: &Path,
    editor: Option<Entity<EditorState>>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Body {
    match project::read_text(path) {
        Ok(FileText::Text(text)) => Body::Text(match editor {
            Some(editor) => {
                editor.update(cx, |state, cx| state.set_value(text.as_str(), window, cx));
                editor
            }
            None => {
                let language = path
                    .extension()
                    .map(|ext| ext.to_string_lossy().into_owned())
                    .unwrap_or_default();
                cx.new(|cx| {
                    EditorState::new(window, cx)
                        .language(language)
                        .line_number(true)
                        .default_value(text)
                })
            }
        }),
        Ok(FileText::Binary) => Body::Message("Binary file. Only text files open here.".into()),
        Ok(FileText::TooLarge(len)) => Body::Message(
            format!(
                "This file is {:.1} MB. Files up to {} MB open here.",
                len as f64 / 1024. / 1024.,
                project::MAX_TEXT_BYTES / 1024 / 1024
            )
            .into(),
        ),
        Err(err) => Body::Message(format!("Couldn’t read the file. {err}").into()),
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

fn fingerprint(projects: &[PathBuf], open: Option<&Path>, roots: &[FileNode]) -> u64 {
    fn walk(nodes: &[FileNode], hasher: &mut impl Hasher) {
        for node in nodes {
            node.name.hash(hasher);
            node.rel.hash(hasher);
            node.is_dir.hash(hasher);
            walk(&node.children, hasher);
        }
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    projects.hash(&mut hasher);
    open.hash(&mut hasher);
    walk(roots, &mut hasher);
    hasher.finish()
}

fn project_id(path: &Path) -> SharedString {
    format!("project\0{}", path.display()).into()
}

fn project_path_of(id: &str) -> Option<PathBuf> {
    id.strip_prefix("project\0").map(PathBuf::from)
}

/// A project row. Open, it holds the project's roots; closed, a stand-in
/// child keeps it expandable.
fn project_item(
    path: &Path,
    open: bool,
    roots: &[FileNode],
    expanded: &HashSet<String>,
) -> TreeItem {
    let id = project_id(path);
    let label = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let mut children = if open {
        tree_items(roots, expanded)
    } else {
        Vec::new()
    };
    if children.is_empty() {
        let label = if open { "Empty" } else { "" };
        children.push(TreeItem::new(format!("{id}\0"), label).disabled(true));
    }
    TreeItem::new(id, label).children(children).expanded(open)
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

/// Each level indents by one icon lane, so a child's chevron sits under its
/// parent's icon.
fn tree_row(entry: &TreeEntry, selected: bool, cx: &App) -> ListItem {
    let item = entry.item();
    let placeholder = item.is_disabled();
    let missing = project::repo_root(&item.id).is_some_and(|(missing, _)| missing);
    let mut chevron = slot();
    if item.is_folder() {
        let icon = if item.is_expanded() {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        chevron = chevron.child(Icon::new(icon).xsmall());
    }
    let mut icon_slot = slot();
    if !placeholder {
        let mut icon = row_icon(item).small();
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
                .when(placeholder, |label| {
                    label.text_color(cx.theme().muted_foreground)
                })
                .child(item.label.clone()),
        );
    if missing {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().warning)
                .child("Missing"),
        );
    }
    ListItem::new(item.id.clone())
        .selected(selected)
        .pl(rems(0.5 + entry.depth() as f32 * 1.25))
        .child(row)
}

fn row_icon(item: &TreeItem) -> Icon {
    if project_path_of(&item.id).is_some() {
        return Icon::new(if item.is_expanded() {
            AssetIcon::FolderOpen
        } else {
            AssetIcon::Folder
        });
    }
    if project::repo_root(&item.id).is_some_and(|(missing, _)| missing) {
        return Icon::new(IconName::CircleAlert);
    }
    if item.id.as_ref() == project::DOCS_ID {
        return Icon::new(IconName::BookOpen);
    }
    Icon::new(if item.is_folder() {
        IconName::Folder
    } else {
        IconName::File
    })
}

struct NewProjectForm {
    name: Entity<InputState>,
    error: Option<SharedString>,
}

impl Render for NewProjectForm {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_form()
            .child(
                field()
                    .label("Name")
                    .description("Created in Documents/h2k-projects")
                    .child(Input::new(&self.name).id("project-name")),
            )
            .when_some(self.error.clone(), |form, error| {
                form.child(field().child(Alert::error("project-name-error", error)))
            })
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let radii = gpui_kit::component::window_corner_radii(window, cx);
        // Panel widths are pixels because the resizable API takes `Pixels`.
        div()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .rounded_bl(radii.bottom_left)
            .rounded_br(radii.bottom_right)
            .child(
                h_resizable("workspace-panes")
                    .child(
                        resizable_panel()
                            .size(px(264.))
                            .size_range(px(200.)..px(420.))
                            .fixed()
                            .child(self.render_sidebar(cx)),
                    )
                    .child(resizable_panel().child(self.render_main(cx)))
                    .child(
                        resizable_panel()
                            .size(px(280.))
                            .size_range(px(220.)..px(440.))
                            .fixed()
                            .visible(self.project.is_some())
                            .child(self.render_details(cx)),
                    ),
            )
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
    fn project_rows_hold_the_open_project_roots() {
        let path = Path::new("/w/Demo");
        let id = project_id(path);
        assert_eq!(project_path_of(&id), Some(path.to_path_buf()));
        assert_eq!(project_path_of("docs/notes"), None);

        let closed = project_item(path, false, &[], &HashSet::new());
        assert_eq!(closed.label.as_ref(), "Demo");
        assert!(closed.is_folder());
        assert!(!closed.is_expanded());

        let roots = vec![dir("docs", "Demo")];
        let open = project_item(path, true, &roots, &HashSet::new());
        assert!(open.is_expanded());
        assert_eq!(open.children[0].id.as_ref(), "docs");
    }

    #[test]
    fn project_search_matches_names_case_insensitively() {
        assert!(project_matches_query("Dev Notes", "dev"));
        assert!(project_matches_query("Dev Notes", "NOTES"));
        assert!(project_matches_query("Dev Notes", ""));
        assert!(!project_matches_query("Dev Notes", "api"));
    }
}
