#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::expect_used)]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
