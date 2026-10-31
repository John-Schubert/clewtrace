//! The recent-projects list: folder paths only, never project content (CT-SEC-003). It is
//! one path per line in a text file in the app config folder, newest first, and holds at
//! most `MAX_RECENT` paths. A missing file is an empty list.

use std::fs;
use std::path::{Path, PathBuf};

use crate::data::{AppError, Obj, array};

// @implements CT-SEC-003
/// The file name inside the app config folder.
pub const RECENT_FILE: &str = "recent-projects.txt";

// @implements CT-SEC-003, CT-EFF-005
/// The most paths the list keeps; older ones drop off.
pub const MAX_RECENT: usize = 10;

// @implements CT-USE-007
fn io_error(what: &str, file: &Path, e: &std::io::Error) -> AppError {
    AppError::new(
        format!("{what}: {e}"),
        file.display().to_string(),
        "Check that the app config folder exists and you can write to it, then try again.",
    )
}

// @implements CT-SEC-003, CT-USE-007
/// The paths in the list file, newest first. A missing file gives an empty list.
pub fn read(file: &Path) -> Result<Vec<PathBuf>, AppError> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(text
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .take(MAX_RECENT)
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(io_error("could not read the recent projects", file, &e)),
    }
}

// @implements CT-SEC-003, CT-USE-007
/// Puts `path` first in the list file, drops a repeat of it and anything past
/// `MAX_RECENT`, and creates the folder and file when they are missing.
pub fn record(file: &Path, path: &Path) -> Result<(), AppError> {
    if path.to_string_lossy().contains(['\n', '\r']) {
        return Err(AppError::new(
            "the folder name has a line break, so it cannot go in the recent list",
            path.display().to_string(),
            "Rename the folder without a line break to keep it in the recent list.",
        ));
    }
    let mut paths = read(file)?;
    paths.retain(|p| p != path);
    paths.insert(0, path.to_path_buf());
    paths.truncate(MAX_RECENT);
    let text: String = paths
        .iter()
        .map(|p| format!("{}\n", p.to_string_lossy()))
        .collect();
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)
            .map_err(|e| io_error("could not create the config folder", dir, &e))?;
    }
    fs::write(file, text).map_err(|e| io_error("could not save the recent projects", file, &e))
}

// @implements CT-SEC-003, CT-USE-007
/// Deletes the list file. Clearing an empty list is not an error.
pub fn clear(file: &Path) -> Result<(), AppError> {
    match fs::remove_file(file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error("could not clear the recent projects", file, &e)),
    }
}

// @implements CT-SEC-003
/// The list as JSON for the UI: each row has the full `path` and the folder `name`.
pub fn json(paths: &[PathBuf]) -> String {
    array(
        paths
            .iter()
            .map(|p| {
                let name = p.file_name().unwrap_or(p.as_os_str());
                Obj::new()
                    .str("path", &p.to_string_lossy())
                    .str("name", &name.to_string_lossy())
                    .end()
            })
            .collect(),
    )
}

// @verifies CT-SEC-003, CT-USE-007, CT-EFF-005
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-SEC-003
    /// A fresh config folder that does not exist yet, so `record` must create it.
    fn config(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("clewtrace-recent-tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        dir.join("config").join(RECENT_FILE)
    }

    // @verifies CT-SEC-003
    #[test]
    fn a_missing_file_is_an_empty_list() {
        assert_eq!(read(&config("missing")).unwrap(), Vec::<PathBuf>::new());
    }

    // @verifies CT-SEC-003
    #[test]
    fn the_newest_project_comes_first_and_a_repeat_moves_up() {
        let file = config("order");
        for p in ["/a", "/b", "/c", "/a"] {
            record(&file, Path::new(p)).unwrap();
        }
        let want: Vec<PathBuf> = ["/a", "/c", "/b"].iter().map(PathBuf::from).collect();
        assert_eq!(read(&file).unwrap(), want);
    }

    // @verifies CT-SEC-003, CT-EFF-005
    #[test]
    fn the_list_keeps_at_most_max_recent_paths() {
        let file = config("cap");
        for n in 0..MAX_RECENT + 5 {
            record(&file, Path::new(&format!("/p{n}"))).unwrap();
        }
        let paths = read(&file).unwrap();
        assert_eq!(paths.len(), MAX_RECENT);
        assert_eq!(paths[0], PathBuf::from(format!("/p{}", MAX_RECENT + 4)));
    }

    // @verifies CT-SEC-003
    #[test]
    fn the_file_holds_folder_paths_and_nothing_else() {
        let file = config("content");
        record(&file, Path::new("/work/one")).unwrap();
        record(&file, Path::new("/work/two")).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "/work/two\n/work/one\n");
    }

    // @verifies CT-SEC-003
    #[test]
    fn clearing_empties_the_list_and_may_repeat() {
        let file = config("clear");
        record(&file, Path::new("/a")).unwrap();
        clear(&file).unwrap();
        assert!(read(&file).unwrap().is_empty());
        clear(&file).unwrap();
    }

    // @verifies CT-USE-007
    #[test]
    fn errors_say_what_where_and_how_to_fix() {
        let file = config("error");
        let e = record(&file, Path::new("/bad\nname")).unwrap_err();
        assert!(e.what.contains("line break"), "{e:?}");
        assert_eq!(e.place, "/bad\nname");
        assert!(e.help.starts_with("Rename"), "{e:?}");
        // A directory where the file should be cannot be read as text.
        fs::create_dir_all(&file).unwrap();
        let e = read(&file).unwrap_err();
        assert!(e.what.starts_with("could not read"), "{e:?}");
        assert_eq!(e.place, file.display().to_string());
        assert!(!e.help.is_empty());
    }

    // @verifies CT-SEC-003
    #[test]
    fn json_gives_the_path_and_folder_name() {
        let paths = [PathBuf::from("/work/My \"Project\"")];
        assert_eq!(
            json(&paths),
            r#"[{"path":"/work/My \"Project\"","name":"My \"Project\""}]"#
        );
    }
}
