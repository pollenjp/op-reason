//! Embeds the manifest into op-reason.exe: per-monitor DPI awareness (V2), run as the invoker, the version from
//! Cargo.toml, comctl32 version 6 (the settings window's standard controls in today's look), and the console allocation
//! policy `detached`: Windows 11 24H2 and later (and Windows Server 2025) make op-reason.exe no console that it does not
//! inherit, so that a scheduled task's relay and a double click show no console window (design
//! `docs/superpowers/specs/2026-11-24-relay-no-console-window-design.md`; older Windows ignore the element). No rc.exe
//! is needed, so cross builds work too.

use std::path::PathBuf;

use embed_manifest::manifest::DpiAwareness;
use embed_manifest::{embed_manifest_file, new_manifest};

/// Asks Windows for no console unless one is inherited (embed-manifest has no setter for it).
const CONSOLE_POLICY: &str = r#"<consoleAllocationPolicy xmlns="http://schemas.microsoft.com/SMI/2024/WindowsSettings">detached</consoleAllocationPolicy>"#;
/// The end of the Windows settings that the builder writes: the policy goes before it.
const SETTINGS_END: &str = "</asmv3:windowsSettings>";

fn part(name: &str) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("{name} does not fit a manifest version part (0..=65535)"))
}

/// The builder's manifest with the console allocation policy among its Windows settings. Fails the build when the end
/// of those settings is not there exactly once, so that a change of embed-manifest cannot drop the policy silently.
fn with_console_policy(xml: &str) -> String {
    assert_eq!(
        xml.matches(SETTINGS_END).count(),
        1,
        "the manifest should end its Windows settings once with {SETTINGS_END}: {xml}"
    );
    xml.replacen(SETTINGS_END, &format!("{CONSOLE_POLICY}{SETTINGS_END}"), 1)
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
            .dpi_awareness(DpiAwareness::PerMonitorV2);
        let file = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
            .join("op-reason.exe.manifest");
        std::fs::write(&file, with_console_policy(&manifest.to_string()))
            .expect("could not write the manifest");
        embed_manifest_file(&file).expect("could not embed the manifest");
    }
}
