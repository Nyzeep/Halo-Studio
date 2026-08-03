#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    std::env::set_var("RUST_MIN_STACK", "8388608");
    let logs_root = bitfun_desktop_lib::logging::product_logs_root("Halo Studio");
    bitfun_desktop_lib::run_with_context_and_options(
        tauri::generate_context!(),
        bitfun_desktop_lib::DesktopRunOptions::with_logs_root(logs_root),
    )
    .await
}
