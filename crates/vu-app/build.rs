fn main() {
    // `app_display_version()` captures `VU_RELEASE_VERSION` via
    // `option_env!` on non-macOS builds. Cargo does not track env reads
    // inside macros, so a cached build would pin a stale version string
    // across a tag change — declare the dep to force invalidation.
    println!("cargo:rerun-if-env-changed=VU_RELEASE_VERSION");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        cc::Build::new()
            .file("src/objc/sparkle_trampoline.m")
            .file("src/objc/global_hotkey_trampoline.m")
            .file("src/objc/quick_terminal_trampoline.m")
            .file("src/objc/ghostty_surface_trampoline.m")
            .flag("-fobjc-arc")
            .flag("-fmodules")
            .compile("vu_objc_trampolines");

        println!("cargo:rerun-if-changed=src/objc/sparkle_trampoline.m");
        println!("cargo:rerun-if-changed=src/objc/global_hotkey_trampoline.m");
        println!("cargo:rerun-if-changed=src/objc/quick_terminal_trampoline.m");
        println!("cargo:rerun-if-changed=src/objc/ghostty_surface_trampoline.m");
        println!("cargo:rustc-link-lib=framework=Carbon");
    }

    #[cfg(target_os = "windows")]
    {
        // Keep the icon filename aligned with the retained Windows
        // `vu-app.exe` release alias.
        let icon = "../../assets/windows/vu-app.ico";
        println!("cargo:rerun-if-changed={}", icon);

        let mut res = winresource::WindowsResource::new();
        // Some locked-down hosts (and the GitHub Actions windows-2022
        // image on occasion) can't auto-discover rc.exe; let CI point
        // us at the toolkit explicitly.
        if let Ok(toolkit) = std::env::var("VU_RC_TOOLKIT_PATH") {
            res.set_toolkit_path(&toolkit);
        }
        res.set_icon(icon);
        res.set("FileDescription", "vu");
        res.set("ProductName", "vu");
        if let Err(e) = res.compile() {
            eprintln!("winresource failed to embed icon: {e}");
            std::process::exit(1);
        }
    }
}
