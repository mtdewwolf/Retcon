//! Embed a Windows application manifest declaring `asInvoker` execution.
//!
//! Without a manifest, Windows' installer-detection heuristic sees "updater" in
//! the executable name (including test binaries like `retcon_updater-<hash>.exe`)
//! and demands UAC elevation, which breaks `cargo test` (os error 740).

fn main() {
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTUAC:level='asInvoker' uiAccess='false'");
    }
}
