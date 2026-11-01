//! Build script: reads tauri.conf.json and the capability files, and generates the access
//! control list the app is compiled with. The app's own commands are listed here, so each
//! one is denied unless a capability grants its `allow-` permission.

/// The commands in `src/commands.rs`. A command missing here cannot be called at all.
// @implements CT-SEC-001, CT-EFF-005, CT-TRC-004, CT-MOD-004, CT-USE-008, CT-MOD-003, CT-EFF-008
const COMMANDS: &[&str] = &[
    "project_status",
    "wait_for_project",
    "wait_for_disk_change",
    "tree_children",
    "element_detail",
    "element_links",
    "element_impact",
    "coverage_gaps",
    "source_lines",
    "source_text",
    "save_source",
    "validate_source",
    "revert_source",
    "editor_keywords",
    "problems",
    "search",
    "pick_folder",
    "open_project",
    "recent_projects",
    "clear_recent",
];

// @implements CT-PLT-001, CT-SEC-001
fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("tauri-build could not read the app config or capabilities");
}
