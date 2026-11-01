//! Saving a file so that a crash or power loss leaves the old or the new content, never a
//! torn file (CT-USE-008).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// @implements CT-USE-008, CT-SEC-003
/// The temp file for `target`: a hidden sibling in the same folder, so the final rename
/// stays on one file system and the system temp folder is never used.
fn temp_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    target.with_file_name(format!(".{name}.{}{TEMP_SUFFIX}", std::process::id()))
}

// @implements CT-USE-008
/// The end of every temp file name `write_atomic` makes. A project's `.gitignore` should
/// list `*.clewtmp` (docs/MODELING.md).
pub const TEMP_SUFFIX: &str = ".clewtmp";

// @implements CT-USE-008
/// Replaces `target` with `bytes`. The bytes go to a temp file in the same folder, which is
/// flushed to disk and then renamed over the target, so a crash leaves the old content or
/// the new, never a mix. On any failure the temp file is removed and the target is as it
/// was. An existing file keeps its permissions.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = temp_path(target);
    let result = write_then_rename(target, &temp, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

// @implements CT-USE-008
/// The steps of `write_atomic`, split out so the caller can clean up after any of them.
fn write_then_rename(target: &Path, temp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(temp)?;
    file.write_all(bytes)?;
    file.flush()?;
    if let Ok(meta) = fs::metadata(target) {
        file.set_permissions(meta.permissions())?;
    }
    file.sync_all()?;
    drop(file);
    fs::rename(temp, target)?;
    sync_folder(target)
}

// @implements CT-USE-008
/// Flushes the folder that holds `target`, so the rename itself survives a power loss and
/// the saved file does not come back as the old one. Windows makes the rename durable with
/// the file and has no folder to flush.
#[cfg(unix)]
fn sync_folder(target: &Path) -> std::io::Result<()> {
    let folder = match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    fs::File::open(folder)?.sync_all()
}

// @implements CT-USE-008
/// Windows has no folder to flush; see the Unix version.
#[cfg(not(unix))]
fn sync_folder(_target: &Path) -> std::io::Result<()> {
    Ok(())
}

// @implements CT-USE-008
/// Removes the temp files that saves killed part way left in the project under `root`, and
/// returns their project-relative paths, sorted. It looks in the folders model files are
/// read from: not hidden ones, and not those holding a `CACHEDIR.TAG`. A temp file changed
/// less than `min_age` ago is left, since another Clewtrace window may still be writing it.
/// What cannot be read or removed is skipped: a leftover temp file is never read as a model
/// file, so it does no harm until the next open.
pub fn remove_stale_temps(root: &Path, min_age: Duration) -> Vec<String> {
    let mut removed = Vec::new();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, rel)) = pending.pop() {
        if dir.join("CACHEDIR.TAG").is_file() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && !name.starts_with('.') {
                pending.push((entry.path(), child));
            } else if kind.is_file()
                && name.starts_with('.')
                && name.ends_with(TEMP_SUFFIX)
                && is_older_than(&entry.path(), min_age)
                && fs::remove_file(entry.path()).is_ok()
            {
                removed.push(child);
            }
        }
    }
    removed.sort();
    removed
}

// @implements CT-USE-008
/// Whether the file at `path` was last changed at least `age` ago. A clock that went back
/// counts as fresh, so a file is never removed on a guess.
fn is_older_than(path: &Path, age: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|since| since >= age)
}

// @verifies CT-USE-008, CT-SEC-003
#[cfg(test)]
mod tests {
    use super::{TEMP_SUFFIX, remove_stale_temps, write_atomic};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("clewtrace-io-tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &PathBuf) -> Vec<String> {
        let mut all: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        all.sort();
        all
    }

    // @verifies CT-USE-008
    #[test]
    fn replaces_the_content_and_leaves_no_temp_file() {
        let dir = folder("replaces");
        let target = dir.join("a.sysml");
        fs::write(&target, "old").unwrap();
        write_atomic(&target, b"new text").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new text");
        assert_eq!(names(&dir), ["a.sysml"]);
    }

    // @verifies CT-USE-008
    #[test]
    fn writes_a_file_that_is_not_there_yet() {
        let dir = folder("creates");
        let target = dir.join("b.sysml");
        write_atomic(&target, b"first").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "first");
    }

    // @verifies CT-USE-008
    #[test]
    fn a_failed_save_keeps_the_old_content_and_leaves_no_temp_file() {
        let dir = folder("fails");
        // The target is a folder, so the final rename cannot replace it.
        let target = dir.join("c.sysml");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep.txt"), "old").unwrap();
        assert!(write_atomic(&target, b"new").is_err());
        assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "old");
        assert_eq!(names(&dir), ["c.sysml"]);
    }

    // @verifies CT-USE-008
    #[test]
    fn a_missing_folder_is_an_error_not_a_temp_file_elsewhere() {
        let dir = folder("missing");
        let target = dir.join("no-such-folder").join("d.sysml");
        assert!(write_atomic(&target, b"x").is_err());
        assert_eq!(names(&dir), Vec::<String>::new());
    }

    // @verifies CT-USE-008, CT-SEC-003
    #[test]
    fn the_temp_file_is_in_the_targets_own_folder() {
        let dir = folder("sibling");
        let target = dir.join("e.sysml");
        fs::write(&target, "old").unwrap();
        let seen = super::temp_path(&target);
        assert_eq!(seen.parent(), Some(dir.as_path()));
        assert_ne!(seen, target);
    }

    // @verifies CT-USE-008
    #[cfg(unix)]
    #[test]
    fn keeps_the_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("mode");
        let target = dir.join("f.sysml");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&target, b"new").unwrap();
        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    // @verifies CT-USE-008
    /// Makes `path` look last written `ago` in the past.
    fn age(path: &PathBuf, ago: Duration) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - ago).unwrap();
    }

    // @verifies CT-USE-008
    #[test]
    fn stale_temp_files_are_removed_in_every_model_folder() {
        let dir = folder("stale");
        fs::create_dir_all(dir.join("model/deep")).unwrap();
        let old_top = dir.join(format!(".a.sysml.11{TEMP_SUFFIX}"));
        let old_deep = dir.join(format!("model/deep/.b.sysml.12{TEMP_SUFFIX}"));
        for path in [&old_top, &old_deep] {
            fs::write(path, "torn").unwrap();
            age(path, Duration::from_secs(120));
        }
        fs::write(dir.join("model/deep/b.sysml"), "kept").unwrap();
        let removed = remove_stale_temps(&dir, Duration::from_secs(60));
        assert_eq!(
            removed,
            [".a.sysml.11.clewtmp", "model/deep/.b.sysml.12.clewtmp"]
        );
        assert!(!old_top.exists() && !old_deep.exists());
        assert_eq!(names(&dir.join("model/deep")), ["b.sysml"]);
    }

    // @verifies CT-USE-008
    #[test]
    fn a_fresh_temp_file_is_left_for_the_save_that_may_still_be_writing_it() {
        let dir = folder("fresh");
        let fresh = dir.join(format!(".a.sysml.13{TEMP_SUFFIX}"));
        fs::write(&fresh, "being written").unwrap();
        assert!(remove_stale_temps(&dir, Duration::from_secs(60)).is_empty());
        assert!(fresh.exists());
    }

    // @verifies CT-USE-008
    #[test]
    fn only_hidden_clewtmp_files_are_removed() {
        let dir = folder("only-temps");
        for name in ["a.clewtmp", ".a.sysml", "notes.txt"] {
            fs::write(dir.join(name), "x").unwrap();
            age(&dir.join(name), Duration::from_secs(120));
        }
        // Hidden and cache folders are not model folders, so nothing there is touched.
        for sub in [".git", "target"] {
            fs::create_dir_all(dir.join(sub)).unwrap();
            fs::write(dir.join(sub).join(format!(".x.1{TEMP_SUFFIX}")), "x").unwrap();
            age(
                &dir.join(sub).join(format!(".x.1{TEMP_SUFFIX}")),
                Duration::from_secs(120),
            );
        }
        fs::write(dir.join("target/CACHEDIR.TAG"), "").unwrap();
        assert!(remove_stale_temps(&dir, Duration::from_secs(60)).is_empty());
        assert_eq!(
            names(&dir),
            [".a.sysml", ".git", "a.clewtmp", "notes.txt", "target"]
        );
    }

    // @verifies CT-USE-008
    #[test]
    fn a_missing_folder_removes_nothing() {
        let dir = folder("gone").join("missing");
        assert!(remove_stale_temps(&dir, Duration::from_secs(60)).is_empty());
    }
}
