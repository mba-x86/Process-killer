// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
fn main() {
    // Release builds always ask for Administrator (same as 1.x). Debug builds keep the default manifest so
    // `cargo tauri dev` works from a normal terminal. For automated measurements/tests of a release build without a UAC
    // prompt, build with PK_NO_ADMIN_MANIFEST=1 (never ship such a build).
    println!("cargo:rerun-if-env-changed=PK_NO_ADMIN_MANIFEST");
    let release = std::env::var("PROFILE").map(|p| p == "release").unwrap_or(false);
    let want_admin = release && std::env::var("PK_NO_ADMIN_MANIFEST").is_err();
    let mut attrs = tauri_build::Attributes::new();
    if want_admin {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        );
    }
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
