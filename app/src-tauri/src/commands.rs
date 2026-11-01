//! The Tauri commands the UI calls, one per kind of page (ADR-0004). Each is a thin wrapper
//! over `data`, runs on Tauri's thread pool so the window never waits on it, and replies
//! with JSON text that `app/src/lib/api.ts` reads.

use std::path::PathBuf;
use std::sync::Arc;

use clew_core::json::quote;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};

use crate::data::{AppError, AppState, reply};
use crate::recent::{self, RECENT_FILE};

// @implements CT-SEC-003, CT-USE-007
/// The recent-projects file in the app config folder.
fn recent_file(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app.path().app_config_dir().map_err(|e| {
        AppError::new(
            format!("the app config folder is unknown: {e}"),
            "",
            "Open the project with its folder path instead: clewtrace <folder>.",
        )
    })?;
    Ok(dir.join(RECENT_FILE))
}

// @implements CT-EFF-004, CT-USE-007
/// Whether a project is open, loading, or failed to open, and its counts when open.
#[tauri::command(async)]
pub fn project_status(state: State<'_, Arc<AppState>>) -> Response {
    Response::new(state.status())
}

// @implements CT-EFF-004, CT-USE-007
/// Waits for a load in progress to end, then replies like `project_status`. The UI calls
/// it once instead of polling.
#[tauri::command(async)]
pub fn wait_for_project(state: State<'_, Arc<AppState>>) -> Response {
    Response::new(state.wait())
}

// @implements CT-EFF-005
/// One page of the tree children of an element; 0 is the root.
#[tauri::command(async)]
pub fn tree_children(
    state: State<'_, Arc<AppState>>,
    parent: usize,
    offset: usize,
    limit: usize,
) -> Response {
    let result = state
        .project()
        .and_then(|p| p.children(parent, offset, limit));
    Response::new(reply(result))
}

// @implements CT-EFF-005
/// The detail of one element.
#[tauri::command(async)]
pub fn element_detail(state: State<'_, Arc<AppState>>, id: usize) -> Response {
    Response::new(reply(state.project().and_then(|p| p.element(id))))
}

// @implements CT-EFF-005
/// One page of the links `from` or `to` an element.
#[tauri::command(async)]
pub fn element_links(
    state: State<'_, Arc<AppState>>,
    id: usize,
    direction: String,
    offset: usize,
    limit: usize,
) -> Response {
    let result = (state.project()).and_then(|p| p.links(id, &direction, offset, limit));
    Response::new(reply(result))
}

// @implements CT-TRC-005, CT-EFF-005
/// One page of the elements and code reachable `upstream` or `downstream` of an element.
#[tauri::command(async)]
pub fn element_impact(
    state: State<'_, Arc<AppState>>,
    id: usize,
    direction: String,
    offset: usize,
    limit: usize,
) -> Response {
    let result = (state.project()).and_then(|p| p.impact(id, &direction, offset, limit));
    Response::new(reply(result))
}

// @implements CT-EFF-005, CT-USE-007
/// One page of the project's errors and warnings.
#[tauri::command(async)]
pub fn problems(state: State<'_, Arc<AppState>>, offset: usize, limit: usize) -> Response {
    Response::new(reply(state.project().map(|p| p.problems(offset, limit))))
}

// @implements CT-EFF-005, CT-EFF-006
/// One page of full-text search hits.
#[tauri::command(async)]
pub fn search(
    state: State<'_, Arc<AppState>>,
    text: String,
    offset: usize,
    limit: usize,
) -> Response {
    let result = state.project().and_then(|p| p.search(&text, offset, limit));
    Response::new(reply(result))
}

// @implements CT-SEC-003
/// Shows the OS folder dialog and replies with the chosen path, or null when the user
/// cancels. It only chooses: `open_project` opens.
#[tauri::command]
pub async fn pick_folder() -> Response {
    let folder = rfd::AsyncFileDialog::new()
        .set_title("Open a Clewtrace project folder")
        .pick_folder()
        .await;
    let json = folder.map_or("null".to_string(), |f| quote(&f.path().to_string_lossy()));
    Response::new(reply(Ok(json)))
}

// @implements CT-SEC-003, CT-USE-007
/// Opens the project in `path`, waits for the load to end, and replies like
/// `project_status`. A project that opens goes first in the recent list; one that fails
/// does not.
#[tauri::command(async)]
pub fn open_project(app: AppHandle, state: State<'_, Arc<AppState>>, path: String) -> Response {
    let path = PathBuf::from(path);
    state.open(path.clone());
    let status = state.wait();
    if state.project().is_ok() {
        // The list is a convenience, so a failure to save it must not undo the open.
        if let Ok(file) = recent_file(&app) {
            let _ = recent::record(&file, &path);
        }
    }
    Response::new(status)
}

// @implements CT-SEC-003, CT-USE-007
/// The recent projects, newest first, as `{path, name}` rows.
#[tauri::command(async)]
pub fn recent_projects(app: AppHandle) -> Response {
    let result = recent_file(&app)
        .and_then(|file| recent::read(&file))
        .map(|paths| recent::json(&paths));
    Response::new(reply(result))
}

// @implements CT-SEC-003, CT-USE-007
/// Empties the recent list.
#[tauri::command(async)]
pub fn clear_recent(app: AppHandle) -> Response {
    let result = recent_file(&app)
        .and_then(|file| recent::clear(&file))
        .map(|()| "[]".to_string());
    Response::new(reply(result))
}
