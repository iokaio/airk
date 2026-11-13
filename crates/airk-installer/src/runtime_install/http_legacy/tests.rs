// SPDX-License-Identifier: Apache-2.0
//! Legacy component compatibility, not modern public transport qualification.
use super::*;
use airk_core::identity::Incarnation;

#[test]
fn authority_zero_registration_keeps_review_required_without_adopted_schema() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let control = base.join("control");
    crate::local_ipc::prepare_root(&control)?;
    let source = base.join("original-source");
    std::fs::create_dir(&source)?;
    let original = source.join("original.txt");
    std::fs::write(&original, b"unchanged original source")?;
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        base.join("original-data").to_str().unwrap().into(),
        "original-collection".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([91; 16]))?;
    registry.register(&registration)?;
    assert!(registry.catalog_state()?.is_none());
    let receipt = Sha256Digest::from_bytes([92; 32]);
    match resolve(&mut registry, registration.clone(), receipt)? {
        LegacyMigrationResolution::ReviewRequired {
            registration: observed,
            configuration_receipt,
        } => {
            assert_eq!(observed, registration);
            assert_eq!(configuration_receipt, receipt);
        }
        LegacyMigrationResolution::Eligible { .. } => {
            panic!("authority-zero configuration must not gain imported authority")
        }
    }
    assert!(registry.catalog_state()?.is_none());
    assert_eq!(
        registry.get(registration.binding)?.registration,
        registration
    );
    assert_eq!(std::fs::read(original)?, b"unchanged original source");
    Ok(())
}
