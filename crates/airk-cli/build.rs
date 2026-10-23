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
    let profile = &catalog["profiles"][0];
    assert_eq!(catalog["schema"], 1);
    assert_eq!(profile["manifest"]["schema"], 2);
    assert_eq!(catalog["profiles"].as_array().unwrap().len(), 1);
    assert_eq!(profile["id"], catalog["default_profile"]);
    assert_eq!(profile["precision"], "fp32");
    if env::var_os("CARGO_FEATURE_BUNDLED_MODEL").is_none() {
        let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        fs::write(
            output.join("model.json"),
            serde_json::to_vec(&profile["manifest"]).unwrap(),
        )
        .unwrap();
        return;
    }
    let directory = env::var_os("AIRK_BUNDLED_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target/local-vector-model"));
    let directory = directory.canonicalize().expect(
        "Prepare the bundled model first: python scripts/provision-local-model.py --destination target/local-vector-model --download (or supply verified offline assets via AIRK_BUNDLED_MODEL_DIR)",
    );
    let manifest_path = directory.join("model.json");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let manifest = fs::read(&manifest_path).expect("missing bundled model.json");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&manifest).unwrap(),
        profile["manifest"],
        "bundled manifest differs from helper catalog"
    );
    let mut generated = String::from("pub static ASSETS: &[(&str, &[u8])] = &[\n");
    assert_eq!(profile["assets"].as_array().unwrap().len(), 3);
    let mut names = std::collections::BTreeSet::new();
    for asset in profile["assets"].as_array().unwrap() {
        let name = asset["name"].as_str().unwrap();
        assert!(names.insert(name), "duplicate bundled asset");
        let field = match name {
            "model.onnx" => "model_sha256",
            "tokenizer.json" => "tokenizer_sha256",
            "LICENSE" => "license_sha256",
            _ => panic!("unexpected bundled asset"),
        };
        assert_eq!(asset["sha256"], profile["manifest"][field]);
        let file = directory.join(name);
        println!("cargo:rerun-if-changed={}", file.display());
        let metadata = fs::symlink_metadata(&file).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert_eq!(metadata.len(), asset["bytes"].as_u64().unwrap());
        let bytes = fs::read(&file).unwrap();
        assert_eq!(bytes.len() as u64, asset["bytes"].as_u64().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            asset["sha256"].as_str().unwrap(),
            "bundled asset digest differs"
        );
        // Snapshot verified bytes in OUT_DIR so a concurrent cache edit cannot
        // change the compiled payload between verification and include_bytes!.
        let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(name);
        fs::write(&output, bytes).unwrap();
        generated.push_str(&format!(
            "({name:?}, include_bytes!({:?})),\n",
            output.to_str().unwrap()
        ));
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(output.join("model.json"), manifest).unwrap();
    generated.push_str(&format!(
        "(\"model.json\", include_bytes!({:?})),\n];\n",
        output.join("model.json").to_str().unwrap()
    ));
    fs::write(output.join("bundled_model.rs"), generated).unwrap();
}
