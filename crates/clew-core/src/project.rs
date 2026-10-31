//! Finding and opening a project. The CLI and the app share this one entry point, so both
//! agree on which folder is the project root.

use std::path::{Path, PathBuf};

use crate::check::{SetupError, TRACE_CONFIG};
use crate::index::INDEX_DIR;
use crate::report::Project;

// @implements CT-TRC-008, CT-USE-007
/// The project root for `start`: the nearest folder, `start` itself or one of its parents,
/// that holds a `trace.toml` file or a `.clewtrace` folder. When there is none, `start`.
/// When `start` is the root, it is returned as given, so relative paths stay relative.
pub fn find_root(start: &Path) -> PathBuf {
    let Ok(absolute) = std::path::absolute(start) else {
        return start.to_path_buf();
    };
    for (depth, dir) in absolute.ancestors().enumerate() {
        if dir.join(TRACE_CONFIG).is_file() || dir.join(INDEX_DIR).is_dir() {
            return if depth == 0 {
                start.to_path_buf()
            } else {
                dir.to_path_buf()
            };
        }
    }
    start.to_path_buf()
}

// @implements CT-TRC-008, CT-USE-007, CT-DEV-003
impl Project {
    /// Finds the project root for `path` with `find_root` and reads the project there.
    pub fn open(path: &Path) -> Result<Project, SetupError> {
        crate::report::load_project(&find_root(path))
    }
}

// @verifies CT-TRC-008, CT-USE-007, CT-DEV-003
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // @verifies CT-TRC-008
    /// A fresh folder under the system temp folder, which has no project above it.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("clew-project-tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // @verifies CT-TRC-008, CT-USE-007
    #[test]
    fn finds_trace_toml_in_a_parent_folder() {
        let root = folder("parent-config");
        fs::write(root.join("trace.toml"), "").unwrap();
        let deep = root.join("model/deep");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(find_root(&deep), root);
    }

    // @verifies CT-TRC-008, CT-USE-007
    #[test]
    fn finds_an_index_folder_in_a_parent_folder() {
        let root = folder("parent-index");
        fs::create_dir_all(root.join(".clewtrace")).unwrap();
        let deep = root.join("a");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(find_root(&deep), root);
    }

    // @verifies CT-TRC-008, CT-USE-007
    #[test]
    fn the_nearest_marker_wins() {
        let outer = folder("nested");
        fs::write(outer.join("trace.toml"), "").unwrap();
        let inner = outer.join("sub");
        fs::create_dir_all(inner.join(".clewtrace")).unwrap();
        assert_eq!(find_root(&inner), inner);
        assert_eq!(find_root(inner.parent().unwrap()), outer);
    }

    // @verifies CT-TRC-008, CT-USE-007
    #[test]
    fn falls_back_to_the_given_folder() {
        let dir = folder("no-marker");
        assert_eq!(find_root(&dir), dir);
    }

    // @verifies CT-TRC-008
    #[test]
    fn a_marker_must_be_a_file_or_folder_of_the_right_kind() {
        let dir = folder("wrong-kind");
        fs::create_dir_all(dir.join("trace.toml")).unwrap();
        fs::write(dir.join(".clewtrace"), "").unwrap();
        assert_eq!(find_root(&dir), dir);
    }

    // @verifies CT-TRC-008
    #[test]
    fn a_start_that_is_the_root_is_returned_as_given() {
        let root = folder("is-root");
        fs::write(root.join("trace.toml"), "").unwrap();
        let with_dots = root.join(".");
        assert_eq!(find_root(&with_dots), with_dots);
    }

    // @verifies CT-TRC-008, CT-DEV-003
    #[test]
    fn open_reads_the_project_from_a_subfolder() {
        let root = folder("open");
        let config = "[project]\nrequirements = [\"reqs.sysml\"]\n\
                      id_pattern = \"CT-[A-Z]{3}-[0-9]{3}\"\ncurrent_phase = 1\n\
                      [coverage]\nsource_roots = []\nlanguages = []\n\
                      file_level_allowed = []\nmax_scope_lines = 200\n";
        fs::write(root.join("trace.toml"), config).unwrap();
        fs::write(root.join("reqs.sysml"), "part def Engine;\n").unwrap();
        let deep = root.join("x/y");
        fs::create_dir_all(&deep).unwrap();
        let project = Project::open(&deep).unwrap();
        assert_eq!(project.root, root);
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        assert_eq!(project.inputs.len(), 2);
    }

    // @verifies CT-USE-007
    #[test]
    fn open_without_a_config_names_trace_toml() {
        let dir = folder("open-none");
        let err = Project::open(&dir).err().unwrap();
        assert!(err.to_string().contains("trace.toml"), "{err}");
    }
}
