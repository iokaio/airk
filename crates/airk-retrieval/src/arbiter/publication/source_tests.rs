// SPDX-License-Identifier: Apache-2.0
//! Synthetic source metadata only; no URL availability or model qualification.
use super::*;
const HF: &str = "https://huggingface.co/Qwen/Qwen3-1.7B/resolve/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/tokenizer.json";
#[test]
fn publication_hf_primary_has_no_fictitious_github_asset_but_real_backup_is_projected() -> Result<()>
{
    let mut profiles = super::tests::selected_profiles()?;
    for profile in &mut profiles {
        for download in &mut profile.downloads {
            for (index, part) in download.parts.iter_mut().enumerate() {
                part.url = format!(
                    "https://huggingface.co/Qwen/Qwen3-1.7B/resolve/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/{}-{}.bin",
                    download.asset_name, index
                );
                part.backup_urls.clear();
            }
        }
    }
    let catalog = serde_json::json!({"schema":CATALOG_SCHEMA,"profiles":profiles});
    let bytes = serde_json::to_vec(&catalog)?;
    let primary = project(&bytes, "v1.9.0")?;
    assert_eq!(primary.profiles.len(), 2);
    assert!(primary.assets.is_empty());
    assert_eq!(
        primary.compiled_catalog_sha256,
        Sha256Digest::from_bytes(Sha256::digest(&bytes).into())
    );
    let mut backed = catalog.clone();
    let part = &mut backed["profiles"][0]["downloads"][0]["parts"][0];
    part["backup_urls"] = serde_json::json!([
        "https://github.com/iokaio/airk/releases/download/v1.9.0/real-metadata-fixture.bin"
    ]);
    // All parts of this asset require a matching backup roster.
    let count = backed["profiles"][0]["downloads"][0]["parts"]
        .as_array()
        .unwrap()
        .len();
    for index in 1..count {
        backed["profiles"][0]["downloads"][0]["parts"][index]["backup_urls"] = serde_json::json!([
            format!(
                "https://github.com/iokaio/airk/releases/download/v1.9.0/real-metadata-fixture-{index}.bin"
            )
        ]);
    }
    let with_backup = project(&serde_json::to_vec(&backed)?, "v1.9.0")?;
    assert_eq!(with_backup.assets.len(), count);
    assert!(with_backup.assets.contains_key("real-metadata-fixture.bin"));
    assert_ne!(
        primary.compiled_catalog_sha256,
        with_backup.compiled_catalog_sha256
    );
    backed["profiles"][0]["downloads"][0]["parts"][0]["backup_urls"] =
        serde_json::json!(["https://github.com/iokaio/airk/releases/download/v0.0.0/foreign.bin"]);
    assert!(project(&serde_json::to_vec(&backed)?, "v1.9.0").is_err());
    let mut missing = catalog;
    missing["profiles"].as_array_mut().unwrap().pop();
    assert!(project(&serde_json::to_vec(&missing)?, "v1.9.0").is_err());
    if let Some(path) = std::env::var_os("AIRK_ARBITER_PUBLICATION_SOURCE_CASES_OUTPUT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema":"airk.arbiter.publication-source-cases/1",
                "hypothetical":true,"actual_source_availability_claimed":false,
                "cases":[
                    {"name":"hf_primary_without_github_file","accepted":true,"projection":primary},
                    {"name":"hf_primary_with_declared_github_backup","accepted":true,"projection":with_backup},
                    {"name":"foreign_github_backup_tag","accepted":false},
                    {"name":"missing_selected_profile","accepted":false}
                ]
            }))?,
        )?;
    }
    Ok(())
}
#[test]
fn github_projection_classifies_every_location_and_refuses_unapproved_backup() -> Result<()> {
    let mut part = AssetPart {
        url: HF.into(),
        backup_urls: vec![],
        bytes: 1,
        sha256: Sha256Digest::from_bytes([1; 32]),
    };
    assert!(github_part_filenames(&part, "v1.9.0")?.is_empty());
    part.backup_urls =
        vec!["https://github.com/iokaio/airk/releases/download/v1.9.0/backup.bin".into()];
    assert_eq!(
        github_part_filenames(&part, "v1.9.0")?,
        BTreeSet::from(["backup.bin".into()])
    );
    part.backup_urls = vec!["https://example.invalid/backup.bin".into()];
    assert!(github_part_filenames(&part, "v1.9.0").is_err());
    Ok(())
}
