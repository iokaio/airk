// SPDX-License-Identifier: Apache-2.0
//! Projection of the immutable compiled catalog, never evaluation metadata.
use airk_core::{arbiter::*, identity::Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedAsset {
    pub bytes: u64,
    pub sha256: Sha256Digest,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub schema: String,
    pub tag: String,
    pub compiled_catalog_sha256: Sha256Digest,
    pub profiles: Vec<Sha256Digest>,
    pub assets: BTreeMap<String, PublishedAsset>,
}
/// Only this entrypoint projects production authority from embedded bytes.
pub fn compiled_publication(tag: &str) -> Result<Publication> {
    ensure!(
        tag == concat!("v", env!("CARGO_PKG_VERSION")),
        "publication tag differs from native version"
    );
    project(include_bytes!("release-catalog.json"), tag)
}
fn project(bytes: &[u8], tag: &str) -> Result<Publication> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Catalog {
        schema: String,
        profiles: Vec<Profile>,
    }
    ensure!(
        tag.starts_with('v')
            && tag.len() <= 192
            && tag
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "invalid final release tag"
    );
    let catalog: Catalog = serde_json::from_slice(bytes)?;
    ensure!(
        catalog.schema == CATALOG_SCHEMA && catalog.profiles.len() == 4,
        "publication requires four qualified model slots"
    );
    let mut models = BTreeSet::new();
    let mut manifests = BTreeSet::new();
    let mut assets = BTreeMap::new();
    let prefix = format!("https://github.com/iokaio/airk/releases/download/{tag}/");
    for profile in catalog.profiles {
        profile.validate_release_downloads()?;
        ensure!(
            profile.qualification.is_some()
                && profile.cpu_fit.is_some()
                && !profile.qualified_targets.is_empty(),
            "publication requires final qualification and explicit execution fit"
        );
        if profile.tasks.contains(&Task::ContextNextRead) {
            ensure!(
                profile.task_policies.get(&Task::ContextNextRead)
                    == Some(&super::tasks::policy_digest()?),
                "publication next-read policy is not native compiled policy"
            );
        }
        ensure!(
            models.insert(profile.model) && manifests.insert(profile.manifest),
            "duplicate publication slot or manifest"
        );
        for download in profile.downloads {
            for part in download.parts {
                let filename = part
                    .url
                    .strip_prefix(&prefix)
                    .ok_or_else(|| anyhow::anyhow!("asset tag differs from final release"))?;
                ensure!(
                    !filename.is_empty() && !filename.contains('/'),
                    "invalid release filename"
                );
                let pin = PublishedAsset {
                    bytes: part.bytes,
                    sha256: part.sha256,
                };
                if let Some(previous) = assets.insert(filename.to_owned(), pin.clone()) {
                    ensure!(previous == pin, "conflicting release asset pins");
                }
            }
        }
    }
    ensure!(
        models
            == BTreeSet::from([
                ModelChoice::SmallIntern,
                ModelChoice::SmallStrands,
                ModelChoice::LargeIntern,
                ModelChoice::LargeKev
            ]),
        "incomplete model slot coverage"
    );
    Ok(Publication {
        schema: "airk.arbiter.publication/1".into(),
        tag: tag.into(),
        compiled_catalog_sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        profiles: manifests.into_iter().collect(),
        assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires four actually qualified compiled release profiles"]
    fn compiled_publication_matches_retained_production_projection() -> Result<()> {
        let projection = compiled_publication(concat!("v", env!("CARGO_PKG_VERSION")))?;
        let value = serde_json::to_value(&projection)?;
        if let Some(path) = std::env::var_os("AIRK_ARBITER_PUBLICATION_OUTPUT") {
            std::fs::write(path, serde_json::to_vec_pretty(&projection)?)?;
        }
        let retained = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/arbiter/publication_catalog.json");
        let golden: serde_json::Value = serde_json::from_slice(&std::fs::read(retained)?)?;
        assert_eq!(
            value, golden,
            "compiled Arbiter publication projection drift"
        );
        Ok(())
    }
    #[test]
    fn publication_requires_all_final_slots_and_consistent_final_asset_pins() -> Result<()> {
        let base: Profile = serde_json::from_slice(include_bytes!(
            "../../../../fixtures/arbiter/profile-downloads.json"
        ))?;
        let mut profiles = Vec::new();
        for (i, model) in [
            ModelChoice::SmallIntern,
            ModelChoice::SmallStrands,
            ModelChoice::LargeIntern,
            ModelChoice::LargeKev,
        ]
        .into_iter()
        .enumerate()
        {
            let mut profile = base.clone();
            profile.model = model;
            profile.manifest = Sha256Digest::from_bytes([i as u8 + 1; 32]);
            profile.cpu_fit = Some(CpuFit::default());
            profile.qualified_targets = vec![QualifiedTarget {
                target: "x86_64-pc-windows-msvc".into(),
                execution_identity:
                    "onnxruntime-cpu/1.30.0/physical1/intra1/inter1/sequential/spinning0".into(),
                receipt: Sha256Digest::from_bytes([42; 32]),
            }];
            profile
                .task_policies
                .insert(Task::ContextNextRead, super::super::tasks::policy_digest()?);
            profile.abi.readout =
                if matches!(model, ModelChoice::SmallIntern | ModelChoice::LargeIntern) {
                    Readout::DecisionSymbol
                } else {
                    Readout::PointerHead
                };
            for download in &mut profile.downloads {
                for part in &mut download.parts {
                    let name = part.url.rsplit('/').next().unwrap();
                    part.url =
                        format!("https://github.com/iokaio/airk/releases/download/v1.9.0/{name}");
                }
            }
            profiles.push(profile);
        }
        let input = serde_json::json!({"schema":CATALOG_SCHEMA,"profiles":profiles});
        let positive = project(&serde_json::to_vec(&input)?, "v1.9.0")?;
        assert_eq!(positive.profiles.len(), 4);
        assert!(!positive.assets.is_empty());
        let mut cases = vec![
            serde_json::json!({"name":"hypothetical_four_final_slots","accepted":true,"projection":positive}),
        ];
        for (name, mut value) in [
            ("missing_slot", input.clone()),
            ("unqualified", input.clone()),
            ("missing_execution_fit", input.clone()),
            ("foreign_release_tag", input.clone()),
            ("conflicting_part_pin", input.clone()),
        ] {
            match name {
                "missing_slot" => {
                    value["profiles"].as_array_mut().unwrap().pop();
                }
                "unqualified" => value["profiles"][0]["qualification"] = serde_json::Value::Null,
                "missing_execution_fit" => {
                    value["profiles"][0]["cpu_fit"] = serde_json::Value::Null
                }
                "foreign_release_tag" => {
                    value["profiles"][0]["downloads"][0]["parts"][0]["url"] =
                        "https://github.com/iokaio/airk/releases/download/v0.0.0/foreign.bin".into()
                }
                _ => {
                    value["profiles"][0]["downloads"][0]["parts"][0]["sha256"] =
                        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".into()
                }
            }
            assert!(
                project(&serde_json::to_vec(&value)?, "v1.9.0").is_err(),
                "{name}"
            );
            cases.push(serde_json::json!({"name":name,"accepted":false}));
        }
        if let Some(path) = std::env::var_os("AIRK_ARBITER_PUBLICATION_CASES_OUTPUT") {
            std::fs::write(
                path,
                serde_json::to_vec_pretty(
                    &serde_json::json!({"schema":"airk.arbiter.publication-cases/1","hypothetical":true,"cases":cases}),
                )?,
            )?;
        }
        Ok(())
    }
}
