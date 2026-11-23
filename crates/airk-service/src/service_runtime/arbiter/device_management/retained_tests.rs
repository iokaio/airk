// SPDX-License-Identifier: Apache-2.0
//! Retained `airk.arbiter.configuration/4` documents, the compiled device
//! catalog binding they were written under, and a device-status sample
//! (`fixtures/arbiter/configuration4`). They were emitted by the code as it was
//! before Large left the catalog (LH, commit 3), so a later state move is
//! tested against bytes an installation could actually hold.
use super::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Test identities of the retained documents (the values of the retained
/// `configuration-disabled-v2.json` fixture).
fn installation() -> airk_core::identity::Sha256Digest {
    airk_core::identity::Sha256Digest::from_bytes([1; 32])
}
fn source_catalog() -> airk_core::identity::Incarnation {
    airk_core::identity::Incarnation::from_bytes([2; 16])
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/arbiter/configuration4")
}

/// The documents as written: name, SHA-256 of the exact bytes, and the
/// Context selection each holds. The digests are pinned here, apart from the
/// code that wrote them, so a changed fixture is seen.
const RETAINED: [(&str, &str); 3] = [
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
];

#[cfg(feature = "local-inference")]
fn enabled(model: SelectedModel) -> Result<DeviceConfiguration> {
    let profile = runtime::catalog_profiles()?
        .into_iter()
        .find(|p| p.model == model.model())
        .context("compiled Context description absent")?;
    let mut doc = DeviceConfiguration::disabled(installation(), source_catalog(), binding()?);
    doc.revision = 2;
    doc.context.enabled = true;
    doc.context.selected_model = Some(model);
    doc.context.indexes = vec![airk_core::arbiter::Enrollment {
        index_id: airk_core::index_catalog::IndexId::from_identity(
            airk_core::identity::Incarnation::from_bytes([31; 16]),
        ),
        tasks: profile.tasks.clone(),
    }];
    doc.validate()?;
    Ok(doc)
}

/// Writes the retained documents once, into a fresh folder named by
/// `AIRK_LH_RETAINED_OUT`. Run before the state move; never overwrites.
#[cfg(feature = "local-inference")]
#[test]
#[ignore = "LH retained configuration/4 emitter; fresh output folder required"]
fn emit_retained_configuration4() -> Result<()> {
    let out = PathBuf::from(
        std::env::var("AIRK_LH_RETAINED_OUT").context("AIRK_LH_RETAINED_OUT names the folder")?,
    );
    std::fs::create_dir(&out).context("a fresh output folder")?;
    let write = |name: &str, bytes: &[u8]| -> Result<()> {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out.join(name))?
            .write_all(bytes)?;
        Ok(())
    };
    let binding = binding()?;
    let mut raw = serde_json::to_vec_pretty(&binding)?;
    raw.push(b'\n');
    write("binding.json", &raw)?;
    let mut raw = serde_json::to_vec_pretty(&runtime::device_catalog()?)?;
    raw.push(b'\n');
    write("catalog.json", &raw)?;
    let disabled = DeviceConfiguration::disabled(installation(), source_catalog(), binding);
    write("disabled.json", &disabled.canonical_bytes()?)?;
    write(
        "compact-enabled.json",
        &enabled(SelectedModel::ContextCompact)?.canonical_bytes()?,
    )?;
    write(
        "large-enabled.json",
        &enabled(SelectedModel::ContextLarge)?.canonical_bytes()?,
    )?;
    // A device-status sample from a real owner with an empty store: both
    // stages disabled, under the compiled binding.
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
    let status = owner.device_status()?;
    status.validate()?;
    let mut raw = serde_json::to_vec_pretty(&status)?;
    raw.push(b'\n');
    write("device-status.json", &raw)?;
    Ok(())
}

fn retained(name: &str) -> Result<Vec<u8>> {
    let bytes =
        std::fs::read(fixtures().join(name)).with_context(|| format!("retained fixture {name}"))?;
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The retained bytes are the pinned ones, and this build decodes each under
/// the binding it was written with.
#[test]
fn retained_configuration4_documents_are_pinned_and_decode() -> Result<()> {
    let binding: DeviceCatalogBinding = serde_json::from_slice(&retained("binding.json")?)?;
    let mut selected = vec![];
    for (name, digest) in RETAINED {
        let bytes = retained(name)?;
        assert_eq!(sha256(&bytes), digest, "{name} changed");
        let doc = DeviceConfiguration::decode(&bytes, installation(), source_catalog(), &binding)?;
        assert_eq!(doc.schema, "airk.arbiter.configuration/4", "{name}");
        assert!(!doc.focus.enabled, "{name}");
        selected.push((doc.context.enabled, doc.context.selected_model));
    }
    assert_eq!(
        selected,
        [
            (false, None),
            (true, Some(SelectedModel::ContextCompact)),
            (true, Some(SelectedModel::ContextLarge)),
        ]
    );
    Ok(())
}

/// Before the state move, the retained binding is the compiled one.
#[cfg(feature = "local-inference")]
#[test]
fn retained_configuration4_binding_is_the_compiled_binding() -> Result<()> {
    let retained_binding: DeviceCatalogBinding =
        serde_json::from_slice(&retained("binding.json")?)?;
    assert_eq!(retained_binding, binding()?);
    let catalog: DeviceCatalog = serde_json::from_slice(&retained("catalog.json")?)?;
    let profiles = runtime::catalog_profiles()?;
    assert_eq!(
        catalog.binding(retained_binding.source, |id| profiles
            .iter()
            .find(|p| p.manifest == id))?,
        retained_binding
    );
    let status: DeviceStatus = serde_json::from_slice(&retained("device-status.json")?)?;
    status.validate()?;
    assert_eq!(status.configuration.device_catalog, retained_binding);
    Ok(())
}
