#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn halo_storage_roots(
    halo_user_root: Option<PathBuf>,
    halo_home: Option<PathBuf>,
    config_root: Option<PathBuf>,
    user_home: Option<PathBuf>,
) -> (PathBuf, PathBuf) {
    let user_root = halo_user_root
        .or_else(|| config_root.map(|root| root.join("Halo Studio")))
        .unwrap_or_else(|| std::env::temp_dir().join("Halo Studio"));
    let home_root = halo_home
        .or_else(|| user_home.map(|root| root.join(".halo-studio")))
        .unwrap_or_else(|| user_root.join("home"));
    (user_root, home_root)
}

fn configure_halo_storage_scope() {
    let (user_root, home_root) = halo_storage_roots(
        env_path("HALO_USER_ROOT"),
        env_path("HALO_HOME"),
        env_path("APPDATA").or_else(|| env_path("XDG_CONFIG_HOME")),
        env_path("USERPROFILE").or_else(|| env_path("HOME")),
    );
    // The shared BitFun infrastructure reads these names. Set them before its
    // first global initialization so Halo never imports another product profile.
    std::env::set_var("BITFUN_USER_ROOT", user_root);
    std::env::set_var("BITFUN_HOME", home_root);
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    configure_halo_storage_scope();
    std::env::set_var("RUST_MIN_STACK", "8388608");
    let logs_root = bitfun_desktop_lib::logging::product_logs_root("Halo Studio");
    bitfun_desktop_lib::run_with_context_and_options(
        tauri::generate_context!(),
        bitfun_desktop_lib::DesktopRunOptions::with_logs_root(logs_root),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halo_storage_scope_uses_its_own_explicit_roots() {
        let (user_root, home_root) = halo_storage_roots(
            Some(PathBuf::from("D:/isolated/halo-user")),
            Some(PathBuf::from("D:/isolated/halo-home")),
            Some(PathBuf::from("D:/ignored/config")),
            Some(PathBuf::from("D:/ignored/home")),
        );

        assert_eq!(user_root, PathBuf::from("D:/isolated/halo-user"));
        assert_eq!(home_root, PathBuf::from("D:/isolated/halo-home"));
    }

    #[test]
    fn halo_storage_scope_derives_names_that_cannot_overlap_bitfun_defaults() {
        let (user_root, home_root) = halo_storage_roots(
            None,
            None,
            Some(PathBuf::from("D:/profiles/config")),
            Some(PathBuf::from("D:/profiles/home")),
        );

        assert_eq!(user_root, PathBuf::from("D:/profiles/config/Halo Studio"));
        assert_eq!(home_root, PathBuf::from("D:/profiles/home/.halo-studio"));
        assert!(!user_root.to_string_lossy().contains("bitfun"));
        assert!(!home_root.to_string_lossy().contains(".bitfun"));
    }
}
