//! The Tauri commands the UI calls, one per kind of page (ADR-0004). Each is a thin wrapper
//! over `data`, runs on Tauri's thread pool so the window never waits on it, and replies
//! with JSON text that `app/src/lib/api.ts` reads.

use std::sync::Arc;

use tauri::State;
use tauri::ipc::Response;

use crate::data::{AppState, reply};

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
