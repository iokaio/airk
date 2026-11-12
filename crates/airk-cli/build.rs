// SPDX-License-Identifier: Apache-2.0
//! Embed only explicitly prepared, pinned assets. Never fetch from a build script.
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/service_manager_macos.m");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/service_manager_macos.m")
            .flag("-fobjc-arc")
            .flag("-Werror")
            .compile("airk_macos_login");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
        println!("cargo:rustc-link-lib=objc");
    }
    println!("cargo:rerun-if-env-changed=AIRK_BUNDLED_MODEL_DIR");
    let workspace = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let catalog = workspace.join("crates/airk-retrieval/src/model_catalog.json");
    println!("cargo:rerun-if-changed={}", catalog.display());
    let catalog: serde_json::Value = serde_json::from_slice(&fs::read(catalog).unwrap()).unwrap();
    assert_eq!(catalog["schema"], 1);
    let profiles = catalog["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let directory = env::var_os("AIRK_BUNDLED_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target/local-vector-model"));
    let mut generated = String::from(
        "pub type EmbeddedProfileFiles = &'static [(&'static str, &'static [u8])];\npub static PROFILES: &[(&str, EmbeddedProfileFiles)] = &[\n",
    );
    for profile in profiles {
        let id = profile["id"].as_str().unwrap();
        assert!(["minilm-l6-v2-u8-v1", "minilm-l6-v2-s8-v1"].contains(&id));
        let (precision, digest, size) = match id {
            "minilm-l6-v2-u8-v1" => (
                "uint8",
                "b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21",
                23_046_789,
            ),
            "minilm-l6-v2-s8-v1" => (
                "int8",
                "4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474",
                23_026_053,
            ),
            _ => panic!("only the two pinned INT8 models may be embedded"),
        };
        assert_eq!(profile["precision"], precision);
        assert_eq!(
            profile["manifest"]["model_sha256"], digest,
            "FP32 or unapproved model forbidden"
        );
        let weight = profile["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == "model.onnx")
            .unwrap();
        assert_eq!(weight["bytes"], size);
        assert_eq!(weight["sha256"], digest);
        let destination = output.join(id);
        fs::create_dir_all(&destination).unwrap();
        fs::write(
            destination.join("model.json"),
            serde_json::to_vec(&profile["manifest"]).unwrap(),
        )
        .unwrap();
        if env::var_os("CARGO_FEATURE_BUNDLED_MODEL").is_none() {
            continue;
        }
        let prepared = directory.join(id);
        let manifest_path = prepared.join("model.json");
        println!("cargo:rerun-if-changed={}", manifest_path.display());
        let manifest = fs::read(manifest_path).expect("Prepare BOTH INT8 models: python scripts/provision-local-model.py --destination target/local-vector-model --download (or verified offline AIRK_BUNDLED_MODEL_DIR)");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&manifest).unwrap(),
            profile["manifest"]
        );
        generated.push_str(&format!("({id:?}, &[\n"));
        let mut names = std::collections::BTreeSet::new();
        assert_eq!(profile["assets"].as_array().unwrap().len(), 3);
        for asset in profile["assets"].as_array().unwrap() {
            let name = asset["name"].as_str().unwrap();
            assert!(names.insert(name));
            let field = match name {
                "model.onnx" => "model_sha256",
                "tokenizer.json" => "tokenizer_sha256",
                "LICENSE" => "license_sha256",
                _ => panic!("unexpected bundled asset"),
            };
            assert_eq!(asset["sha256"], profile["manifest"][field]);
            let file = prepared.join(name);
            println!("cargo:rerun-if-changed={}", file.display());
            let metadata = fs::symlink_metadata(&file).unwrap();
            assert!(metadata.is_file() && !metadata.file_type().is_symlink());
            assert_eq!(metadata.len(), asset["bytes"].as_u64().unwrap());
            let bytes = fs::read(file).unwrap();
            assert_eq!(bytes.len() as u64, asset["bytes"].as_u64().unwrap());
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                asset["sha256"].as_str().unwrap()
            );
            // Snapshot verified bytes: later cache edits cannot change the payload.
            fs::write(destination.join(name), bytes).unwrap();
            generated.push_str(&format!(
                "({name:?}, include_bytes!({:?})),\n",
                destination.join(name).to_str().unwrap()
            ));
        }
        generated.push_str(&format!(
            "(\"model.json\", include_bytes!({:?})),\n]),\n",
            destination.join("model.json").to_str().unwrap()
        ));
    }
    generated.push_str("];\n");
    fs::write(output.join("bundled_model.rs"), generated).unwrap();
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
    let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("admin-assets");
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
