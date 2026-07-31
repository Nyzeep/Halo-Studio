fn main() {
    #[cfg(feature = "tauri-build-script")]
    tauri_build::build();

    #[cfg(not(feature = "tauri-build-script"))]
    println!("cargo:rerun-if-changed=build.rs");
}
