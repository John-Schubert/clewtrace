//! Build script: reads tauri.conf.json and the capability files, and generates the access
//! control list the app is compiled with.

// @implements CT-PLT-001, CT-SEC-001
fn main() {
    tauri_build::build();
}
