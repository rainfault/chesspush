fn main() {
    #[cfg(feature = "desktop")]
    {
        // The Windows resource compiler also needs to rerun when only the icon changes.
        println!("cargo:rerun-if-changed=icons/icon.ico");
        tauri_build::build();
    }
}
