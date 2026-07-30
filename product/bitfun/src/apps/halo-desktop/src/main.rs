#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Halo Studio");
}
