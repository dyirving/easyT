fn main() {
    // tauri-build does not currently emit a Cargo dependency for the Windows
    // icon, so replacing icon.ico alone can leave a stale resource.lib cached.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build()
}
