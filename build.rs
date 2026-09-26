//! Embed the icon, application manifest and version info into the Windows
//! app. Other targets, and the core-only build, need none of it.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/righttype.rc");
    println!("cargo:rerun-if-changed=packaging/windows/righttype.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let winos = std::env::var_os("CARGO_FEATURE_WINOS").is_some();
    if !(windows && winos) {
        return;
    }
    // An MSVC target needs rc.exe from a Windows SDK. Cross-checking from
    // another OS (clippy, `cargo check`) cannot have it; the binary it would
    // produce is never shipped, so skip the resources there instead of failing.
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let host_windows = std::env::var("HOST").is_ok_and(|h| h.contains("windows"));
    if msvc && !host_windows {
        println!("cargo:warning=cross-compiling for MSVC: Windows resources not embedded");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Resource-script string literals: forward slashes work for both rc.exe
    // and windres and need no escaping.
    let path = |p: &str| root.join(p).display().to_string().replace('\\', "/");
    let rc = std::fs::read_to_string(root.join("packaging/windows/righttype.rc"))
        .expect("reading righttype.rc")
        .replace("@ICON@", &path("assets/icon.ico"))
        .replace("@MANIFEST@", &path("packaging/windows/righttype.manifest"))
        .replace("@MAJOR@", env!("CARGO_PKG_VERSION_MAJOR"))
        .replace("@MINOR@", env!("CARGO_PKG_VERSION_MINOR"))
        .replace("@PATCH@", env!("CARGO_PKG_VERSION_PATCH"))
        .replace("@VERSION@", env!("CARGO_PKG_VERSION"));
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("righttype.rc");
    std::fs::write(&out, rc).expect("writing righttype.rc");
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .expect("embedding Windows resources");
}
