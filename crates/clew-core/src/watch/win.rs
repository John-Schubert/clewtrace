//! The Windows project watcher (task 2.10c). One `ReadDirectoryChangesW` on the root, over
//! the whole tree, with overlapped I/O. Its thread waits in `WaitForMultipleObjects` with no
//! timeout on two events: the read finishing and the watch being stopped. Nothing runs
//! while no files change (CT-EFF-008). notify is not used here: its Windows thread wakes
//! every 100 ms.
//!
//! Decoding the OS's change records and what they mean is plain Rust, so its tests run on
//! every system. Only the OS calls are Windows-only.

use std::path::Path;

use super::{Seen, relative};

// @begin CT-EFF-008
// The FILE_ACTION values of a change record, as Windows defines them. A test on Windows
// checks them against windows-sys.
const ADDED: u32 = 1;
const REMOVED: u32 = 2;
const MODIFIED: u32 = 3;
const RENAMED_OLD_NAME: u32 = 4;
const RENAMED_NEW_NAME: u32 = 5;
// @end

// @implements CT-EFF-008, CT-EFF-007
/// The change records in `buf`, the bytes `ReadDirectoryChangesW` wrote: each record is
/// its action and the path relative to the watched folder. Each record holds the offset
/// of the next (0 for the last), its action, the length of its name in bytes, and the
/// name in UTF-16. A record cut short ends the list.
fn records(buf: &[u8]) -> Vec<(u32, String)> {
    let word = |at: usize| {
        let bytes = buf.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?) as usize)
    };
    let mut found = Vec::new();
    let mut at = 0;
    while let (Some(next), Some(action), Some(len)) = (word(at), word(at + 4), word(at + 8)) {
        let start = at + 12;
        let Some(name) = start.checked_add(len).and_then(|end| buf.get(start..end)) else {
            break;
        };
        let units: Vec<u16> = name
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        // A name that is not valid UTF-16 is skipped, as `relative` skips one that is not
        // valid UTF-8 on other systems.
        if let Ok(name) = String::from_utf16(&units) {
            found.push((action as u32, name));
        }
        if next == 0 {
            break;
        }
        at += next;
    }
    found
}

// @implements CT-EFF-008, CT-EFF-007, CT-USE-009
/// What the change records mean for the project under `root`. A changed folder means
/// nothing by itself (Windows reports one when a file in it changes); a folder added or
/// removed means the whole project should be read again.
fn seen(root: &Path, records: &[(u32, String)]) -> Vec<Seen> {
    let mut found = Vec::new();
    let mut files = Vec::new();
    for (action, name) in records {
        let path = root.join(name);
        let Some(rel) = relative(root, &path) else {
            continue;
        };
        match *action {
            MODIFIED if path.is_dir() => {}
            ADDED | RENAMED_NEW_NAME if path.is_dir() => found.push(Seen::Everything),
            // A folder removed or renamed away is gone and cannot be told from a file by
            // looking, the same rule as for a rename on other systems.
            REMOVED | RENAMED_OLD_NAME
                if !path.exists() && Path::new(&rel).extension().is_none() =>
            {
                found.push(Seen::Everything);
            }
            ADDED | REMOVED | MODIFIED | RENAMED_OLD_NAME | RENAMED_NEW_NAME => files.push(rel),
            _ => {}
        }
    }
    if !files.is_empty() {
        found.push(Seen::Paths(files));
    }
    found
}

// @begin CT-EFF-008, CT-EFF-007, CT-USE-009
// The calls into Windows.
#[cfg(windows)]
mod os;
#[cfg(windows)]
pub(super) use os::{Stop, start};
// @end

// @verifies CT-EFF-008, CT-EFF-007, CT-USE-009
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // @verifies CT-EFF-008
    fn record(next: u32, action: u32, name: &str) -> Vec<u8> {
        let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut bytes = Vec::new();
        bytes.extend(next.to_le_bytes());
        bytes.extend(action.to_le_bytes());
        bytes.extend((name.len() as u32).to_le_bytes());
        bytes.extend(name);
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        bytes
    }

    // @verifies CT-EFF-008
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("clew-watch-win").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::path::absolute(dir).unwrap()
    }

    // @verifies CT-EFF-008, CT-EFF-007
    #[test]
    fn change_records_are_decoded_in_order() {
        let next = record(0, ADDED, "é.sysml").len() as u32;
        let mut buf = record(next, ADDED, "é.sysml");
        buf.extend(record(0, MODIFIED, "model\\a.sysml"));
        assert_eq!(
            records(&buf),
            [
                (ADDED, "é.sysml".to_string()),
                (MODIFIED, "model\\a.sysml".to_string())
            ]
        );
    }

    // @verifies CT-EFF-008
    #[test]
    fn records_cut_short_or_empty_end_the_list() {
        assert!(records(&[]).is_empty());
        let mut buf = record(0, ADDED, "a.sysml");
        buf.truncate(buf.len() - 4);
        assert!(records(&buf).is_empty());
        // The next record is past the end, and the name ends one byte short.
        let mut buf = record(4096, ADDED, "a.sysml");
        buf.extend([1, 2, 3]);
        assert_eq!(records(&buf), [(ADDED, "a.sysml".to_string())]);
        assert!(records(&buf[..12 + 14 - 1]).is_empty());
    }

    // @verifies CT-EFF-008, CT-EFF-007, CT-USE-009
    #[test]
    fn file_changes_are_project_relative_and_hidden_ones_are_dropped() {
        let root = folder("files");
        fs::create_dir_all(root.join("model")).unwrap();
        let found = seen(
            &root,
            &[
                (MODIFIED, "model/a.sysml".into()),
                (ADDED, "model/.a.sysml.1.clewtmp".into()),
                (RENAMED_NEW_NAME, "model/b.sysml".into()),
                (REMOVED, ".git/index".into()),
                (99, "model/c.sysml".into()),
            ],
        );
        assert!(
            matches!(&found[..], [Seen::Paths(p)] if p == &["model/a.sysml", "model/b.sysml"]),
            "{found:?}"
        );
    }

    // @verifies CT-EFF-008, CT-EFF-007
    #[test]
    fn a_changed_folder_is_nothing_but_an_added_or_removed_one_is_everything() {
        let root = folder("folders");
        fs::create_dir_all(root.join("model")).unwrap();
        assert!(seen(&root, &[(MODIFIED, "model".into())]).is_empty());
        let added = seen(&root, &[(ADDED, "model".into())]);
        assert!(matches!(&added[..], [Seen::Everything]), "{added:?}");
        let gone = seen(&root, &[(RENAMED_OLD_NAME, "old".into())]);
        assert!(matches!(&gone[..], [Seen::Everything]), "{gone:?}");
        let removed = seen(&root, &[(REMOVED, "model/a.sysml".into())]);
        assert!(
            matches!(&removed[..], [Seen::Paths(p)] if p == &["model/a.sysml"]),
            "{removed:?}"
        );
    }

    // @verifies CT-EFF-008
    #[cfg(windows)]
    #[test]
    fn the_actions_are_the_ones_windows_defines() {
        use windows_sys::Win32::Storage::FileSystem as fs;
        assert_eq!(ADDED, fs::FILE_ACTION_ADDED);
        assert_eq!(REMOVED, fs::FILE_ACTION_REMOVED);
        assert_eq!(MODIFIED, fs::FILE_ACTION_MODIFIED);
        assert_eq!(RENAMED_OLD_NAME, fs::FILE_ACTION_RENAMED_OLD_NAME);
        assert_eq!(RENAMED_NEW_NAME, fs::FILE_ACTION_RENAMED_NEW_NAME);
    }
}
