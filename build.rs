use glob::glob;
use std::{cmp::Reverse, env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NPCAP_SDK_DIR");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        configure_windows();
    }
}

fn configure_windows() {
    // Only handle Windows MSVC x86_64 for now; easy to extend later
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    match target_arch.as_str() {
        "x86_64" => {
            let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
            let sdk_root = find_npcap_sdk(&manifest_dir);
            let Some(sdk_root) = sdk_root else {
                panic!(
                    "Npcap SDK not found. Set NPCAP_SDK_DIR or place a folder like 'npcap-sdk-1.15' next to Cargo.toml"
                );
            };

            let lib_x64 = sdk_root.join("Lib").join("x64");
            if !lib_x64.exists() {
                panic!("Npcap x64 lib directory not found: {}", norm(&lib_x64));
            }
            println!("cargo:rustc-link-search=native={}", norm(&lib_x64));

            // Ensure import library exists
            let packet_lib = lib_x64.join("Packet.lib");
            if !packet_lib.exists() {
                panic!(
                    "Missing Packet.lib in {}. Ensure Npcap SDK for x64 is present.",
                    norm(&lib_x64)
                );
            }

            // Name-based linking; Packet.lib will be picked from the search path
            println!("cargo:rustc-link-lib=dylib=Packet");
        }
        other => {
            // Unsupported for now; fail fast so it doesn't silently mislink
            panic!("Unsupported Windows arch for Npcap config: {}", other);
        }
    }
}

fn find_npcap_sdk(root: &PathBuf) -> Option<PathBuf> {
    // 1) NPCAP_SDK_DIR if set
    if let Ok(dir) = env::var("NPCAP_SDK_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }
    // 2) Use glob to match any folder starting with "npcap-sdk*"
    let pattern = root.join("npcap-sdk*");
    let mut picks: Vec<PathBuf> = glob(&pattern.to_string_lossy())
        .ok()?
        .filter_map(|r| r.ok())
        .filter(|p| p.is_dir())
        .collect();
    // Prefer longer names (e.g., npcap-sdk-1.15 over npcap-sdk)
    picks.sort_by_key(|p| Reverse(p.as_os_str().len()));
    picks.into_iter().next()
}

fn norm(p: &PathBuf) -> String {
    p.to_string_lossy().replace('\\', "/")
}
