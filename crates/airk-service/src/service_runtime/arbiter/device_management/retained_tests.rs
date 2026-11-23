// SPDX-License-Identifier: Apache-2.0
//! Retained `airk.arbiter.configuration/4` documents, the compiled device
//! catalog binding they were written under, and a device-status sample
//! (`fixtures/arbiter/configuration4`). They were emitted by the code as it was
//! before Large left the catalog (LH, commit 3: its ignored emitter
//! `emit_retained_configuration4` lives in that commit), so the state move is
//! tested against bytes an installation could actually hold.
use super::*;
#[cfg(feature = "local-inference")]
use airk_core::arbiter::device::RetiredModel;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Test identities of the retained documents (the values of the retained
/// `configuration-disabled-v2.json` fixture).
#[cfg(feature = "local-inference")]
fn installation() -> airk_core::identity::Sha256Digest {
    airk_core::identity::Sha256Digest::from_bytes([1; 32])
}
#[cfg(feature = "local-inference")]
fn source_catalog() -> airk_core::identity::Incarnation {
    airk_core::identity::Incarnation::from_bytes([2; 16])
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/arbiter/configuration4")
}

/// The documents as written, with the SHA-256 of their exact bytes. The
/// digests are pinned here, apart from the code that wrote them, so a changed
/// fixture is seen. `binding.json`, `catalog.json` and `device-status.json`
/// are pinned too: they describe the Large-era catalog and no longer decode
/// as current types.
const RETAINED: [(&str, &str); 6] = [
    (
        "disabled.json",
        "3fdf6238488e06b7e9723d96d2efb9060a4a9234e11ed8af9bf874936948c3e5",
    ),
    (
        "compact-enabled.json",
        "e341c87f4068afcfb18a66272c6cecd1a7265050d63ccfa20c985c0983071ff3",
    ),
    (
        "large-enabled.json",
        "785238b59d45e6b04b0476bd983d8eb0e53e86af836360577368caffc22124e1",
    ),
    (
        "binding.json",
        "d5cb439171b7db7a5da056b817d17af419fbd89ad61741f181af148529f23136",
    ),
    (
        "catalog.json",
        "5adb423b62240cb8cbbca4a7ea29b66cc751562c767403706a52b2a13d8d9235",
    ),
    (
        "device-status.json",
        "e48caa928005657aa35c4d2d0fda701c73d4ace241c7f13552bab3186fef283f",
    ),
];

fn retained(name: &str) -> Result<Vec<u8>> {
    let bytes =
        std::fs::read(fixtures().join(name)).with_context(|| format!("retained fixture {name}"))?;
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn retained_configuration4_files_are_unchanged() -> Result<()> {
    for (name, digest) in RETAINED {
        assert_eq!(sha256(&retained(name)?), digest, "{name} changed");
    }
    Ok(())
}

/// The compiled transition starts at the binding the retained documents were
/// written under and ends at the binding of the catalog compiled now.
#[cfg(feature = "local-inference")]
#[test]
fn the_compiled_transition_runs_from_the_retained_binding_to_the_compiled_one() -> Result<()> {
    let retained_binding: DeviceCatalogBinding =
        serde_json::from_slice(&retained("binding.json")?)?;
    let transition = configuration4_transition(binding()?)?;
    assert_eq!(transition.old, retained_binding);
    assert_eq!(transition.new, binding()?);
    assert_ne!(transition.old, transition.new);
    // The old catalog held three slots; the compiled one holds two.
    let old: serde_json::Value = serde_json::from_slice(&retained("catalog.json")?)?;
    assert_eq!(old["schema"], "airk.arbiter.device-catalog/1");
    assert!(old["context_large"].is_array() && old["context_compact"].is_array());
    let new = serde_json::to_value(runtime::device_catalog()?)?;
    assert_eq!(new["schema"], "airk.arbiter.device-catalog/2");
    assert_eq!(new["arbiter"], old["context_compact"]);
    assert_eq!(new["focus"], old["focus"]);
    assert!(new.get("context_large").is_none());
    Ok(())
}

/// Each retained document migrates under the compiled transition as entry 56
/// decided: disabled stays disabled; `context_compact` becomes `arbiter`; a
/// Large selection is cleared, the stage turned off with its enrollment
/// kept, and the retirement recorded. Arbiter is never selected for it.
#[cfg(feature = "local-inference")]
#[test]
fn retained_configuration4_documents_migrate_to_configuration5() -> Result<()> {
    let transition = configuration4_transition(binding()?)?;
    let migrate = |name: &str| -> Result<DeviceConfiguration> {
        DeviceConfiguration::migrate_v4(
            &retained(name)?,
            installation(),
            source_catalog(),
            &transition,
        )
    };
    let disabled = migrate("disabled.json")?;
    assert_eq!(disabled.schema, "airk.arbiter.configuration/5");
    assert_eq!(disabled.revision, 2);
    assert_eq!(disabled.device_catalog, binding()?);
    assert!(!disabled.focus.enabled && !disabled.context.enabled);
    assert_eq!(disabled.context.selected_model, None);
    assert_eq!(disabled.context_retired, None);

    let compact = migrate("compact-enabled.json")?;
    assert_eq!(compact.revision, 3);
    assert!(compact.context.enabled);
    assert_eq!(compact.context.selected_model, Some(SelectedModel::Arbiter));
    assert_eq!(compact.context.indexes.len(), 1);
    assert_eq!(compact.context_retired, None);

    let large = migrate("large-enabled.json")?;
    assert_eq!(large.revision, 3);
    assert!(!large.context.enabled);
    assert_eq!(large.context.selected_model, None);
    assert_eq!(large.context.indexes.len(), 1);
    assert_eq!(large.context_retired, Some(RetiredModel::ContextLarge));
    assert!(!large.focus.enabled);

    // Each result is canonical configuration/5 under the compiled binding.
    for doc in [&disabled, &compact, &large] {
        let bytes = doc.canonical_bytes()?;
        assert_eq!(
            &DeviceConfiguration::decode(&bytes, installation(), source_catalog(), &binding()?)?,
            doc
        );
    }
    // The retained documents no longer decode as current configuration.
    assert!(
        DeviceConfiguration::decode(
            &retained("compact-enabled.json")?,
            installation(),
            source_catalog(),
            &transition.old
        )
        .is_err()
    );
    Ok(())
}

/// The status of a retired Large selection says so.
#[cfg(feature = "local-inference")]
#[test]
fn a_retired_large_selection_is_reported_with_its_reason() -> Result<()> {
    let root = tempfile::tempdir()?;
    let owner = super::super::Owner::new(
        Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
        airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
            airk_core::identity::Incarnation::from_bytes([81; 16]),
            airk_core::identity::Incarnation::from_bytes([82; 16]),
            airk_core::index_catalog::CatalogRevision::INITIAL,
            1,
        )?,
    )?;
    let mut document = DeviceConfiguration::disabled(installation(), source_catalog(), binding()?);
    document.context_retired = Some(RetiredModel::ContextLarge);
    document.validate()?;
    let status = owner.context.device_stage_status(&document)?;
    assert_eq!(status.availability, Availability::Disabled);
    assert_eq!(status.reason, RETIRED_LARGE_REASON);
    let focus = owner.focus.device_stage_status(&document)?;
    assert_ne!(focus.reason, RETIRED_LARGE_REASON);
    Ok(())
}
