//! Publishes the release version of the whole app to the crates that show or
//! compare it. Only the root `Cargo.toml` is bumped by a release, so reading
//! `CARGO_PKG_VERSION` inside a member crate would report a stale version.

use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let workspace_manifest = Path::new(&manifest_dir).join("../../Cargo.toml");
    println!("cargo:rerun-if-changed={}", workspace_manifest.display());

    let manifest = std::fs::read_to_string(&workspace_manifest)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", workspace_manifest.display()));

    let version = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("no package version in {}", workspace_manifest.display()));

    println!("cargo:rustc-env=CORVO_VERSION={version}");
}
