//! Embeds the manifest into op-reason.exe: per-monitor DPI awareness (V2), run as the invoker, and the version
//! from Cargo.toml. No rc.exe is needed, so cross builds work too.

use embed_manifest::manifest::DpiAwareness;
use embed_manifest::{embed_manifest, new_manifest};

fn part(name: &str) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("{name} does not fit a manifest version part (0..=65535)"))
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let manifest = new_manifest("op-reason")
            .version(
                part("CARGO_PKG_VERSION_MAJOR"),
                part("CARGO_PKG_VERSION_MINOR"),
                part("CARGO_PKG_VERSION_PATCH"),
                0,
            )
            .remove_dependency("Microsoft.Windows.Common-Controls")
            .dpi_awareness(DpiAwareness::PerMonitorV2);
        embed_manifest(manifest).expect("could not embed the manifest");
    }
}
