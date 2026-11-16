// SPDX-License-Identifier: Apache-2.0
//! Fictional complete C7 metadata, never an executable or release approval.
use super::*;
use serde_json::{Value, json};
use std::path::PathBuf;

struct Fixture {
    root: tempfile::TempDir,
    main: PathBuf,
    cli: PathBuf,
    executable: String,
    metadata: Value,
    receipt: Value,
}
impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let mut receipt: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/split/receipts/build_receipt_2.json"
        )))?;
        receipt = receipt["base"].take();
        let mut metadata = receipt["runtime"].clone();
        metadata["version"] = env!("CARGO_PKG_VERSION").into();
        metadata["os"] = std::env::consts::OS.into();
        metadata["arch"] = std::env::consts::ARCH.into();
        let declared: airk_core::runtime_metadata::SplitRuntimeMetadata =
            serde_json::from_value(metadata.clone())?;
        let main = root.path().join(declared.service_relative_path()?);
        let cli = root.path().join(declared.cli_relative_path()?);
        fs::create_dir_all(main.parent().context("fixture main parent")?)?;
        fs::create_dir_all(cli.parent().context("fixture CLI parent")?)?;
        fs::write(&main, b"original fictional C7 service image")?;
        fs::write(&cli, b"original fictional C7 CLI image")?;
        let executable = airk_host::runtime_pair::digest_file(&main)?;
        metadata["airk_sha256"] = executable.clone().into();
        metadata["airkctl_sha256"] = airk_host::runtime_pair::digest_file(&cli)?.into();
        receipt["runtime"] = metadata.clone();
        let fixture = Self {
            root,
            main,
            cli,
            executable,
            metadata,
            receipt,
        };
        fixture.save(&fixture.receipt, &fixture.metadata, None)?;
        Ok(fixture)
    }
    fn save(&self, receipt: &Value, metadata: &Value, omit: Option<&str>) -> Result<()> {
        self.save_bytes(&serde_json::to_vec(receipt)?, metadata, omit)
    }
    fn save_bytes(&self, receipt: &[u8], metadata: &Value, omit: Option<&str>) -> Result<()> {
        let metadata_bytes = serde_json::to_vec(metadata)?;
        fs::write(self.root.path().join("airk-runtime.json"), &metadata_bytes)?;
        fs::write(self.root.path().join("airk-build.json"), receipt)?;
        let main_name = self
            .main
            .strip_prefix(self.root.path())?
            .to_string_lossy()
            .replace('\\', "/");
        let cli_name = self
            .cli
            .strip_prefix(self.root.path())?
            .to_string_lossy()
            .replace('\\', "/");
        let mut inventory = json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),"files":{
            (main_name):self.executable,(cli_name):metadata["airkctl_sha256"],
            "airk-runtime.json":format!("{:x}",Sha256::digest(&metadata_bytes)),
            "airk-build.json":format!("{:x}",Sha256::digest(receipt))}});
        if let Some(name) = omit {
            inventory["files"].as_object_mut().unwrap().remove(name);
        }
        fs::write(
            self.root.path().join("airk-install.json"),
            serde_json::to_vec(&inventory)?,
        )?;
        Ok(())
    }
}

#[test]
fn c7_build_observation_requires_original_complete_pair_and_both_physical_images() -> Result<()> {
    let fixture = Fixture::new()?;
    let (payload, observed) = build(&fixture.main, &fixture.executable)?;
    assert_eq!(observed.source_sha256.to_string(), "5".repeat(64));
    assert_eq!(observed.source_files, 1834);
    assert_eq!(observed.profile, "release");
    assert_eq!(
        payload.to_string(),
        airk_host::runtime_pair::digest_file(&fixture.root.path().join("airk-install.json"))?
    );
    // Null runtime metadata remains an unavailable source observation, never success.
    fs::remove_file(fixture.root.path().join("airk-build.json"))?;
    assert!(build(&fixture.main, &fixture.executable).is_err());
    Ok(())
}

#[test]
fn c7_build_observation_refuses_unknown_missing_tampered_or_projected_authority() -> Result<()> {
    for mutation in [
        "schema",
        "runtime_missing",
        "top_unknown",
        "runtime_unknown",
        "profile",
        "version",
        "target",
        "receipt_pair",
        "metadata_pair",
        "legacy_wrapper",
        "cli_bytes",
        "cli_claim",
        "runtime_inventory",
        "cli_inventory",
        "receipt_bytes",
        "oversize",
        "rustc_bound",
        "rustc_empty",
        "rustc_control",
        "source_zero",
        "source_over",
        "source_unknown",
        "report_missing",
        "report_scalar",
        "cli_missing",
    ] {
        let fixture = Fixture::new()?;
        let mut receipt = fixture.receipt.clone();
        let mut metadata = fixture.metadata.clone();
        match mutation {
            "schema" => receipt["schema"] = 3.into(),
            "runtime_missing" => {
                receipt.as_object_mut().unwrap().remove("runtime");
            }
            "top_unknown" => receipt["invented_build_authority"] = true.into(),
            "runtime_unknown" => receipt["runtime"]["invented_build_authority"] = true.into(),
            "profile" => receipt["profile"] = "qualified".into(),
            "version" => receipt["runtime"]["version"] = "0.0.1".into(),
            "target" => receipt["runtime"]["os"] = "unknown".into(),
            "receipt_pair" => receipt["runtime"]["airk_sha256"] = "9".repeat(64).into(),
            "metadata_pair" => metadata["i1_protocol"] = 2.into(),
            "legacy_wrapper" => {
                metadata = json!({"schema":2,"version":env!("CARGO_PKG_VERSION"),
                "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"airk_sha256":fixture.executable.clone()})
            }
            "cli_claim" => {
                receipt["runtime"]["airkctl_sha256"] = "9".repeat(64).into();
                metadata["airkctl_sha256"] = "9".repeat(64).into();
            }
            "rustc_bound" => receipt["rustc"] = "r".repeat(4097).into(),
            "rustc_empty" => receipt["rustc"] = "".into(),
            "rustc_control" => receipt["rustc"] = "compiler\u{0001}".into(),
            "source_zero" => receipt["source"]["files"] = 0.into(),
            "source_over" => receipt["source"]["files"] = 1_000_001.into(),
            "source_unknown" => receipt["source"]["invented_authority"] = true.into(),
            "report_missing" => {
                receipt.as_object_mut().unwrap().remove("admin_assets");
            }
            "report_scalar" => receipt["static_ort"] = Value::Null,
            _ => {}
        }
        let cli_name = fixture
            .cli
            .strip_prefix(fixture.root.path())?
            .to_string_lossy()
            .replace('\\', "/");
        let omit = match mutation {
            "runtime_inventory" => Some("airk-runtime.json"),
            "cli_inventory" => Some(cli_name.as_str()),
            _ => None,
        };
        fixture.save(&receipt, &metadata, omit)?;
        match mutation {
            "cli_bytes" => fs::write(&fixture.cli, b"changed fictional CLI")?,
            "cli_missing" => fs::remove_file(&fixture.cli)?,
            "receipt_bytes" => fs::write(
                fixture.root.path().join("airk-build.json"),
                b"changed receipt",
            )?,
            "oversize" => fixture.save_bytes(&vec![b' '; 16385], &metadata, None)?,
            _ => {}
        }
        assert!(
            build(&fixture.main, &fixture.executable).is_err(),
            "accepted {mutation}"
        );
    }
    Ok(())
}
#[test]
fn c7_build_observation_refuses_literal_duplicate_source_and_pair_fields() -> Result<()> {
    let fixture = Fixture::new()?;
    let raw = serde_json::to_string(&fixture.receipt)?;
    let runtime = serde_json::to_string(&fixture.receipt["runtime"])?;
    let duplicate_runtime = runtime.replacen(
        "\"airkctl_sha256\":",
        &format!(
            "\"airkctl_sha256\":\"{}\",\"airkctl_sha256\":",
            "9".repeat(64)
        ),
        1,
    );
    let source = serde_json::to_string(&fixture.receipt["source"])?;
    let duplicates = [
        (
            "top runtime",
            raw.replacen(
                "\"runtime\":",
                &format!("\"runtime\":{runtime},\"runtime\":"),
                1,
            ),
        ),
        ("runtime CLI", raw.replacen(&runtime, &duplicate_runtime, 1)),
        (
            "top source",
            raw.replacen(
                &format!("\"source\":{source}"),
                &format!("\"source\":{source},\"source\":{source}"),
                1,
            ),
        ),
        (
            "source files",
            raw.replacen("\"files\":1834", "\"files\":0,\"files\":1834", 1),
        ),
    ];
    for (ordinal, (name, duplicate)) in duplicates.into_iter().enumerate() {
        assert_ne!(
            duplicate, raw,
            "literal control not injected: {ordinal} ({name})"
        );
        fixture.save_bytes(duplicate.as_bytes(), &fixture.metadata, None)?;
        assert!(
            build(&fixture.main, &fixture.executable).is_err(),
            "accepted duplicate original bytes: {ordinal} ({name})"
        );
    }
    Ok(())
}

#[test]
fn c7_build_compiler_bound_matches_writer_without_changing_legacy_observation() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut receipt = fixture.receipt.clone();
    receipt["rustc"] = "r".repeat(2049).into();
    fixture.save(&receipt, &fixture.metadata, None)?;
    assert_eq!(
        build(&fixture.main, &fixture.executable)?.1.rustc.len(),
        2049
    );
    receipt["schema"] = 1.into();
    fixture.save(&receipt, &fixture.metadata, None)?;
    assert!(
        build(&fixture.main, &fixture.executable).is_err(),
        "legacy byte bound changed"
    );
    Ok(())
}
