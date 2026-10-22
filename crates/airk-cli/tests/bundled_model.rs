// SPDX-License-Identifier: Apache-2.0
//! The shipped executable provisions its own pinned model without external assets.
use anyhow::{Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

fn run(binary: &Path, root: &Path, data: &Path, collection: &str, args: &[&str]) -> Result<Value> {
    let output = Command::new(binary)
        .current_dir(root)
        .args([
            "--root",
            root.to_str().unwrap(),
            "--data-dir",
            data.to_str().unwrap(),
            "--collection",
            collection,
        ])
        .args(args)
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("AIRK_BUNDLED_MODEL_DIR", root.join("missing-build-inputs"))
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn copied_executable_installs_offline_and_preserves_custom_inputs() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let binary = root.join(if cfg!(windows) { "airk.exe" } else { "airk" });
    fs::copy(env!("CARGO_BIN_EXE_airk"), &binary)?;
    let data = root.join("state");
    let report = run(&binary, &root, &data, "one", &["model-catalog"])?;
    let profile = &report["catalog"]["profiles"][0];
    assert_eq!(report["bundled_model"]["manifest"], profile["manifest"]);
    assert!(!data.exists(), "catalog reads must not install the model");
    // Independent projects can perform first-use setup together. Only the
    // atomic input-model installation is shared; the second preserves it.
    std::thread::scope(|scope| -> Result<()> {
        let first = scope.spawn(|| {
            run(
                &binary,
                &root,
                &data,
                "one",
                &["collections", "vectors", "install-bundled"],
            )
        });
        let second = scope.spawn(|| {
            run(
                &binary,
                &root,
                &data,
                "two",
                &["collections", "vectors", "install-bundled"],
            )
        });
        first.join().unwrap()?;
        second.join().unwrap()?;
        Ok(())
    })?;
    let local = data.join("models/local");
    for asset in profile["assets"].as_array().unwrap() {
        let bytes = fs::read(local.join(asset["name"].as_str().unwrap()))?;
        assert_eq!(bytes.len() as u64, asset["bytes"].as_u64().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            asset["sha256"].as_str().unwrap()
        );
    }
    let mut custom = profile["manifest"].clone();
    custom["max_tokens"] = 128.into();
    let custom = serde_json::to_vec(&custom)?;
    fs::write(local.join("model.json"), &custom)?;
    run(
        &binary,
        &root,
        &data,
        "two",
        &["collections", "vectors", "install-bundled"],
    )?;
    assert_eq!(fs::read(local.join("model.json"))?, custom);
    // Incomplete existing directories are errors, never silently repaired.
    fs::write(local.join("tokenizer.json"), b"broken fixture")?;
    assert!(
        run(
            &binary,
            &root,
            &data,
            "two",
            &["collections", "vectors", "install-bundled"]
        )
        .is_err()
    );
    assert_eq!(fs::read(local.join("tokenizer.json"))?, b"broken fixture");
    Ok(())
}

#[test]
fn disabled_vectors_do_not_install_a_model() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let data = root.join("state");
    let binary = Path::new(env!("CARGO_BIN_EXE_airk"));
    run(
        binary,
        &root,
        &data,
        "disabled",
        &["collections", "vectors", "set", "--enabled", "false"],
    )?;
    assert!(
        run(
            binary,
            &root,
            &data,
            "disabled",
            &["collections", "vectors", "install-bundled"]
        )
        .is_err()
    );
    assert!(!data.join("models/local").exists());
    run(
        binary,
        &root,
        &data,
        "disabled",
        &["collections", "vectors", "set", "--enabled", "true"],
    )?;
    let policy = root.join("quota.json");
    fs::write(
        &policy,
        br#"{"root_bytes":1048576,"default_collection_bytes":1048576,"collections":{}}"#,
    )?;
    run(
        binary,
        &root,
        &data,
        "disabled",
        &["collections", "quota", "set", policy.to_str().unwrap()],
    )?;
    assert!(
        run(
            binary,
            &root,
            &data,
            "disabled",
            &["collections", "vectors", "install-bundled"]
        )
        .is_err()
    );
    assert!(
        !data.join("models/local").exists(),
        "quota failure must not publish partial inputs"
    );
    Ok(())
}
