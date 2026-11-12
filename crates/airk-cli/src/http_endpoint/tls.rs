// SPDX-License-Identifier: Apache-2.0
use super::{InstallationBinding, digest, read_private, write_private};
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, PKCS_ECDSA_P256_SHA256, PublicKeyData, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject};
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
    sync::Arc,
};
use time::{Duration, OffsetDateTime};

const BUNDLE: &str = "tls-identity.json";
const CA_FILE: &str = "ca.pem";
const MAX_BUNDLE: usize = 65_536;
const MAX_CA: usize = 8192;
const SKEW_MS: u64 = 300_000;
const RENEW_MS: u64 = 7 * 86_400_000;
const RETIRED: &str = "retired-private-key.json";

/// Public fingerprints only. The corresponding private bundle is never
/// serialized into an activation journal, diagnostic, or error message.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsBundleIdentity {
    pub schema: u32,
    pub binding: InstallationBinding,
    pub ca_sha256: Sha256Digest,
    pub certificate_sha256: Sha256Digest,
    pub bundle_sha256: Sha256Digest,
    pub ca_pem_sha256: Sha256Digest,
}

fn checked_bundle(bytes: &[u8], binding: &InstallationBinding) -> Result<Bundle> {
    crate::http_endpoint::validate_binding(binding)?;
    let bundle: Bundle = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("invalid staged HTTP TLS identity record"))?;
    ensure!(
        bundle.schema == 1 && bundle.binding == *binding,
        "staged HTTP TLS binding mismatch"
    );
    let certificates = CertificateDer::pem_slice_iter(bundle.ca_pem.as_bytes())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        certificates.len() == 1 && certificates[0].as_ref() == bundle.ca_der.as_slice(),
        "staged HTTP issuer PEM and DER differ"
    );
    // Validate key/certificate encodings without imposing the current clock:
    // manual replacement must also be able to preserve an expiring old issuer.
    let issuer_key = KeyPair::from_pem(&bundle.ca_key_pem)
        .map_err(|_| anyhow::anyhow!("invalid staged HTTP issuer key"))?;
    let parsed = rustls::server::ParsedCertificate::try_from(&certificates[0])?;
    ensure!(
        parsed.subject_public_key_info().as_ref()
            == issuer_key.subject_public_key_info().as_slice(),
        "staged HTTP issuer private key differs from certificate"
    );
    rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(bundle.leaf_der.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bundle.leaf_key_der.clone())),
        )?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(bundle.ca_der.clone()))?;
    let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::new(rustls::crypto::ring::default_provider()),
    )
    .build()?;
    use rustls::client::danger::ServerCertVerifier;
    // Check the original signature/SAN at a time inside the saved leaf's life.
    // Current expiration is deliberately not permission to replace an issuer.
    verifier.verify_server_cert(
        &CertificateDer::from(bundle.leaf_der.clone()),
        &[],
        &rustls::pki_types::ServerName::try_from("127.0.0.1")?,
        &[],
        rustls::pki_types::UnixTime::since_unix_epoch(std::time::Duration::from_millis(
            bundle.leaf_expires_unix_ms.saturating_sub(1000),
        )),
    )?;
    Ok(bundle)
}

fn identity(bytes: &[u8], bundle: &Bundle) -> TlsBundleIdentity {
    TlsBundleIdentity {
        schema: 1,
        binding: bundle.binding.clone(),
        ca_sha256: digest(&bundle.ca_der),
        certificate_sha256: digest(&bundle.leaf_der),
        bundle_sha256: digest(bytes),
        ca_pem_sha256: digest(bundle.ca_pem.as_bytes()),
    }
}

fn exact_files(root: &Path, expected: &TlsBundleIdentity) -> Result<(Vec<u8>, Vec<u8>)> {
    ensure!(expected.schema == 1, "unsupported staged HTTP TLS identity");
    let bytes =
        read_private(root, BUNDLE, MAX_BUNDLE)?.context("staged HTTP key bundle missing")?;
    let bundle = checked_bundle(&bytes, &expected.binding)?;
    ensure!(
        identity(&bytes, &bundle) == *expected,
        "staged HTTP bundle identity changed"
    );
    let ca = read_private(root, CA_FILE, MAX_CA)?.context("staged HTTP issuer export missing")?;
    ensure!(
        ca == bundle.ca_pem.as_bytes(),
        "staged HTTP issuer export changed"
    );
    Ok((bytes, ca))
}

fn immutable_private(root: &Path, name: &str, bytes: &[u8], maximum: usize) -> Result<()> {
    match read_private(root, name, maximum)? {
        Some(existing) => ensure!(existing == bytes, "immutable staged HTTP identity changed"),
        None => write_private(root, name, bytes, maximum, false)?,
    }
    Ok(())
}

fn staged_key_root(root: &Path, expected: &TlsBundleIdentity) -> Result<()> {
    crate::local_ipc::require_existing_root(root)?;
    let root = root.canonicalize()?;
    ensure!(
        root.file_name()
            .is_some_and(|name| name == "old" || name == "current-old" || name == "new"),
        "HTTP key retirement is outside a named issuer stage"
    );
    let operation = root.parent().context("HTTP issuer operation missing")?;
    operation
        .file_name()
        .and_then(|name| name.to_str())
        .context("HTTP issuer operation name missing")?
        .parse::<airk_core::identity::Incarnation>()?;
    let rotations = operation.parent().context("HTTP issuer stages missing")?;
    ensure!(
        rotations
            .file_name()
            .is_some_and(|name| name == "issuer-rotations"),
        "HTTP issuer staging layout changed"
    );
    let active = rotations
        .parent()
        .context("HTTP active identity root missing")?;
    ensure!(
        active.file_name().is_some_and(|name| name == "http-1"),
        "HTTP identity root changed"
    );
    let control = active.parent().context("HTTP control root missing")?;
    ensure!(
        airk_store::service::registry::retained_namespace(control)? == expected.binding.namespace,
        "HTTP staged key belongs to another native namespace"
    );
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema: u32,
    binding: InstallationBinding,
    created_unix_ms: u64,
    ca_expires_unix_ms: u64,
    leaf_expires_unix_ms: u64,
    ca_pem: String,
    ca_der: Vec<u8>,
    ca_key_pem: String,
    leaf_der: Vec<u8>,
    leaf_key_der: Vec<u8>,
}

/// Keys are intentionally neither Debug nor printable. Only public DER digests
/// and a protected public-CA file reference leave native control.
pub struct TlsIdentity {
    config: Arc<rustls::ServerConfig>,
    ca_file: PathBuf,
    ca_sha256: Sha256Digest,
    certificate_sha256: Sha256Digest,
}
fn date(ms: u64) -> Result<OffsetDateTime> {
    Ok(OffsetDateTime::from_unix_timestamp(i64::try_from(
        ms / 1000,
    )?)?)
}
fn distinguished_name(name: &str) -> DistinguishedName {
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, name);
    dn
}
fn parsed_certificate(der: &[u8]) -> Result<x509_parser::certificate::X509Certificate<'_>> {
    let (remaining, certificate) = x509_parser::parse_x509_certificate(der)
        .map_err(|_| anyhow::anyhow!("invalid HTTP certificate encoding"))?;
    ensure!(remaining.is_empty(), "trailing HTTP certificate data");
    certificate
        .extensions_map()
        .map_err(|_| anyhow::anyhow!("duplicate HTTP certificate extension"))?;
    Ok(certificate)
}

/// Existing issuer bytes are immutable trust. A deficient CA requires explicit
/// replacement; leaf repair must never mint a replacement trusted issuer.
fn issuer_key_identifier(bundle: &Bundle) -> Result<Vec<u8>> {
    use x509_parser::{
        extensions::ParsedExtension,
        oid_registry::{
            OID_X509_EXT_AUTHORITY_KEY_IDENTIFIER, OID_X509_EXT_SUBJECT_KEY_IDENTIFIER,
        },
    };
    let ca = parsed_certificate(&bundle.ca_der)?;
    ensure!(
        ca.subject() == ca.issuer(),
        "HTTP issuer is not the owned self-signed root"
    );
    let constraints = ca
        .basic_constraints()?
        .context("HTTP issuer requires explicit replacement: missing CA constraints")?;
    ensure!(
        constraints.critical
            && constraints.value.ca
            && constraints.value.path_len_constraint == Some(0),
        "HTTP issuer requires explicit replacement: invalid CA constraints"
    );
    let usage = ca
        .key_usage()?
        .context("HTTP issuer requires explicit replacement: missing signing usage")?;
    ensure!(
        usage.critical && usage.value.key_cert_sign() && usage.value.crl_sign(),
        "HTTP issuer requires explicit replacement: invalid signing usage"
    );
    let extension = ca
        .get_extension_unique(&OID_X509_EXT_SUBJECT_KEY_IDENTIFIER)?
        .context("HTTP issuer requires explicit replacement: missing subject key identifier")?;
    let ParsedExtension::SubjectKeyIdentifier(identifier) = extension.parsed_extension() else {
        anyhow::bail!("HTTP issuer requires explicit replacement: invalid subject key identifier");
    };
    ensure!(
        !extension.critical && !identifier.0.is_empty() && identifier.0.len() <= 64,
        "HTTP issuer requires explicit replacement: invalid subject key identifier"
    );
    // RFC 5280 permits omitting AKI on a self-signed root. If one exists it must
    // name this same key; correcting issuer DER would change client trust.
    if let Some(authority) = ca.get_extension_unique(&OID_X509_EXT_AUTHORITY_KEY_IDENTIFIER)? {
        ensure!(
            !authority.critical
                && matches!(authority.parsed_extension(),
            ParsedExtension::AuthorityKeyIdentifier(key) if key.key_identifier.as_ref().is_some_and(|key| key.0 == identifier.0)),
            "HTTP issuer requires explicit replacement: invalid authority key identifier"
        );
    }
    Ok(identifier.0.to_vec())
}

fn leaf_has_issuer_identifier(bundle: &Bundle, expected: &[u8]) -> Result<bool> {
    use x509_parser::{
        extensions::ParsedExtension, oid_registry::OID_X509_EXT_AUTHORITY_KEY_IDENTIFIER,
    };
    let certificate = parsed_certificate(&bundle.leaf_der)?;
    let Some(extension) =
        certificate.get_extension_unique(&OID_X509_EXT_AUTHORITY_KEY_IDENTIFIER)?
    else {
        return Ok(false);
    };
    let ParsedExtension::AuthorityKeyIdentifier(identifier) = extension.parsed_extension() else {
        anyhow::bail!("invalid HTTP leaf authority key identifier");
    };
    Ok(!extension.critical
        && identifier
            .key_identifier
            .as_ref()
            .is_some_and(|key| key.0 == expected))
}

fn leaf_parameters(now: u64) -> Result<CertificateParams> {
    let mut parameters = CertificateParams::default();
    parameters.distinguished_name = distinguished_name("AIRK loopback service");
    parameters.subject_alt_names = vec![SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))];
    parameters.not_before = date(now.saturating_sub(SKEW_MS))?;
    parameters.not_after = date(now)? + Duration::days(30);
    parameters.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    parameters.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    // rcgen defaults to false. RFC 5280/OpenSSL strict verification requires the
    // issuing CA's key identifier on this non-self-signed server certificate.
    parameters.use_authority_key_identifier_extension = true;
    Ok(parameters)
}
fn leaf(bundle: &mut Bundle, now: u64) -> Result<()> {
    issuer_key_identifier(bundle)?;
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
    let issuer = Issuer::from_ca_cert_pem(&bundle.ca_pem, KeyPair::from_pem(&bundle.ca_key_pem)?)?;
    let parameters = leaf_parameters(now)?;
    let certificate = parameters.signed_by(&key, &issuer)?;
    bundle.leaf_der = certificate.der().to_vec();
    bundle.leaf_key_der = key.serialize_der();
    bundle.leaf_expires_unix_ms = now
        .checked_add(30 * 86_400_000)
        .ok_or_else(|| anyhow::anyhow!("HTTP certificate time overflow"))?;
    Ok(())
}
impl TlsIdentity {
    /// Nonmutating inspection: unlike load_or_create this never renews a leaf,
    /// repairs an export, or replaces an expired issuer.
    pub(crate) fn inspect_bundle(
        root: &Path,
        binding: &InstallationBinding,
    ) -> Result<TlsBundleIdentity> {
        let bytes = read_private(root, BUNDLE, MAX_BUNDLE)?.context("HTTP key bundle missing")?;
        let bundle = checked_bundle(&bytes, binding)?;
        let ca = read_private(root, CA_FILE, MAX_CA)?.context("HTTP issuer export missing")?;
        ensure!(
            ca == bundle.ca_pem.as_bytes(),
            "HTTP issuer export differs from key bundle"
        );
        Ok(identity(&bytes, &bundle))
    }

    /// Complete only missing immutable stage files. Existing material is never
    /// renewed or overwritten, including a resumed partial copy.
    pub(crate) fn clone_exact_bundle(
        active: &Path,
        staged_old: &Path,
        expected: &TlsBundleIdentity,
    ) -> Result<()> {
        let (bundle, ca) = exact_files(active, expected)?;
        crate::local_ipc::prepare_root(staged_old)?;
        ensure!(
            active.canonicalize()? != staged_old.canonicalize()?,
            "HTTP identity stage aliases active root"
        );
        immutable_private(staged_old, BUNDLE, &bundle, MAX_BUNDLE)?;
        immutable_private(staged_old, CA_FILE, &ca, MAX_CA)?;
        exact_files(staged_old, expected)?;
        Ok(())
    }

    /// Generate once in an absent owner-private stage. A directory surviving an
    /// interrupted attempt without a bundle is uncertain and cannot mint a
    /// second issuer. A missing public export can resume from its exact bundle.
    pub(crate) fn create_staged_bundle(
        staged_new: &Path,
        binding: InstallationBinding,
        now_unix_ms: u64,
    ) -> Result<TlsBundleIdentity> {
        match std::fs::symlink_metadata(staged_new) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Self::load_or_create(staged_new, binding.clone(), now_unix_ms)?;
            }
            Err(error) => return Err(error.into()),
            Ok(_) => {
                crate::local_ipc::require_existing_root(staged_new)?;
                let bytes = read_private(staged_new, BUNDLE, MAX_BUNDLE)?
                    .context("existing HTTP issuer stage has no known bundle")?;
                let bundle = checked_bundle(&bytes, &binding)?;
                immutable_private(staged_new, CA_FILE, bundle.ca_pem.as_bytes(), MAX_CA)?;
            }
        }
        Self::inspect_bundle(staged_new, &binding)
    }

    /// Freeze the latest legitimate leaf only after native stop/exclusion.
    /// The immutable prepared issuer/key remains the original approval proof.
    pub(crate) fn snapshot_current_predecessor(
        active: &Path,
        prepared_old: &Path,
        current_old: &Path,
        expected_prepared: &TlsBundleIdentity,
        exclusion: &airk_store::service::ServiceLease,
    ) -> Result<TlsBundleIdentity> {
        ensure!(
            active.canonicalize()? == exclusion.root().join("http-1").canonicalize()?
                && airk_store::service::registry::retained_namespace(exclusion.root())?
                    == expected_prepared.binding.namespace,
            "HTTP predecessor snapshot lacks exact native owner exclusion"
        );
        let (prepared_bytes, _) = exact_files(prepared_old, expected_prepared)?;
        let prepared = checked_bundle(&prepared_bytes, &expected_prepared.binding)?;
        let bytes = read_private(active, BUNDLE, MAX_BUNDLE)?
            .context("active HTTP predecessor bundle missing")?;
        let current = checked_bundle(&bytes, &expected_prepared.binding)?;
        ensure!(
            current.ca_der == prepared.ca_der
                && current.ca_pem == prepared.ca_pem
                && current.ca_key_pem == prepared.ca_key_pem
                && current.created_unix_ms == prepared.created_unix_ms
                && current.ca_expires_unix_ms == prepared.ca_expires_unix_ms,
            "HTTP predecessor issuer changed after preparation"
        );
        let expected = Self::inspect_bundle(active, &expected_prepared.binding)?;
        Self::clone_exact_bundle(active, current_old, &expected)?;
        Ok(expected)
    }

    /// The caller's actual native lease proves that no service can read this
    /// pair while it is changed. Each file may be old or new after interruption;
    /// anything else is preserved and refused before either write.
    pub(crate) fn publish_staged_bundle(
        active: &Path,
        staged_old: &Path,
        staged_new: &Path,
        expected_old: &TlsBundleIdentity,
        expected_new: &TlsBundleIdentity,
        desired_new: bool,
        exclusion: &airk_store::service::ServiceLease,
    ) -> Result<()> {
        crate::http_endpoint::validate_binding(&expected_old.binding)?;
        ensure!(
            expected_old.binding == expected_new.binding
                && expected_old.ca_sha256 != expected_new.ca_sha256,
            "crossed HTTP issuer rotation identities"
        );
        ensure!(
            active.canonicalize()? == exclusion.root().join("http-1").canonicalize()?
                && airk_store::service::registry::retained_namespace(exclusion.root())?
                    == expected_old.binding.namespace,
            "HTTP issuer publication lacks exact native owner exclusion"
        );
        ensure!(
            active.canonicalize()? != staged_old.canonicalize()?
                && active.canonicalize()? != staged_new.canonicalize()?
                && staged_old.canonicalize()? != staged_new.canonicalize()?,
            "HTTP issuer stages alias active material"
        );
        let old = exact_files(staged_old, expected_old)?;
        let new = exact_files(staged_new, expected_new)?;
        let current_bundle =
            read_private(active, BUNDLE, MAX_BUNDLE)?.context("active HTTP bundle is missing")?;
        let current_ca = read_private(active, CA_FILE, MAX_CA)?
            .context("active HTTP issuer export is missing")?;
        ensure!(
            (current_bundle == old.0 || current_bundle == new.0)
                && (current_ca == old.1 || current_ca == new.1),
            "active HTTP identity contains unknown material; preserved"
        );
        let (bundle, ca, expected) = if desired_new {
            (&new.0, &new.1, expected_new)
        } else {
            (&old.0, &old.1, expected_old)
        };
        if current_bundle != *bundle {
            write_private(active, BUNDLE, bundle, MAX_BUNDLE, true)?;
        }
        if current_ca != *ca {
            write_private(active, CA_FILE, ca, MAX_CA, true)?;
        }
        exact_files(active, expected)?;
        Ok(())
    }

    /// Finalization removes only this operation's exact staged old private
    /// bundle. The public issuer and all unrelated trust/receipt files remain.
    pub(crate) fn retire_staged_private_key(
        staged_old: &Path,
        expected: &TlsBundleIdentity,
    ) -> Result<()> {
        crate::http_endpoint::validate_binding(&expected.binding)?;
        ensure!(expected.schema == 1, "unsupported staged HTTP TLS identity");
        staged_key_root(staged_old, expected)?;
        let ca = read_private(staged_old, CA_FILE, MAX_CA)?
            .context("old staged issuer export missing")?;
        ensure!(
            digest(&ca) == expected.ca_pem_sha256,
            "old staged issuer export changed"
        );
        let marker = serde_json::to_vec(expected)?;
        match read_private(staged_old, BUNDLE, MAX_BUNDLE)? {
            Some(_) => {
                exact_files(staged_old, expected)?;
                immutable_private(staged_old, RETIRED, &marker, 4096)?;
                // Recheck the exact protected bytes immediately before unlink.
                exact_files(staged_old, expected)?;
                std::fs::remove_file(staged_old.join(BUNDLE))?;
                #[cfg(unix)]
                std::fs::File::open(staged_old)?.sync_all()?;
            }
            None => ensure!(
                read_private(staged_old, RETIRED, 4096)?.as_deref() == Some(marker.as_slice()),
                "old staged key absence has no exact retirement marker"
            ),
        }
        Ok(())
    }

    pub fn load_or_create(
        root: &Path,
        binding: InstallationBinding,
        now_unix_ms: u64,
    ) -> Result<Self> {
        crate::http_endpoint::validate_binding(&binding)?;
        crate::local_ipc::prepare_root(root)?;
        let existing = read_private(root, BUNDLE, MAX_BUNDLE)?;
        let mut changed = existing.is_none();
        let mut bundle = if let Some(bytes) = existing {
            // Authenticate the saved issuer key and original leaf signature
            // before repairing a legacy leaf under that same trusted issuer.
            let bundle = checked_bundle(&bytes, &binding)?;
            ensure!(
                now_unix_ms.saturating_add(SKEW_MS) >= bundle.created_unix_ms,
                "HTTP TLS identity clock moved backwards"
            );
            ensure!(
                bundle.ca_expires_unix_ms > now_unix_ms.saturating_add(31 * 86_400_000),
                "HTTP issuer requires explicit replacement and client re-enrollment"
            );
            bundle
        } else {
            let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
            let mut parameters = CertificateParams::default();
            parameters.distinguished_name =
                distinguished_name(&format!("AIRK local issuer {}", binding.installation));
            parameters.not_before = date(now_unix_ms.saturating_sub(SKEW_MS))?;
            parameters.not_after = date(now_unix_ms)? + Duration::days(3650);
            parameters.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
            parameters.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
            parameters.use_authority_key_identifier_extension = true;
            // NameConstraints are deferred until actual supported-client interoperability is qualified.
            let ca = parameters.self_signed(&key)?;
            let mut bundle = Bundle {
                schema: 1,
                binding,
                created_unix_ms: now_unix_ms,
                ca_expires_unix_ms: now_unix_ms
                    .checked_add(3650 * 86_400_000)
                    .ok_or_else(|| anyhow::anyhow!("HTTP issuer time overflow"))?,
                leaf_expires_unix_ms: 0,
                ca_pem: ca.pem(),
                ca_der: ca.der().to_vec(),
                ca_key_pem: key.serialize_pem(),
                leaf_der: Vec::new(),
                leaf_key_der: Vec::new(),
            };
            leaf(&mut bundle, now_unix_ms)?;
            bundle
        };
        let issuer_identifier = issuer_key_identifier(&bundle)?;
        if bundle.leaf_expires_unix_ms <= now_unix_ms.saturating_add(RENEW_MS)
            || !leaf_has_issuer_identifier(&bundle, &issuer_identifier)?
        {
            leaf(&mut bundle, now_unix_ms)?;
            changed = true;
        }
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(bundle.leaf_der.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bundle.leaf_key_der.clone())),
        )?;
        match read_private(root, CA_FILE, MAX_CA)? {
            Some(bytes) => ensure!(
                bytes == bundle.ca_pem.as_bytes(),
                "HTTP issuer export mismatch; explicit repair required"
            ),
            None => {
                // Publish the durable key bundle before the public export. A crash
                // between them can recover the identical issuer, never mint a new one.
                if changed {
                    write_private(
                        root,
                        BUNDLE,
                        &serde_json::to_vec(&bundle)?,
                        MAX_BUNDLE,
                        true,
                    )?;
                    changed = false;
                }
                write_private(root, CA_FILE, bundle.ca_pem.as_bytes(), MAX_CA, false)?;
            }
        }
        if changed {
            write_private(
                root,
                BUNDLE,
                &serde_json::to_vec(&bundle)?,
                MAX_BUNDLE,
                true,
            )?;
        }
        Ok(Self {
            config: Arc::new(config),
            ca_file: root.join(CA_FILE),
            ca_sha256: digest(&bundle.ca_der),
            certificate_sha256: digest(&bundle.leaf_der),
        })
    }
    pub fn ca_certificate_file(&self) -> &Path {
        &self.ca_file
    }
    pub fn ca_sha256(&self) -> Sha256Digest {
        self.ca_sha256
    }
    pub fn certificate_sha256(&self) -> Sha256Digest {
        self.certificate_sha256
    }
    pub fn server_config(&self) -> Arc<rustls::ServerConfig> {
        self.config.clone()
    }
    pub fn acceptor(&self) -> tokio_rustls::TlsAcceptor {
        self.config.clone().into()
    }
}

#[cfg(test)]
mod rotation_tests;
