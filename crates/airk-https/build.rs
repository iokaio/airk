// SPDX-License-Identifier: Apache-2.0
//! Embed only explicitly prepared, pinned assets. Never fetch from a build script.
//! The sealed bundle stays at `crates/airk-cli/admin-assets/` (docs/simplify/airk-cli-split.md, step 3h).
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    admin_assets(&output);
}

/// An embedded admin asset name (`admin_contract::is_asset_name`): dot-separated
/// non-empty `[a-z0-9_-]` segments, the first starting with `[a-z0-9]`.
fn admin_asset_name(name: &str) -> bool {
    name.len() <= 128
        && name
            .as_bytes()
            .first()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && name.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'-'
                        || byte == b'_'
                })
        })
}

/// Seal the admin bundle (split plan §7, Phase 5): every file the manifest
/// lists must exist with its recorded SHA-256, or the build fails. Verified
/// bytes are snapshotted into `OUT_DIR` and embedded from there.
fn admin_assets(output: &std::path::Path) {
    let source =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../airk-cli/admin-assets");
    let manifest_path = source.join("manifest.json");
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path).expect("crates/airk-cli/admin-assets/manifest.json is required"),
    )
    .expect("admin asset manifest is JSON");
    let fields = manifest.as_object().expect("admin asset manifest object");
    assert!(
        fields.keys().all(|key| key == "schema" || key == "files"),
        "unknown admin asset manifest field"
    );
    assert_eq!(manifest["schema"], "airk-admin-assets/1");
    let files = manifest["files"]
        .as_object()
        .expect("admin asset manifest files");
    let destination = output.join("admin-assets");
    let _ = fs::remove_dir_all(&destination);
    fs::create_dir_all(&destination).unwrap();
    let mut generated = String::from(
        "/// (name, sha256, content type, bytes) of each sealed admin asset.\npub static ADMIN_ASSETS: &[(&str, &str, &str, &[u8])] = &[\n",
    );
    for (name, entry) in files {
        assert!(admin_asset_name(name), "invalid admin asset name {name:?}");
        let entry = entry.as_object().expect("admin asset entry");
        assert!(
            entry
                .keys()
                .all(|key| key == "sha256" || key == "content_type"),
            "unknown admin asset field for {name}"
        );
        let digest = entry["sha256"].as_str().expect("admin asset sha256");
        let content_type = entry["content_type"]
            .as_str()
            .expect("admin asset content_type");
        assert!(
            !content_type.is_empty()
                && content_type
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() || byte == b' '),
            "invalid content type for admin asset {name}"
        );
        let file = source.join(name);
        println!("cargo:rerun-if-changed={}", file.display());
        let metadata = fs::symlink_metadata(&file)
            .unwrap_or_else(|_| panic!("missing admin asset {name}: rebuild the admin bundle"));
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        let bytes = fs::read(&file).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            digest,
            "stale admin asset {name}: rebuild the admin bundle"
        );
        let snapshot = destination.join(name);
        fs::write(&snapshot, &bytes).unwrap();
        generated.push_str(&format!(
            "({name:?}, {digest:?}, {content_type:?}, include_bytes!({:?})),\n",
            snapshot.to_str().unwrap()
        ));
    }
    generated.push_str("];\n");
    fs::write(output.join("admin_assets.rs"), generated).unwrap();
}
