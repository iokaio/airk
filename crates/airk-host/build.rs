// SPDX-License-Identifier: Apache-2.0
//! Embed only explicitly prepared, pinned assets. Never fetch from a build script.
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=AIRK_BUNDLED_MODEL_DIR");
    let workspace = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let catalog = workspace.join("crates/airk-retrieval/src/model_catalog.json");
    println!("cargo:rerun-if-changed={}", catalog.display());
    let catalog: serde_json::Value = serde_json::from_slice(&fs::read(catalog).unwrap()).unwrap();
    assert_eq!(catalog["schema"], 2);
    if env::var_os("CARGO_FEATURE_BUNDLED_MODEL").is_some() {
        let capture = workspace.join("fixtures/models/model-policy.json");
        println!("cargo:rerun-if-changed={}", capture.display());
        let capture: serde_json::Value = serde_json::from_slice(
            &fs::read(capture).expect("Capture reviewed native model policy before bundled builds"),
        )
        .unwrap();
        assert_eq!(capture["schema"], "airk.minilm.native-policy/2");
        assert_eq!(
            capture["catalog"], catalog,
            "bundled model inputs differ from native approved policy"
        );
    }
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
        assert!(["minilm-l6-v2-u8-v2", "minilm-l6-v2-s8-v2"].contains(&id));
        let precision = match id {
            "minilm-l6-v2-u8-v2" => "mixed-u8",
            "minilm-l6-v2-s8-v2" => "mixed-s8",
            _ => panic!("only the two pinned INT8 models may be embedded"),
        };
        assert_eq!(profile["precision"], precision);
        let weight = profile["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == "model.onnx")
            .unwrap();
        assert_eq!(weight["sha256"], profile["manifest"]["model_sha256"]);
        assert!(
            profile["quantization"]["fp32_embeddings_retained"]
                .as_bool()
                .unwrap()
        );
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
        let manifest = fs::read(manifest_path).expect("Prepare BOTH trained mixed-INT8 models from explicit authenticated local inputs using scripts/provision-local-model.py, then set AIRK_BUNDLED_MODEL_DIR");
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
}
