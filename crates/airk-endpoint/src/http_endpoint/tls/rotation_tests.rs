// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::Incarnation;

struct Fixture {
    _owner: tempfile::TempDir,
    control: PathBuf,
    active: PathBuf,
    old: PathBuf,
    current_old: PathBuf,
    new: PathBuf,
    binding: InstallationBinding,
    now: u64,
}
impl Fixture {
    fn new() -> Result<Self> {
        let owner = tempfile::tempdir()?;
        let control = owner.path().join("control");
        crate::local_ipc::prepare_root(&control)?;
        let registry = airk_store::service::registry::ServiceRegistry::open(
            &control,
            Incarnation::from_bytes([2; 16]),
        )?;
        let binding = InstallationBinding {
            installation: Sha256Digest::from_bytes([1; 32]),
            user: crate::local_ipc::current_user()?,
            namespace: registry.incarnation()?,
        };
        let active = control.join("http-1");
        let stage = active
            .join("issuer-rotations")
            .join(Incarnation::from_bytes([3; 16]).to_string());
        let now = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis(),
        )?;
        TlsIdentity::load_or_create(&active, binding.clone(), now)?;
        crate::local_ipc::prepare_root(&stage)?;
        Ok(Self {
            _owner: owner,
            control,
            active,
            old: stage.join("old"),
            current_old: stage.join("current-old"),
            new: stage.join("new"),
            binding,
            now,
        })
    }
    fn prepare(&self) -> Result<(TlsBundleIdentity, TlsBundleIdentity)> {
        let old = TlsIdentity::inspect_bundle(&self.active, &self.binding)?;
        TlsIdentity::clone_exact_bundle(&self.active, &self.old, &old)?;
        let new = TlsIdentity::create_staged_bundle(&self.new, self.binding.clone(), self.now)?;
        Ok((old, new))
    }
    fn bytes(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        Ok((
            std::fs::read(self.active.join(BUNDLE))?,
            std::fs::read(self.active.join(CA_FILE))?,
        ))
    }
}

fn assert_strict_certificate_profile(bundle: &Bundle) -> Result<()> {
    use x509_parser::{
        extensions::{GeneralName, ParsedExtension},
        oid_registry::{
            OID_X509_EXT_AUTHORITY_KEY_IDENTIFIER, OID_X509_EXT_SUBJECT_KEY_IDENTIFIER,
        },
    };
    let (ca_tail, ca) = x509_parser::parse_x509_certificate(&bundle.ca_der)?;
    let (leaf_tail, leaf) = x509_parser::parse_x509_certificate(&bundle.leaf_der)?;
    assert!(ca_tail.is_empty() && leaf_tail.is_empty());
    ca.extensions_map()?;
    leaf.extensions_map()?;
    assert_eq!(ca.subject(), ca.issuer());
    let constraints = ca.basic_constraints()?.unwrap();
    assert!(constraints.critical && constraints.value.ca);
    assert_eq!(constraints.value.path_len_constraint, Some(0));
    let ca_usage = ca.key_usage()?.unwrap();
    assert!(ca_usage.critical && ca_usage.value.key_cert_sign() && ca_usage.value.crl_sign());
    let ski = ca
        .get_extension_unique(&OID_X509_EXT_SUBJECT_KEY_IDENTIFIER)?
        .unwrap();
    assert!(!ski.critical);
    let ParsedExtension::SubjectKeyIdentifier(ski) = ski.parsed_extension() else {
        panic!("CA subject key identifier missing")
    };
    assert!(!ski.0.is_empty());
    let aki = leaf
        .get_extension_unique(&OID_X509_EXT_AUTHORITY_KEY_IDENTIFIER)?
        .unwrap();
    assert!(!aki.critical);
    let ParsedExtension::AuthorityKeyIdentifier(aki) = aki.parsed_extension() else {
        panic!("leaf authority key identifier missing")
    };
    assert_eq!(aki.key_identifier.as_ref().unwrap().0, ski.0);
    assert_eq!(leaf.issuer(), ca.subject());
    assert!(leaf.validity().not_before >= ca.validity().not_before);
    assert!(leaf.validity().not_after <= ca.validity().not_after);
    let usage = leaf.key_usage()?.unwrap();
    assert!(usage.critical && usage.value.digital_signature() && !usage.value.key_cert_sign());
    assert!(leaf.extended_key_usage()?.unwrap().value.server_auth);
    let san = leaf.subject_alternative_name()?.unwrap();
    assert_eq!(san.value.general_names.len(), 1);
    assert!(
        matches!(&san.value.general_names[0],GeneralName::IPAddress(address) if *address == &[127u8,0,0,1][..])
    );
    Ok(())
}

/// Exact old native issuance profile: signed leaf without AKI and self-signed
/// CA without optional AKI. Only this private fixture uses the old defect.
fn save_legacy_aki_missing_identity(f: &Fixture) -> Result<Bundle> {
    let mut bundle: Bundle = serde_json::from_slice(&f.bytes()?.0)?;
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name("AIRK legacy issuer fixture");
    params.not_before = date(f.now.saturating_sub(SKEW_MS))?;
    params.not_after = date(f.now)? + Duration::days(3650);
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    assert!(!params.use_authority_key_identifier_extension);
    let certificate = params.self_signed(&key)?;
    bundle.ca_der = certificate.der().to_vec();
    bundle.ca_pem = certificate.pem();
    bundle.ca_key_pem = key.serialize_pem();
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
    let issuer = Issuer::from_ca_cert_pem(&bundle.ca_pem, KeyPair::from_pem(&bundle.ca_key_pem)?)?;
    let mut params = leaf_parameters(f.now)?;
    params.use_authority_key_identifier_extension = false;
    bundle.leaf_der = params.signed_by(&key, &issuer)?.der().to_vec();
    bundle.leaf_key_der = key.serialize_der();
    write_private(
        &f.active,
        BUNDLE,
        &serde_json::to_vec(&bundle)?,
        MAX_BUNDLE,
        true,
    )?;
    write_private(&f.active, CA_FILE, bundle.ca_pem.as_bytes(), MAX_CA, true)?;
    Ok(bundle)
}

#[test]
fn native_fresh_renewed_and_staged_certificates_include_strict_issuer_extensions() -> Result<()> {
    let f = Fixture::new()?;
    let (_, new) = f.prepare()?;
    assert_strict_certificate_profile(&serde_json::from_slice::<Bundle>(&f.bytes()?.0)?)?;
    let new_bundle: Bundle = serde_json::from_slice(&exact_files(&f.new, &new)?.0)?;
    assert_strict_certificate_profile(&new_bundle)?;
    let original_ca = std::fs::read(f.active.join(CA_FILE))?;
    let original = TlsIdentity::inspect_bundle(&f.active, &f.binding)?;
    TlsIdentity::load_or_create(&f.active, f.binding.clone(), f.now + 24 * 86_400_000)?;
    let renewed = TlsIdentity::inspect_bundle(&f.active, &f.binding)?;
    assert_eq!(original.ca_sha256, renewed.ca_sha256);
    assert_ne!(original.certificate_sha256, renewed.certificate_sha256);
    assert_eq!(std::fs::read(f.active.join(CA_FILE))?, original_ca);
    assert_strict_certificate_profile(&serde_json::from_slice::<Bundle>(&f.bytes()?.0)?)?;
    Ok(())
}

#[test]
fn persisted_leaf_missing_aki_repairs_under_same_authenticated_issuer_without_rewriting_stages()
-> Result<()> {
    let f = Fixture::new()?;
    let legacy = save_legacy_aki_missing_identity(&f)?;
    assert!(!leaf_has_issuer_identifier(
        &legacy,
        &issuer_key_identifier(&legacy)?
    )?);
    let original = f.bytes()?;
    let old = TlsIdentity::inspect_bundle(&f.active, &f.binding)?;
    assert!(
        f.bytes()? == original,
        "read-only inspection must preserve old evidence"
    );
    TlsIdentity::clone_exact_bundle(&f.active, &f.old, &old)?;
    let staged = exact_files(&f.old, &old)?;
    let repaired = TlsIdentity::load_or_create(&f.active, f.binding.clone(), f.now + 1000)?;
    let bundle: Bundle = serde_json::from_slice(&f.bytes()?.0)?;
    assert_eq!(bundle.ca_der, legacy.ca_der);
    assert_eq!(bundle.ca_pem, legacy.ca_pem);
    assert_eq!(bundle.ca_key_pem, legacy.ca_key_pem);
    assert_eq!(bundle.created_unix_ms, legacy.created_unix_ms);
    assert_eq!(f.bytes()?.1, original.1);
    assert_eq!(repaired.ca_sha256(), old.ca_sha256);
    assert_ne!(bundle.leaf_der, legacy.leaf_der);
    assert_strict_certificate_profile(&bundle)?;
    assert!(
        exact_files(&f.old, &old)? == staged,
        "immutable predecessor stage must not be repaired"
    );
    let restarted = TlsIdentity::load_or_create(&f.active, f.binding.clone(), f.now + 2000)?;
    assert_eq!(
        repaired.certificate_sha256(),
        restarted.certificate_sha256()
    );
    Ok(())
}

#[test]
fn missing_aki_never_authorizes_repair_of_unauthenticated_saved_certificate() -> Result<()> {
    let f = Fixture::new()?;
    let mut legacy = save_legacy_aki_missing_identity(&f)?;
    let end = legacy.leaf_der.len() - 1;
    legacy.leaf_der[end] ^= 1;
    write_private(
        &f.active,
        BUNDLE,
        &serde_json::to_vec(&legacy)?,
        MAX_BUNDLE,
        true,
    )?;
    let before = f.bytes()?;
    assert!(TlsIdentity::load_or_create(&f.active, f.binding.clone(), f.now + 1000).is_err());
    assert!(
        f.bytes()? == before,
        "unknown original signature must preserve keys and trust bytes"
    );
    Ok(())
}

#[test]
fn deficient_saved_ca_never_gets_silently_replaced_during_leaf_repair() -> Result<()> {
    let f = Fixture::new()?;
    let mut legacy = save_legacy_aki_missing_identity(&f)?;
    let key = KeyPair::from_pem(&legacy.ca_key_pem)?;
    let mut parameters = CertificateParams::default();
    parameters.distinguished_name = distinguished_name("AIRK legacy issuer fixture");
    parameters.not_before = date(f.now.saturating_sub(SKEW_MS))?;
    parameters.not_after = date(f.now)? + Duration::days(3650);
    parameters.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    parameters.key_usages = vec![KeyUsagePurpose::CrlSign]; // No certificate-signing authority.
    let ca = parameters.self_signed(&key)?;
    legacy.ca_der = ca.der().to_vec();
    legacy.ca_pem = ca.pem();
    write_private(
        &f.active,
        BUNDLE,
        &serde_json::to_vec(&legacy)?,
        MAX_BUNDLE,
        true,
    )?;
    write_private(&f.active, CA_FILE, legacy.ca_pem.as_bytes(), MAX_CA, true)?;
    let before = f.bytes()?;
    assert!(TlsIdentity::load_or_create(&f.active, f.binding.clone(), f.now + 1000).is_err());
    assert!(
        f.bytes()? == before,
        "deficient issuer must require explicit replacement, never new trust bytes"
    );
    Ok(())
}

#[test]
fn staged_identity_is_generated_once_without_renewing_or_replacing_active_material() -> Result<()> {
    let f = Fixture::new()?;
    let original = f.bytes()?;
    let (old, new) = f.prepare()?;
    assert!(old.ca_sha256 != new.ca_sha256);
    assert!(
        TlsIdentity::create_staged_bundle(&f.new, f.binding.clone(), f.now + 24 * 86_400_000)?
            == new
    );
    assert!(TlsIdentity::inspect_bundle(&f.old, &f.binding)? == old);
    assert!(f.bytes()? == original);
    // Resume the exact bundle-first publication boundary without a new key.
    std::fs::remove_file(f.new.join(CA_FILE))?;
    assert!(TlsIdentity::create_staged_bundle(&f.new, f.binding.clone(), f.now)? == new);
    let uncertain = f.new.parent().unwrap().join("uncertain-new");
    crate::local_ipc::prepare_root(&uncertain)?;
    assert!(TlsIdentity::create_staged_bundle(&uncertain, f.binding.clone(), f.now).is_err());
    assert!(f.bytes()? == original);
    Ok(())
}

#[test]
fn publication_recovers_both_mixed_boundaries_and_refuses_unknown_material() -> Result<()> {
    let f = Fixture::new()?;
    let (old, new) = f.prepare()?;
    let old_files = exact_files(&f.old, &old)?;
    let new_files = exact_files(&f.new, &new)?;
    let exclusion = airk_store::service::ServiceLease::acquire(&f.control)?;
    write_private(&f.active, BUNDLE, &new_files.0, MAX_BUNDLE, true)?;
    assert!(TlsIdentity::inspect_bundle(&f.active, &f.binding).is_err());
    TlsIdentity::publish_staged_bundle(&f.active, &f.old, &f.new, &old, &new, true, &exclusion)?;
    assert!(TlsIdentity::inspect_bundle(&f.active, &f.binding)? == new);
    write_private(&f.active, BUNDLE, &old_files.0, MAX_BUNDLE, true)?;
    TlsIdentity::publish_staged_bundle(&f.active, &f.old, &f.new, &old, &new, true, &exclusion)?;
    assert!(TlsIdentity::inspect_bundle(&f.active, &f.binding)? == new);
    write_private(&f.active, CA_FILE, b"foreign public issuer", MAX_CA, true)?;
    let foreign = f.bytes()?;
    assert!(
        TlsIdentity::publish_staged_bundle(&f.active, &f.old, &f.new, &old, &new, true, &exclusion)
            .is_err()
    );
    assert!(f.bytes()? == foreign);
    assert!(exact_files(&f.old, &old)? == old_files);
    assert!(exact_files(&f.new, &new)? == new_files);
    Ok(())
}

#[test]
fn publication_rejects_crossed_fingerprints_bindings_exclusion_and_tampered_keys() -> Result<()> {
    let f = Fixture::new()?;
    let (old, new) = f.prepare()?;
    let original = f.bytes()?;
    let mut crossed = old.clone();
    crossed.bundle_sha256 = Sha256Digest::from_bytes([9; 32]);
    assert!(TlsIdentity::clone_exact_bundle(&f.active, &f.current_old, &crossed).is_err());
    assert!(!f.current_old.exists());
    let foreign = Fixture::new()?;
    let exclusion = airk_store::service::ServiceLease::acquire(&foreign.control)?;
    assert!(
        TlsIdentity::publish_staged_bundle(&f.active, &f.old, &f.new, &old, &new, true, &exclusion)
            .is_err()
    );
    let exclusion = airk_store::service::ServiceLease::acquire(&f.control)?;
    let mut wrong_binding = new.clone();
    wrong_binding.binding.namespace = Incarnation::from_bytes([8; 16]);
    assert!(
        TlsIdentity::publish_staged_bundle(
            &f.active,
            &f.old,
            &f.new,
            &old,
            &wrong_binding,
            true,
            &exclusion
        )
        .is_err()
    );
    let mut bundle: Bundle = serde_json::from_slice(&std::fs::read(f.new.join(BUNDLE))?)?;
    bundle.ca_key_pem = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?.serialize_pem();
    write_private(
        &f.new,
        BUNDLE,
        &serde_json::to_vec(&bundle)?,
        MAX_BUNDLE,
        true,
    )?;
    assert!(TlsIdentity::inspect_bundle(&f.new, &f.binding).is_err());
    assert!(
        TlsIdentity::publish_staged_bundle(&f.active, &f.old, &f.new, &old, &new, true, &exclusion)
            .is_err()
    );
    assert!(f.bytes()? == original);
    Ok(())
}

#[test]
fn stopped_snapshot_accepts_only_renewed_leaf_and_preserves_original_preparation() -> Result<()> {
    let f = Fixture::new()?;
    let (old, new) = f.prepare()?;
    let prepared = exact_files(&f.old, &old)?;
    TlsIdentity::load_or_create(&f.active, f.binding.clone(), f.now + 24 * 86_400_000)?;
    let renewed = TlsIdentity::inspect_bundle(&f.active, &f.binding)?;
    assert!(
        renewed.ca_sha256 == old.ca_sha256 && renewed.certificate_sha256 != old.certificate_sha256
    );
    let exclusion = airk_store::service::ServiceLease::acquire(&f.control)?;
    assert!(
        TlsIdentity::snapshot_current_predecessor(
            &f.active,
            &f.old,
            &f.current_old,
            &old,
            &exclusion
        )? == renewed
    );
    assert!(exact_files(&f.old, &old)? == prepared);
    assert!(
        TlsIdentity::publish_staged_bundle(&f.active, &f.old, &f.new, &old, &new, true, &exclusion)
            .is_err()
    );
    TlsIdentity::publish_staged_bundle(
        &f.active,
        &f.current_old,
        &f.new,
        &renewed,
        &new,
        true,
        &exclusion,
    )?;
    assert!(TlsIdentity::inspect_bundle(&f.active, &f.binding)? == new);
    // A changed issuer private key is never treated as a legitimate leaf refresh.
    let mut corrupt: Bundle = serde_json::from_slice(&prepared.0)?;
    corrupt.ca_key_pem = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?.serialize_pem();
    write_private(
        &f.active,
        BUNDLE,
        &serde_json::to_vec(&corrupt)?,
        MAX_BUNDLE,
        true,
    )?;
    write_private(&f.active, CA_FILE, &prepared.1, MAX_CA, true)?;
    assert!(
        TlsIdentity::snapshot_current_predecessor(
            &f.active,
            &f.old,
            &f.current_old,
            &old,
            &exclusion
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn expiring_old_issuer_is_inspected_without_renewal_and_staged_key_retirement_is_exact()
-> Result<()> {
    let f = Fixture::new()?;
    let (old, new) = f.prepare()?;
    let original = f.bytes()?;
    assert!(
        TlsIdentity::load_or_create(&f.active, f.binding.clone(), f.now + 3620 * 86_400_000)
            .is_err()
    );
    assert!(TlsIdentity::inspect_bundle(&f.active, &f.binding)? == old);
    assert!(f.bytes()? == original);
    write_private(
        &f.old,
        "retained-trust.json",
        b"owned trust receipt",
        4096,
        false,
    )?;
    assert!(TlsIdentity::retire_staged_private_key(&f.active, &old).is_err());
    TlsIdentity::retire_staged_private_key(&f.old, &old)?;
    TlsIdentity::retire_staged_private_key(&f.old, &old)?;
    assert!(!f.old.join(BUNDLE).exists());
    assert_eq!(std::fs::read(f.old.join(CA_FILE))?, original.1);
    assert_eq!(
        std::fs::read(f.old.join("retained-trust.json"))?,
        b"owned trust receipt"
    );
    // Explicit prepared-abort may retire the unused new stage, never active.
    TlsIdentity::retire_staged_private_key(&f.new, &new)?;
    TlsIdentity::retire_staged_private_key(&f.new, &new)?;
    assert!(f.bytes()? == original);
    Ok(())
}
