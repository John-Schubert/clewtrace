//! Watching a project folder with OS file events, so changes made outside the app (a git
//! pull, another editor) reach the open project (task 2.10b). Nothing runs while no files
//! change: the watcher thread sleeps in the OS until an event comes, and the burst timer
//! exists only between the first event of a burst and its end (CT-EFF-008).

// @begin CT-EFF-008
// Windows has no watcher until task 2.10c, so the burst and path code is unused there.
#![cfg_attr(windows, allow(dead_code, unused_imports))]
// @end

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

// @implements CT-EFF-008
/// How long the folder must be still before a burst of events counts as ended.
pub const QUIET: Duration = Duration::from_millis(200);

// @implements CT-EFF-008, CT-EFF-007
/// The longest a burst is gathered before it is passed on, so a long `git checkout` shows
/// progress instead of nothing until it ends.
pub const LONGEST_BURST: Duration = Duration::from_secs(2);

// @implements CT-EFF-008, CT-USE-007
/// Something that stops the watcher seeing every change, with how to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trouble {
    pub what: String,
    pub help: String,
}

// @implements CT-EFF-008, CT-EFF-007, CT-USE-009
/// The changes of one burst of file events.
#[derive(Debug, Default)]
pub struct Batch {
    /// Project-relative paths with `/` separators, sorted, with no hidden paths, temp
    /// files, or paths under a folder holding a `CACHEDIR.TAG`.
    pub paths: Vec<String>,
    /// A folder was added, removed, or renamed, or the OS dropped events: what changed is
    /// not known file by file, so the whole project should be read again.
    pub everything: bool,
    /// What went wrong while watching.
    pub trouble: Vec<Trouble>,
    /// New folders to watch, on systems that watch folder by folder.
    folders: Vec<PathBuf>,
}

// @implements CT-EFF-008, CT-EFF-007
/// What one file event means, as the watcher passes it to the burst thread.
#[derive(Debug)]
enum Seen {
    Paths(Vec<String>),
    Everything,
    /// A new folder, by its full path.
    Folder(PathBuf),
    Trouble(Trouble),
}

// @implements CT-EFF-008, CT-USE-007
/// The report when the OS has no file watches left. Clewtrace never falls back to polling,
/// so this says how to raise the limit.
fn out_of_watches() -> Trouble {
    Trouble {
        what: "the system ran out of file watches, so some changes made outside Clewtrace \
               are not seen"
            .to_string(),
        help: "Raise the limit: run `sudo sysctl fs.inotify.max_user_watches=524288`, and to \
               keep it, put the line `fs.inotify.max_user_watches=524288` in a file in \
               /etc/sysctl.d/. Then reopen the project. Until then, reopen the project to \
               see changes made outside Clewtrace."
            .to_string(),
    }
}

// @implements CT-EFF-008, CT-EFF-007
/// The project-relative form of `path`, or `None` when the project does not read it: the
/// root itself, a path outside the root, a hidden path (such as `.git`, `.clewtrace`, or a
/// save's temp file), or a path under a folder holding a `CACHEDIR.TAG`. These are the
/// rules `index::sysml_files` reads model files by.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    let mut dir = root.to_path_buf();
    for part in rest.components() {
        let Component::Normal(name) = part else {
            return None;
        };
        let name = name.to_str()?;
        if name.starts_with('.') || dir.join("CACHEDIR.TAG").is_file() {
            return None;
        }
        dir.push(name);
        parts.push(name);
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

// @implements CT-EFF-008
/// `dir` and the folders under it that the project reads files from, by the rules of
/// `relative`.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn folders_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if dir.join("CACHEDIR.TAG").is_file() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if !hidden && entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(entry.path());
            }
        }
        found.push(dir);
    }
    found
}

mod burst;
use burst::debounce;

// @implements CT-EFF-008
/// A running watch of a project folder. Dropping it stops the OS watch; its thread then
/// ends by itself, after any batch it is passing on.
pub struct ProjectWatcher {
    #[cfg(not(windows))]
    _watcher: std::sync::Arc<std::sync::Mutex<notify::RecommendedWatcher>>,
}

// @implements CT-EFF-008, CT-EFF-007, CT-USE-009
/// Watches the project under `root` with the OS's own file events and calls `on_change`
/// with each burst of changes, on a thread of its own. Returns `None` on Windows, which is
/// not watched yet (task 2.10c), and the trouble when the watch cannot start.
///
/// Linux watches each folder the project reads, adding new folders as they appear, and
/// reports running out of watches with how to raise the limit; it never polls. macOS
/// watches the root with FSEvents, which covers the folders under it.
#[cfg(not(windows))]
pub fn watch_project(
    root: &Path,
    mut on_change: impl FnMut(Batch) + Send + 'static,
) -> Result<Option<ProjectWatcher>, Trouble> {
    use std::sync::{Arc, Mutex, mpsc};
    let failed = |e: &dyn std::fmt::Display| Trouble {
        what: format!("could not watch the project folder for changes: {e}"),
        help: "Reopen the project to see changes made outside Clewtrace.".to_string(),
    };
    let root = std::path::absolute(root).map_err(|e| failed(&e))?;
    let (tx, rx) = mpsc::channel();
    let event_root = root.clone();
    let handler = move |event: notify::Result<notify::Event>| {
        for s in seen(&event_root, event) {
            let _ = tx.send(s);
        }
    };
    let watcher = notify::recommended_watcher(handler).map_err(|e| failed(&e))?;
    let watcher = Arc::new(Mutex::new(watcher));
    let first = if cfg!(target_os = "macos") {
        add_watches(
            &watcher,
            std::slice::from_ref(&root),
            notify::RecursiveMode::Recursive,
        )
    } else {
        add_watches(
            &watcher,
            &folders_under(&root),
            notify::RecursiveMode::NonRecursive,
        )
    };
    let weak = Arc::downgrade(&watcher);
    let run = move || {
        if let Some(trouble) = first {
            on_change(Batch {
                trouble: vec![trouble],
                ..Batch::default()
            });
        }
        debounce(&rx, QUIET, LONGEST_BURST, |mut batch| {
            let folders = std::mem::take(&mut batch.folders);
            if let Some(watcher) = weak.upgrade().filter(|_| !cfg!(target_os = "macos")) {
                let new: Vec<PathBuf> = folders.iter().flat_map(|f| folders_under(f)).collect();
                let mode = notify::RecursiveMode::NonRecursive;
                if let Some(trouble) = add_watches(&watcher, &new, mode) {
                    batch.trouble.push(trouble);
                }
            }
            on_change(batch);
        });
    };
    std::thread::Builder::new()
        .name("clew-watch".into())
        .spawn(run)
        .map_err(|e| failed(&e))?;
    Ok(Some(ProjectWatcher { _watcher: watcher }))
}

// @implements CT-EFF-008
/// Windows is not watched yet: notify's Windows watcher wakes every 100 ms, which
/// CT-EFF-008 rules out. Task 2.10c adds one that waits on the OS.
#[cfg(windows)]
pub fn watch_project(
    _root: &Path,
    _on_change: impl FnMut(Batch) + Send + 'static,
) -> Result<Option<ProjectWatcher>, Trouble> {
    Ok(None)
}

// @implements CT-EFF-008, CT-USE-007
/// Watches each of `folders`. Stops at the first sign that the OS has no watches left and
/// returns that trouble; a folder that went away in the meantime is skipped.
#[cfg(not(windows))]
fn add_watches(
    watcher: &std::sync::Mutex<notify::RecommendedWatcher>,
    folders: &[PathBuf],
    mode: notify::RecursiveMode,
) -> Option<Trouble> {
    use notify::Watcher;
    let mut watcher = watcher
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for folder in folders {
        if let Err(e) = watcher.watch(folder, mode)
            && matches!(e.kind, notify::ErrorKind::MaxFilesWatch)
        {
            return Some(out_of_watches());
        }
    }
    None
}

// @implements CT-EFF-008, CT-EFF-007, CT-USE-007
/// What one notify event means for the project under `root`. Reading a file, and a change
/// to its times or permissions only, mean nothing: the app's own reads must not count as
/// changes, or each reload would cause the next.
#[cfg(not(windows))]
fn seen(root: &Path, event: notify::Result<notify::Event>) -> Vec<Seen> {
    use notify::event::{CreateKind, EventKind, ModifyKind, RemoveKind};
    let event = match event {
        Ok(event) => event,
        Err(e) if matches!(e.kind, notify::ErrorKind::MaxFilesWatch) => {
            return vec![Seen::Trouble(out_of_watches())];
        }
        Err(e) => {
            return vec![Seen::Trouble(Trouble {
                what: format!("watching the project folder failed: {e}"),
                help: "Reopen the project to see changes made outside Clewtrace.".to_string(),
            })];
        }
    };
    if event.need_rescan() {
        return vec![Seen::Everything];
    }
    let mut found = Vec::new();
    let mut files = Vec::new();
    for path in &event.paths {
        let Some(rel) = relative(root, path) else {
            continue;
        };
        match event.kind {
            EventKind::Access(_) | EventKind::Modify(ModifyKind::Metadata(_)) => {}
            EventKind::Create(CreateKind::Folder) => found.push(Seen::Folder(path.clone())),
            EventKind::Remove(RemoveKind::Folder) => found.push(Seen::Everything),
            _ if path.is_dir() => found.push(Seen::Folder(path.clone())),
            // A folder renamed away is gone and cannot be told from a file by looking.
            EventKind::Modify(ModifyKind::Name(_))
                if !path.exists() && Path::new(&rel).extension().is_none() =>
            {
                found.push(Seen::Everything);
            }
            _ => files.push(rel),
        }
    }
    if !files.is_empty() {
        found.push(Seen::Paths(files));
    }
    found
}

// @verifies CT-EFF-008, CT-EFF-007, CT-USE-009
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // @verifies CT-EFF-008
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("clew-watch-tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::path::absolute(dir).unwrap()
    }

    // @verifies CT-EFF-008, CT-EFF-007
    #[test]
    fn paths_are_project_relative_and_hidden_or_cache_paths_are_dropped() {
        let root = folder("relative");
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/CACHEDIR.TAG"), "").unwrap();
        let rel = |p: &str| relative(&root, &root.join(p));
        assert_eq!(rel("model/a.sysml").as_deref(), Some("model/a.sysml"));
        assert_eq!(rel("trace.toml").as_deref(), Some("trace.toml"));
        assert_eq!(rel(".git/index"), None);
        assert_eq!(rel("model/.a.sysml.7.clewtmp"), None);
        assert_eq!(rel("target/debug/x.rs"), None);
        assert_eq!(rel(""), None);
        assert_eq!(relative(&root, Path::new("/elsewhere/a.sysml")), None);
    }

    // @verifies CT-EFF-008
    #[test]
    fn the_watched_folders_are_the_ones_model_files_are_read_from() {
        let root = folder("folders");
        for dir in ["model/deep", ".git/objects", "target/debug", "src"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("target/CACHEDIR.TAG"), "").unwrap();
        let mut found = folders_under(&root);
        found.sort();
        let want: Vec<PathBuf> = ["", "model", "model/deep", "src"]
            .iter()
            .map(|p| {
                if p.is_empty() {
                    root.clone()
                } else {
                    root.join(p)
                }
            })
            .collect();
        assert_eq!(found, want);
    }

    // @verifies CT-EFF-008
    #[test]
    fn running_out_of_watches_says_how_to_raise_the_limit() {
        let trouble = out_of_watches();
        assert!(trouble.what.contains("file watches"), "{}", trouble.what);
        assert!(
            trouble.help.contains("fs.inotify.max_user_watches"),
            "{}",
            trouble.help
        );
        assert!(trouble.help.contains("reopen"), "{}", trouble.help);
    }

    // @verifies CT-EFF-008
    #[cfg(not(windows))]
    #[test]
    fn the_watcher_is_the_os_event_watcher_never_the_polling_one() {
        use notify::{RecommendedWatcher, Watcher, WatcherKind};
        let kind = RecommendedWatcher::kind();
        assert_ne!(kind, WatcherKind::PollWatcher);
        #[cfg(target_os = "linux")]
        assert_eq!(kind, WatcherKind::Inotify);
        #[cfg(target_os = "macos")]
        assert_eq!(kind, WatcherKind::Fsevent);
    }

    // @verifies CT-EFF-008, CT-EFF-007
    #[cfg(not(windows))]
    #[test]
    fn reading_a_file_or_touching_it_is_not_a_change() {
        use notify::event::{AccessKind, EventKind, MetadataKind, ModifyKind};
        let root = folder("seen");
        let event = |kind| notify::Event::new(kind).add_path(root.join("model/a.sysml"));
        assert!(seen(&root, Ok(event(EventKind::Access(AccessKind::Any)))).is_empty());
        let touched = EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any));
        assert!(seen(&root, Ok(event(touched))).is_empty());
        let written = EventKind::Modify(ModifyKind::Any);
        assert!(
            matches!(&seen(&root, Ok(event(written)))[..], [Seen::Paths(p)] if p == &["model/a.sysml"])
        );
    }
}
