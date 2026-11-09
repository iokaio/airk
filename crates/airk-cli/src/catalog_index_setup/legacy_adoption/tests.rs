// SPDX-License-Identifier: Apache-2.0
//! Actual owned configuration/image capture; no old program is executed and
//! these origin checks do not qualify a completed native archive migration.
use super::*;
use crate::runtime_legacy_origin::{ConfigFormat, ConfigLocator, NativeLegacyOrigin};
use airk_core::{
    catalog_legacy_adoption::source_inventory_digest,
    catalog_setup::{IndexSetupRequest, SetupSource},
    identity::Incarnation,
    index_catalog::CatalogRevision,
    service::{ProjectRegistration, RequestKey},
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn fixture(
    defaults: bool,
) -> Result<(
    tempfile::TempDir,
    NativeLegacyOrigin,
    LegacyAdoptionPlan,
    PathBuf,
    PathBuf,
)> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    let global = base.join("editor/User/globalStorage");
    let platform = if cfg!(windows) {
        "win32-x64"
    } else if cfg!(target_os = "macos") {
        "darwin-arm64"
    } else {
        "linux-x64"
    };
    let bytes = b"opaque predecessor origin fixture, never execute";
    let digest = format!("{:x}", Sha256::digest(bytes));
    let cache = global
        .join("iokaio.airk/helpers")
        .join(format!("1.0.0-build.7-{platform}-{}", &digest[..16]));
    std::fs::create_dir_all(&cache)?;
    let executable = cache.join(if cfg!(windows) { "airk.exe" } else { "airk" });
    std::fs::write(&executable, bytes)?;
    let root = base.join("missing-source");
    let mut argv = vec!["--root".to_owned(), root.to_str().unwrap().to_owned()];
    if !defaults {
        argv.extend([
            "--data-dir".into(),
            base.join("old-data").to_str().unwrap().into(),
            "--collection".into(),
            "saved".into(),
        ]);
    }
    argv.push("mcp".into());
    let config = base.join("original-config.json");
    let entry = json!({"command":executable,"args":argv,"disabledTools":["original-policy"]});
    std::fs::write(
        &config,
        serde_json::to_vec(&json!({"mcpServers":{"airk":entry,"other":entry}}))?,
    )?;
    let origin = crate::runtime_legacy_origin::inspect_at_global_storage(
        &ConfigLocator {
            path: config.clone(),
            format: ConfigFormat::McpServersJson,
            server: "airk".into(),
            container_path: None,
        },
        &global,
    )?;
    let scope = origin.standalone_scope()?;
    let original_registration = ProjectRegistration::new(
        scope.root.to_str().unwrap().into(),
        scope.data_dir.to_str().unwrap().into(),
        scope.collection.clone(),
    )?;
    let registration = ProjectRegistration::new(
        original_registration.root.clone(),
        base.join("private-new-index").to_str().unwrap().into(),
        original_registration.collection.clone(),
    )?;
    let plan = LegacyAdoptionPlan {
        schema: 1,
        setup: IndexSetupRequest {
            caller: Incarnation::from_bytes([1; 16]),
            key: RequestKey {
                namespace: Incarnation::from_bytes([2; 16]),
                epoch: 0,
                nonce: Incarnation::from_bytes([3; 16]),
            },
            expected_catalog_revision: CatalogRevision::INITIAL,
            name: "Origin fixture".into(),
            registration,
            sources: vec![SetupSource {
                canonical_path: original_registration.root.clone(),
                aliases: vec![],
                owner: true,
            }],
            setup_watch: false,
        },
        source_inventory_sha256: source_inventory_digest(
            &original_registration.root,
            &original_registration.collection,
            &BTreeMap::new(),
        )?,
        original_registration,
        origin_sha256: origin.identity()?,
        config_sha256: origin.observation().config_sha256.parse()?,
        executable_sha256: origin.observation().executable_sha256.parse()?,
        quiescence_sha256: Sha256Digest::from_bytes([4; 32]),
        archive_inventory_sha256: Sha256Digest::from_bytes([5; 32]),
        original_publication_sha256: Sha256Digest::from_bytes([6; 32]),
    };
    Ok((temp, origin, plan, config, global))
}
#[test]
fn independently_captured_alias_claim_shares_only_the_exact_native_original_scope() -> Result<()> {
    let (_temp, mut primary, plan, config, global) = fixture(false)?;
    let mut alias = crate::runtime_legacy_origin::inspect_at_global_storage(
        &ConfigLocator {
            path: config.clone(),
            format: ConfigFormat::McpServersJson,
            server: "other".into(),
            container_path: None,
        },
        &global,
    )?;
    assert_ne!(primary.identity()?, alias.identity()?);
    assert!(
        validate_origin(&plan, &mut alias).is_err(),
        "secondary origin replaced primary plan identity"
    );
    validate_scope(&plan, &mut alias)?;
    validate_origin(&plan, &mut primary)?;
    let first = crate::runtime_legacy_authority::configuration_claim(&primary)?;
    let second = crate::runtime_legacy_authority::configuration_claim(&alias)?;
    assert_eq!(first.config_sha256, second.config_sha256);
    assert_ne!(first.origin_sha256, second.origin_sha256);
    assert_ne!(first.original_server, second.original_server);
    let mut changed: serde_json::Value = serde_json::from_slice(&std::fs::read(&config)?)?;
    changed["mcpServers"]["other"]["args"][3] = json!(
        Path::new(&plan.original_registration.data_root).with_file_name("unrelated-original-data")
    );
    std::fs::write(&config, serde_json::to_vec(&changed)?)?;
    let mut crossed = crate::runtime_legacy_origin::inspect_at_global_storage(
        &ConfigLocator {
            path: config,
            format: ConfigFormat::McpServersJson,
            server: "other".into(),
            container_path: None,
        },
        &global,
    )?;
    assert!(
        validate_scope(&plan, &mut crossed).is_err(),
        "another native backing became a secondary authority claim"
    );
    assert!(!Path::new(&plan.original_registration.root).exists());
    Ok(())
}

#[test]
fn actual_origin_checks_preserve_missing_sources_and_verified_old_defaults() -> Result<()> {
    for defaults in [false, true] {
        let (_temp, mut origin, plan, _, _) = fixture(defaults)?;
        validate_origin(&plan, &mut origin)?;
        assert!(!Path::new(&plan.original_registration.root).exists());
        if defaults {
            assert_eq!(plan.original_registration.collection, "default");
            assert_eq!(
                Path::new(&plan.original_registration.data_root),
                Path::new(&plan.original_registration.root).join(".airk")
            );
        }
    }
    Ok(())
}
#[test]
fn another_original_server_cannot_reuse_the_same_file_and_index_proof() -> Result<()> {
    let (_temp, _origin, plan, config, global) = fixture(false)?;
    let mut other = crate::runtime_legacy_origin::inspect_at_global_storage(
        &ConfigLocator {
            path: config,
            format: ConfigFormat::McpServersJson,
            server: "other".into(),
            container_path: None,
        },
        &global,
    )?;
    assert_eq!(
        other.observation().config_sha256,
        plan.config_sha256.to_string()
    );
    assert!(validate_origin(&plan, &mut other).is_err());
    Ok(())
}
#[test]
fn changed_original_configuration_is_refused_before_native_authority() -> Result<()> {
    let (_temp, mut origin, plan, config, _) = fixture(false)?;
    std::fs::write(config, b"{}")?;
    assert!(validate_origin(&plan, &mut origin).is_err());
    Ok(())
}
#[cfg(windows)]
#[test]
fn missing_windows_saved_roots_keep_their_archived_spelling() -> Result<()> {
    let (_temp, mut origin, mut plan, _, _) = fixture(false)?;
    let plain = plan
        .original_registration
        .root
        .strip_prefix(r"\\?\")
        .unwrap()
        .to_owned();
    plan.original_registration = ProjectRegistration::new(
        plain.clone(),
        plan.original_registration.data_root.clone(),
        plan.original_registration.collection.clone(),
    )?;
    plan.setup.registration = ProjectRegistration::new(
        plain.clone(),
        plan.setup.registration.data_root.clone(),
        plan.setup.registration.collection.clone(),
    )?;
    plan.setup.sources[0].canonical_path = plain.clone();
    plan.source_inventory_sha256 = source_inventory_digest(
        &plain,
        &plan.original_registration.collection,
        &BTreeMap::new(),
    )?;
    validate_origin(&plan, &mut origin)?;
    assert_eq!(plan.original_registration.root, plain);
    Ok(())
}
