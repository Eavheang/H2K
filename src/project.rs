//! Project folders under `~/Documents/h2k-projects/`.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROJECT_MD: &str = "\
---
repos: []
---
# Overview

# Objectives

# Scope

# Stack

# Conventions

# Constraints
";

const DIRS: &[&str] = &[
    "notes",
    "knowledge/general",
    "knowledge/requirements",
    "knowledge/technical",
    "knowledge/decisions",
    "knowledge/meetings",
    "tickets",
    ".h2k",
];

const GITIGNORE: &str = ".h2k/\n";

pub(crate) const DOCS_ID: &str = "docs";

pub(crate) struct FileNode {
    pub name: String,
    pub rel: String,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
}

#[derive(Debug)]
pub(crate) enum CreateError {
    EmptyName,
    InvalidName,
    AlreadyExists(String),
    NoHome,
    Io(io::Error),
    Git(String),
}

impl fmt::Display for CreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("Enter a name."),
            Self::InvalidName => f.write_str("Name cannot include / or \\."),
            Self::AlreadyExists(name) => {
                write!(f, "A project named “{name}” already exists.")
            }
            Self::NoHome => f.write_str("Home directory is not set."),
            Self::Io(err) => write!(f, "Couldn’t create the project. {err}"),
            Self::Git(err) => write!(f, "Couldn’t initialize the git repository. {err}"),
        }
    }
}

pub(crate) fn workspace_root() -> Result<PathBuf, CreateError> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty());
    let Some(home) = home else {
        return Err(CreateError::NoHome);
    };
    Ok(PathBuf::from(home).join("Documents").join("h2k-projects"))
}

/// Directory names under `workspace`. A missing directory is an empty list.
/// Files are skipped.
pub(crate) fn list_projects(workspace: &Path) -> io::Result<Vec<String>> {
    if !workspace.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(workspace)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        names.push(name);
    }
    names.sort_by_key(|name| name.to_lowercase());
    Ok(names)
}

/// `~/…` when `path` is under the home directory.
pub(crate) fn display_path(path: &Path) -> String {
    let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) else {
        return path.display().to_string();
    };
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// "Docs", or the linked repo's folder name.
pub(crate) fn node_scope(id: &str) -> String {
    if id == DOCS_ID || id.starts_with("docs/") || id.starts_with("docs\0") {
        return "Docs".to_string();
    }
    if let Some((_, path)) = repo_root(id) {
        return folder_name(Path::new(path));
    }
    if let Some((path, _)) = repo_child(id) {
        return folder_name(Path::new(path));
    }
    "Docs".to_string()
}

/// Create `workspace/name` with the standard layout and a git repo.
/// An existing folder is left untouched.
pub(crate) fn create_project(workspace: &Path, name: &str) -> Result<PathBuf, CreateError> {
    let name = validate_name(name)?;
    fs::create_dir_all(workspace).map_err(CreateError::Io)?;
    let path = workspace.join(&name);
    if path.parent() != Some(workspace) {
        return Err(CreateError::InvalidName);
    }
    if path.try_exists().map_err(CreateError::Io)? {
        return Err(CreateError::AlreadyExists(name));
    }
    if let Err(err) = fs::create_dir(&path) {
        if err.kind() == io::ErrorKind::AlreadyExists {
            return Err(CreateError::AlreadyExists(name));
        }
        return Err(CreateError::Io(err));
    }
    if let Err(err) = write_layout(&path) {
        let _ = fs::remove_dir_all(&path);
        return Err(err);
    }
    Ok(path)
}

fn validate_name(name: &str) -> Result<String, CreateError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CreateError::EmptyName);
    }
    if name == "." || name == ".." || name.len() > 255 {
        return Err(CreateError::InvalidName);
    }
    if name
        .chars()
        .any(|ch| ch.is_control() || ch == '/' || ch == '\\')
    {
        return Err(CreateError::InvalidName);
    }
    Ok(name.to_string())
}

fn write_layout(path: &Path) -> Result<(), CreateError> {
    fs::write(path.join("project.md"), PROJECT_MD).map_err(CreateError::Io)?;
    for dir in DIRS {
        fs::create_dir_all(path.join(dir)).map_err(CreateError::Io)?;
    }
    fs::write(path.join(".gitignore"), GITIGNORE).map_err(CreateError::Io)?;
    git_init(path)
}

fn git_init(path: &Path) -> Result<(), CreateError> {
    let output = Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .map_err(|err| CreateError::Git(err.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    Err(CreateError::Git(if stderr.is_empty() {
        output.status.to_string()
    } else {
        stderr.to_string()
    }))
}

pub(crate) struct RepoLink {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub(crate) enum LinkError {
    AlreadyLinked,
    NotDirectory,
    Io(io::Error),
    Project(String),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyLinked => f.write_str("This folder is already linked."),
            Self::NotDirectory => f.write_str("Choose a folder."),
            Self::Io(err) => write!(f, "Couldn’t update the project. {err}"),
            Self::Project(err) => write!(f, "Couldn’t read project.md. {err}"),
        }
    }
}

struct ProjectDoc {
    repos: Vec<RepoLink>,
    body: String,
}

/// `true` when `path` is inside a git work tree.
pub(crate) fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
}

/// Record `folder` in `project.md`. Does not copy or move it.
pub(crate) fn link_repo(project: &Path, folder: &Path) -> Result<(), LinkError> {
    let folder = canonical_dir(folder)?;
    let mut doc = read_doc(&project.join("project.md"))?;
    if doc.repos.iter().any(|repo| same_path(&repo.path, &folder)) {
        return Err(LinkError::AlreadyLinked);
    }
    doc.repos.push(RepoLink {
        name: folder_name(&folder),
        path: folder,
    });
    write_doc(&project.join("project.md"), &doc)
}

/// Point an existing link at `folder`. The previous folder is left in place.
pub(crate) fn relink_repo(project: &Path, old: &Path, folder: &Path) -> Result<(), LinkError> {
    let folder = canonical_dir(folder)?;
    let mut doc = read_doc(&project.join("project.md"))?;
    if doc
        .repos
        .iter()
        .any(|repo| same_path(&repo.path, &folder) && !same_path(&repo.path, old))
    {
        return Err(LinkError::AlreadyLinked);
    }
    let Some(repo) = doc.repos.iter_mut().find(|repo| same_path(&repo.path, old)) else {
        return Err(LinkError::Project("That repo is no longer linked.".into()));
    };
    repo.name = folder_name(&folder);
    repo.path = folder;
    write_doc(&project.join("project.md"), &doc)
}

/// Drop the link. The folder on disk is not touched.
pub(crate) fn unlink_repo(project: &Path, folder: &Path) -> Result<(), LinkError> {
    let mut doc = read_doc(&project.join("project.md"))?;
    let before = doc.repos.len();
    doc.repos.retain(|repo| !same_path(&repo.path, folder));
    if doc.repos.len() == before {
        return Ok(());
    }
    write_doc(&project.join("project.md"), &doc)
}

fn canonical_dir(folder: &Path) -> Result<PathBuf, LinkError> {
    let folder = folder.canonicalize().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            LinkError::NotDirectory
        } else {
            LinkError::Io(err)
        }
    })?;
    if !folder.is_dir() {
        return Err(LinkError::NotDirectory);
    }
    Ok(folder)
}

fn folder_name(folder: &Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into())
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => normalize(a) == normalize(b),
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn read_doc(path: &Path) -> Result<ProjectDoc, LinkError> {
    let text = fs::read_to_string(path).map_err(LinkError::Io)?;
    let (frontmatter, body) = split_frontmatter(&text).map_err(LinkError::Project)?;
    let repos = match frontmatter {
        Some(frontmatter) => parse_repos(frontmatter).map_err(LinkError::Project)?,
        None => Vec::new(),
    };
    Ok(ProjectDoc {
        repos,
        body: body.to_string(),
    })
}

fn write_doc(path: &Path, doc: &ProjectDoc) -> Result<(), LinkError> {
    let mut text = String::from("---\n");
    if doc.repos.is_empty() {
        text.push_str("repos: []\n");
    } else {
        text.push_str("repos:\n");
        for repo in &doc.repos {
            text.push_str("  - name: ");
            text.push_str(&quote(&repo.name));
            text.push_str("\n    path: ");
            text.push_str(&quote(&repo.path.display().to_string()));
            text.push('\n');
        }
    }
    text.push_str("---\n");
    text.push_str(&doc.body);
    fs::write(path, text).map_err(LinkError::Io)
}

fn split_frontmatter(text: &str) -> Result<(Option<&str>, &str), String> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok((None, text));
    };
    let Some(end) = rest.find("\n---\n") else {
        return Err("frontmatter is missing its closing ---".into());
    };
    Ok((Some(&rest[..end]), &rest[end + "\n---\n".len()..]))
}

fn parse_repos(frontmatter: &str) -> Result<Vec<RepoLink>, String> {
    let lines: Vec<&str> = frontmatter
        .lines()
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() || lines == ["repos: []"] {
        return Ok(Vec::new());
    }
    if lines.first().copied() != Some("repos:") {
        return Err("frontmatter must be a repos list".into());
    }
    let mut repos = Vec::new();
    let mut lines = lines[1..].iter();
    while let Some(name_line) = lines.next() {
        let Some(name) = name_line.strip_prefix("  - name: ") else {
            return Err("expected a repo name".into());
        };
        let Some(path_line) = lines.next() else {
            return Err("expected a repo path".into());
        };
        let Some(path) = path_line.strip_prefix("    path: ") else {
            return Err("expected a repo path".into());
        };
        repos.push(RepoLink {
            name: unquote(name)?,
            path: PathBuf::from(unquote(path)?),
        });
    }
    Ok(repos)
}

fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' | '"' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\n"),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn unquote(value: &str) -> Result<String, String> {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Ok(value.to_string());
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            _ => return Err("invalid escape in project.md".into()),
        }
    }
    Ok(out)
}

/// A repo root id is `repo\0ok\0{path}` or `repo\0missing\0{path}`.
/// `(missing, path)`.
pub(crate) fn repo_root(id: &str) -> Option<(bool, &str)> {
    let rest = id.strip_prefix("repo\0")?;
    let (kind, path) = rest.split_once('\0')?;
    if path.contains('\0') {
        return None;
    }
    match kind {
        "missing" => Some((true, path)),
        "ok" => Some((false, path)),
        _ => None,
    }
}

fn repo_id(path: &Path, missing: bool) -> String {
    let kind = if missing { "missing" } else { "ok" };
    format!("repo\0{kind}\0{}", path.display())
}

fn child_id(parent_id: &str, name: &str) -> String {
    if repo_root(parent_id).is_some() {
        format!("{parent_id}\0{name}")
    } else if let Some((path, rel)) = repo_child(parent_id) {
        format!("repo\0ok\0{path}\0{rel}/{name}")
    } else {
        format!("{parent_id}/{name}")
    }
}

fn repo_child(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix("repo\0ok\0")?;
    let (path, rel) = rest.split_once('\0')?;
    if rel.is_empty() {
        None
    } else {
        Some((path, rel))
    }
}

pub(crate) fn node_path(project: &Path, id: &str) -> PathBuf {
    if let Some((_, path)) = repo_root(id) {
        return PathBuf::from(path);
    }
    if let Some((path, rel)) = repo_child(id) {
        return PathBuf::from(path).join(rel);
    }
    match id.strip_prefix("docs/") {
        Some(rel) => project.join(rel),
        None => project.to_path_buf(),
    }
}

/// Docs folder first, then each linked repo. A missing repo is not expandable.
/// `.git` is omitted. `expanded` holds node ids.
pub(crate) fn read_project_tree(
    project: &Path,
    expanded: &HashSet<String>,
) -> io::Result<Vec<FileNode>> {
    let doc =
        read_doc(&project.join("project.md")).map_err(|err| io::Error::other(err.to_string()))?;
    let label = project
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let mut roots = vec![read_dir_node(project, DOCS_ID, &label, expanded)?];
    for repo in doc.repos {
        if repo.path.is_dir() {
            match read_dir_node(project, &repo_id(&repo.path, false), &repo.name, expanded) {
                Ok(node) => roots.push(node),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    roots.push(missing_node(&repo));
                }
                Err(err) => return Err(err),
            }
        } else {
            roots.push(missing_node(&repo));
        }
    }
    Ok(roots)
}

fn missing_node(repo: &RepoLink) -> FileNode {
    FileNode {
        name: repo.name.clone(),
        rel: repo_id(&repo.path, true),
        is_dir: false,
        children: Vec::new(),
    }
}

fn read_dir_node(
    project: &Path,
    id: &str,
    label: &str,
    expanded: &HashSet<String>,
) -> io::Result<FileNode> {
    let children = if expanded.contains(id) {
        read_children(project, id, expanded)?
    } else {
        Vec::new()
    };
    Ok(FileNode {
        name: label.to_string(),
        rel: id.to_string(),
        is_dir: true,
        children,
    })
}

fn read_children(
    project: &Path,
    parent_id: &str,
    expanded: &HashSet<String>,
) -> io::Result<Vec<FileNode>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir(node_path(project, parent_id))? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name == ".git" {
            continue;
        }
        let id = child_id(parent_id, &name);
        let is_dir = entry.file_type()?.is_dir();
        let children = if is_dir && expanded.contains(&id) {
            match read_children(project, &id, expanded) {
                Ok(children) => children,
                Err(err) if err.kind() == io::ErrorKind::NotFound => Vec::new(),
                Err(err) => return Err(err),
            }
        } else {
            Vec::new()
        };
        nodes.push(FileNode {
            name,
            rel: id,
            is_dir,
            children,
        });
    }
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("h2k-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn names(nodes: &[FileNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.name.as_str()).collect()
    }

    #[test]
    fn list_projects_returns_directories_sorted() {
        let workspace = TempDir::new();
        fs::create_dir(workspace.0.join("Beta")).unwrap();
        fs::create_dir(workspace.0.join("alpha")).unwrap();
        fs::write(workspace.0.join("notes.txt"), "no").unwrap();
        assert_eq!(
            list_projects(&workspace.0).unwrap(),
            vec!["alpha".to_string(), "Beta".to_string()]
        );
        assert!(
            list_projects(&workspace.0.join("missing"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn create_project_writes_layout_and_git_ignore() {
        let workspace = TempDir::new();
        let path = create_project(&workspace.0, " Demo ").unwrap();
        assert_eq!(path, workspace.0.join("Demo"));

        let md = fs::read_to_string(path.join("project.md")).unwrap();
        assert_eq!(md, PROJECT_MD);
        assert_eq!(
            fs::read_to_string(path.join(".gitignore")).unwrap(),
            GITIGNORE
        );
        for dir in DIRS {
            assert!(path.join(dir).is_dir(), "{dir}");
        }

        let mut top: Vec<_> = fs::read_dir(&path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        top.sort();
        assert_eq!(
            top,
            vec![
                ".git",
                ".gitignore",
                ".h2k",
                "knowledge",
                "notes",
                "project.md",
                "tickets",
            ]
        );

        let mut knowledge: Vec<_> = fs::read_dir(path.join("knowledge"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        knowledge.sort();
        assert_eq!(
            knowledge,
            vec![
                "decisions",
                "general",
                "meetings",
                "requirements",
                "technical",
            ]
        );

        let ignored = Command::new("git")
            .args(["-C", path.to_str().unwrap(), "check-ignore", "-q", ".h2k"])
            .status()
            .unwrap();
        assert!(ignored.success());
        let tracked = Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap(),
                "check-ignore",
                "-q",
                "project.md",
            ])
            .status()
            .unwrap();
        assert!(!tracked.success());
        let inside = Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap(),
                "rev-parse",
                "--is-inside-work-tree",
            ])
            .output()
            .unwrap();
        assert!(inside.status.success());
        assert_eq!(String::from_utf8_lossy(&inside.stdout).trim(), "true");
    }

    #[test]
    fn create_project_rejects_an_existing_name_without_writing() {
        let workspace = TempDir::new();
        let path = create_project(&workspace.0, "Demo").unwrap();
        fs::write(path.join("project.md"), "custom").unwrap();
        fs::write(path.join("keep.txt"), "keep").unwrap();

        let err = create_project(&workspace.0, "Demo").unwrap_err();
        assert_eq!(err.to_string(), "A project named “Demo” already exists.");
        assert_eq!(
            fs::read_to_string(path.join("project.md")).unwrap(),
            "custom"
        );
        assert_eq!(fs::read_to_string(path.join("keep.txt")).unwrap(), "keep");
    }

    #[test]
    fn create_project_rejects_a_path_name_before_creating_the_workspace() {
        let missing = std::env::temp_dir().join(format!(
            "h2k-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let err = create_project(&missing, "a/b").unwrap_err();
        assert!(matches!(err, CreateError::InvalidName));
        assert!(!missing.exists());
        assert!(matches!(
            create_project(&TempDir::new().0, "  ").unwrap_err(),
            CreateError::EmptyName
        ));
        assert!(matches!(
            create_project(&TempDir::new().0, "..").unwrap_err(),
            CreateError::InvalidName
        ));
    }

    #[test]
    fn read_project_tree_follows_files_added_and_removed_on_disk() {
        let workspace = TempDir::new();
        let path = create_project(&workspace.0, "Demo").unwrap();
        let mut expanded = HashSet::from(["docs".into(), "docs/knowledge".into()]);
        let open = read_project_tree(&path, &expanded).unwrap();
        assert_eq!(open[0].name, "Demo");
        assert_eq!(
            names(&open[0].children),
            vec![
                ".h2k",
                "knowledge",
                "notes",
                "tickets",
                ".gitignore",
                "project.md",
            ]
        );
        assert!(open[0].children.iter().all(|node| node.name != ".git"));
        let knowledge = open[0]
            .children
            .iter()
            .find(|node| node.name == "knowledge")
            .unwrap();
        assert_eq!(
            names(&knowledge.children),
            vec![
                "decisions",
                "general",
                "meetings",
                "requirements",
                "technical",
            ]
        );

        expanded.insert("docs/notes".into());
        fs::write(path.join("notes/a.md"), "hi").unwrap();
        let with_note = read_project_tree(&path, &expanded).unwrap();
        let notes = with_note[0]
            .children
            .iter()
            .find(|node| node.name == "notes")
            .unwrap();
        assert_eq!(names(&notes.children), vec!["a.md"]);

        fs::remove_file(path.join("notes/a.md")).unwrap();
        let gone = read_project_tree(&path, &expanded).unwrap();
        let notes = gone[0]
            .children
            .iter()
            .find(|node| node.name == "notes")
            .unwrap();
        assert!(notes.children.is_empty());
    }

    #[test]
    fn link_unlink_and_relink_leave_the_code_in_place() {
        let workspace = TempDir::new();
        let project = create_project(&workspace.0, "Demo").unwrap();
        let code = workspace.0.join("backend");
        fs::create_dir(&code).unwrap();
        fs::write(code.join("main.rs"), "fn main() {}").unwrap();
        let md = project.join("project.md");
        let mut text = fs::read_to_string(&md).unwrap();
        text.push_str("Keep this.\n");
        fs::write(&md, &text).unwrap();

        assert!(!is_git_repo(&code));
        link_repo(&project, &code).unwrap();
        let linked = fs::read_to_string(&md).unwrap();
        assert!(linked.contains("Keep this."));
        assert!(linked.contains("# Constraints"));
        assert!(linked.contains("backend"));
        assert!(code.join("main.rs").is_file());

        let before = fs::read_to_string(&md).unwrap();
        let err = link_repo(&project, &code).unwrap_err();
        assert!(matches!(err, LinkError::AlreadyLinked));
        assert_eq!(fs::read_to_string(&md).unwrap(), before);

        let link = workspace.0.join("link");
        std::os::unix::fs::symlink(&code, &link).unwrap();
        assert!(matches!(
            link_repo(&project, &link).unwrap_err(),
            LinkError::AlreadyLinked
        ));

        let canonical = code.canonicalize().unwrap();
        let mut expanded = HashSet::new();
        expanded.insert(format!("repo\0ok\0{}", canonical.display()));
        fs::write(code.join("new.txt"), "a").unwrap();
        let tree = read_project_tree(&project, &expanded).unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[1].name, "backend");
        assert!(tree[1].children.iter().any(|node| node.name == "new.txt"));
        fs::remove_file(code.join("new.txt")).unwrap();
        let tree = read_project_tree(&project, &expanded).unwrap();
        assert!(tree[1].children.iter().all(|node| node.name != "new.txt"));

        fs::remove_dir_all(&code).unwrap();
        let tree = read_project_tree(&project, &HashSet::new()).unwrap();
        assert_eq!(tree[1].name, "backend");
        assert!(!tree[1].is_dir);
        assert_eq!(
            repo_root(&tree[1].rel),
            Some((true, canonical.display().to_string().as_str()))
        );

        let other = workspace.0.join("frontend");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("app.rs"), "fn app() {}").unwrap();
        relink_repo(&project, &canonical, &other).unwrap();
        let tree = read_project_tree(&project, &HashSet::new()).unwrap();
        assert_eq!(tree[1].name, "frontend");
        assert!(other.join("app.rs").is_file());

        unlink_repo(&project, &other).unwrap();
        let after = fs::read_to_string(&md).unwrap();
        assert!(after.contains("repos: []"));
        assert!(after.contains("Keep this."));
        assert!(other.join("app.rs").is_file());
        assert_eq!(
            read_project_tree(&project, &HashSet::new()).unwrap().len(),
            1
        );
    }
}
