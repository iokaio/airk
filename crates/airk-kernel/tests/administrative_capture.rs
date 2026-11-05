// SPDX-License-Identifier: Apache-2.0
//! Native administrative capture retains policy and exact source locations.
use airk_core::{
    identity::Incarnation,
    index_catalog::{CatalogRevision, DenyId, IndexId, PathScope, SourceRootId},
    service::ProjectRegistration,
    source_policy::{DenyReason, DenyRule, POLICY_SCHEMA, SourcePolicy, SourceRoot},
};
use airk_kernel::{Kernel, service_configuration::ProtectedModelInput};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn owner() -> Result<tempfile::TempDir> {
    Ok(match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-admin-capture-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    })
}

fn restrict(kernel: &Kernel) -> Result<()> {
    let registration = ProjectRegistration::new(
        kernel
            .root()
            .canonicalize()?
            .to_str()
            .context("root UTF-8")?
            .into(),
        kernel
            .data_dir()
            .canonicalize()?
            .to_str()
            .context("data UTF-8")?
            .into(),
        kernel.collection().into(),
    )?;
    let root_id = SourceRootId::from_identity(Incarnation::from_bytes([122; 16]));
    let policy = SourcePolicy {
        schema: POLICY_SCHEMA.into(),
        index_id: IndexId::from_identity(Incarnation::from_bytes([121; 16])),
        owner_binding: registration.binding,
        namespace: "collections-1.4".into(),
        collection: kernel.collection().into(),
        epoch: CatalogRevision::INITIAL,
        roots: vec![SourceRoot {
            id: root_id,
            canonical_path: registration.root,
            aliases: vec![],
            owner: true,
        }],
        positive_includes: None,
        denies: vec![DenyRule {
            source_root_id: root_id,
            kind: PathScope::Subtree,
            path: "private".into(),
            rule_id: DenyId::from_identity(Incarnation::from_bytes([123; 16])),
            reason: DenyReason::Trim,
            created_epoch: CatalogRevision::INITIAL,
            inherited_from: None,
        }],
    };
    kernel.apply_source_policy(&policy, Incarnation::from_bytes([124; 16]))?;
    Ok(())
}

#[test]
fn native_administrative_pins_preserve_canonical_identity_and_permanent_denials() -> Result<()> {
    let owner = owner()?;
    let root = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir_all(root.join("private"))?;
    fs::create_dir_all(&data)?;
    fs::write(root.join("allowed.json"), b"{\"allowed\":true}")?;
    fs::write(root.join("private/denied.json"), b"{\"denied\":true}")?;
    let kernel = Kernel::open_service_collection(&root, Some(data), "default")?;
    kernel.configure_vectors(false)?;
    restrict(&kernel)?;
    let allowed = root.join("allowed.json").canonicalize()?;
    let pin = kernel.prepare_source_input(&allowed, 1024)?;
    assert_eq!(Path::new(&pin.path), allowed);
    assert_eq!(
        kernel.verified_source_input(&pin, 1024)?,
        b"{\"allowed\":true}"
    );
    assert!(
        kernel
            .prepare_source_input(&root.join("private/denied.json"), 1024)
            .is_err()
    );
    let denied_model = kernel
        .install_vector_model(&root.join("private"))
        .err()
        .context("denied model accepted")?;
    assert!(denied_model.is::<airk_kernel::SourcePolicyReadUnavailable>());
    fs::write(
        root.join("tokenizer.json"),
        br#"{"schema":2,"tokenizer_file":"asset.json"}"#,
    )?;
    let calibration = kernel
        .usage_calibrate(airk_kernel::usage::CalibrationRequest {
            tokenizer: &root.join("tokenizer.json"),
            baseline: &allowed,
            returned: &allowed,
            generation: 1,
            content_kind: airk_core::usage::ContentKind::PlainText,
            record: false,
        })
        .err()
        .context("denied calibration input directory accepted")?;
    assert!(calibration.is::<airk_kernel::SourcePolicyReadUnavailable>());
    fs::write(&allowed, b"{\"allowed\":false}")?;
    assert!(kernel.verified_source_input(&pin, 1024).is_err());
    Ok(())
}

fn model(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)?;
    let vocabulary = br#"{"amber":[1.0,0.0]}"#;
    fs::write(directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"administrative capture fixture","license":"Apache-2.0",
            "sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2,
        }))?,
    )?;
    Ok(())
}

#[test]
fn protected_custom_model_planning_preserves_complete_asset_identity() -> Result<()> {
    let owner = owner()?;
    let directory = owner.path().canonicalize()?.join("model");
    model(&directory)?;
    let plan = ProtectedModelInput(&directory).inspect_plan()?;
    assert_eq!(
        plan,
        airk_retrieval::model::inspect_object_plan(&directory)?
    );
    let root = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&root)?;
    fs::create_dir(&data)?;
    let kernel = Kernel::open(&root, Some(data))?;
    assert!(kernel.install_vector_model(&directory)?.enabled);
    Ok(())
}

#[cfg(unix)]
#[test]
fn pinned_inputs_and_model_assets_refuse_late_leaf_and_directory_redirects() -> Result<()> {
    use std::os::unix::fs::symlink;
    let owner = owner()?;
    let root = owner.path().canonicalize()?;
    let allowed = root.join("allowed");
    let denied = root.join("denied");
    model(&allowed)?;
    model(&denied)?;
    let prepared =
        airk_kernel::service_configuration::prepare_input(&allowed.join("model.json"), 16_384)?;
    fs::remove_file(allowed.join("model.json"))?;
    symlink(denied.join("model.json"), allowed.join("model.json"))?;
    assert!(airk_kernel::service_configuration::verified_input(&prepared, 16_384).is_err());
    assert!(ProtectedModelInput(&allowed).inspect_plan().is_err());
    fs::remove_file(allowed.join("model.json"))?;
    fs::rename(&allowed, root.join("retained-allowed"))?;
    symlink(&denied, &allowed)?;
    assert!(ProtectedModelInput(&allowed).inspect_plan().is_err());
    Ok(())
}
