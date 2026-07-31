#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    std::env::set_var("RUST_MIN_STACK", "8388608");
    bitfun_desktop_lib::run_with_context(tauri::generate_context!()).await
}
