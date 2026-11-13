// SPDX-License-Identifier: Apache-2.0
//! Explicit issuer migration. A staged certificate never authorizes service startup.
use super::*;
use airk_core::service_http::EndpointDescriptor;
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const RECORD: &str = "issuer-rotation.json";
const LIMIT: usize = 32 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Prepared,
    Stopping,
    Activating,
    Restarting,
    Activated,
    Finalized,
    Aborted,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationBasis {
    LiveNativeHealth,
    StoppedNativeExclusion,
}
pub struct PrepareReview {
    pub expected_ca: Sha256Digest,
    pub expected_descriptor: Sha256Digest,
    pub overlap_seconds: u32,
    pub now: u64,
    pub basis: PreparationBasis,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub phase: Phase,
    pub revision: u64,
    pub preparation_basis: PreparationBasis,
    pub installation: Sha256Digest,
    pub user: String,
    pub namespace: Incarnation,
    pub old_ca_sha256: Sha256Digest,
    pub new_ca_sha256: Sha256Digest,
    pub old_ca_certificate_file: PathBuf,
    pub new_ca_certificate_file: PathBuf,
    pub old_endpoint: EndpointDescriptor,
    pub new_endpoint: Option<EndpointDescriptor>,
    pub overlap_expires_unix_ms: Option<u64>,
    pub overlap_seconds: u32,
    pub old_bundle: TlsBundleIdentity,
    pub activation_old_bundle: Option<TlsBundleIdentity>,
    pub stop_proof: Option<serde_json::Value>,
    pub new_bundle: TlsBundleIdentity,
}
impl Record {
    pub fn digest(&self) -> Result<Sha256Digest> {
        Ok(digest(&serde_json::to_vec(self)?))
    }
    pub fn binding(&self) -> InstallationBinding {
        InstallationBinding {
            installation: self.installation,
            user: self.user.clone(),
            namespace: self.namespace,
        }
    }
    pub fn old_root(&self) -> Result<&Path> {
        self.old_ca_certificate_file
            .parent()
            .context("rotation old issuer path unavailable")
    }
    pub fn activation_old_root(&self) -> Result<PathBuf> {
        Ok(self
            .old_root()?
            .parent()
            .context("rotation stage unavailable")?
            .join("current-old"))
    }
    pub fn new_root(&self) -> Result<&Path> {
        self.new_ca_certificate_file
            .parent()
            .context("rotation new issuer path unavailable")
    }
    fn validate(&self, control: &Path) -> Result<()> {
        let binding = installation_binding(control, self.namespace)?;
        ensure!(
            self.schema == 1
                && self.revision > 0
                && (60..=86_400).contains(&self.overlap_seconds)
                && self.binding() == binding,
            "issuer rotation installation changed"
        );
        self.old_endpoint.validate()?;
        ensure!(
            self.old_endpoint.installation == binding.installation
                && self.old_endpoint.namespace == binding.namespace
                && self.old_endpoint.user == binding.user
                && self.old_endpoint.ca_sha256 == self.old_ca_sha256,
            "issuer rotation predecessor changed"
        );
        if let Some(current) = &self.activation_old_bundle {
            ensure!(
                current.binding == binding && current.ca_sha256 == self.old_ca_sha256,
                "issuer activation predecessor changed authority"
            );
        }
        ensure!(
            !matches!(
                self.phase,
                Phase::Activating | Phase::Restarting | Phase::Activated | Phase::Finalized
            ) || self.activation_old_bundle.is_some(),
            "issuer activation predecessor missing"
        );
        let stage = stage_root(control, self.operation_id);
        ensure!(
            self.old_ca_certificate_file == stage.join("old/ca.pem")
                && self.new_ca_certificate_file == stage.join("new/ca.pem")
                && self.old_bundle.binding == binding
                && self.new_bundle.binding == binding
                && self.old_bundle.ca_sha256 == self.old_ca_sha256
                && self.new_bundle.ca_sha256 == self.new_ca_sha256
                && self.old_ca_sha256 != self.new_ca_sha256,
            "issuer rotation staged identity changed"
        );
        if let Some(endpoint) = &self.new_endpoint {
            endpoint.validate()?;
            ensure!(
                endpoint.installation == binding.installation
                    && endpoint.namespace == binding.namespace
                    && endpoint.user == binding.user
                    && endpoint.ca_sha256 == self.new_ca_sha256
                    && endpoint.generation > self.old_endpoint.generation
                    && endpoint.service_instance != self.old_endpoint.service_instance
                    && endpoint.executable_sha256 == self.old_endpoint.executable_sha256
                    && endpoint.state == airk_core::service_http::EndpointState::Ready,
                "issuer rotation successor changed"
            );
        }
        ensure!(
            !matches!(
                self.phase,
                Phase::Restarting | Phase::Activated | Phase::Finalized
            ) || self.overlap_expires_unix_ms.is_some(),
            "issuer rotation overlap unavailable"
        );
        ensure!(
            !matches!(self.phase, Phase::Activated | Phase::Finalized)
                || self.new_endpoint.is_some(),
            "issuer rotation verified successor unavailable"
        );
        Ok(())
    }
}

/// The issuer-rotation lock: an owner-private lock file (moved to `airk-ipc`, U90).
pub type MutationLock = airk_ipc::private_record::PrivateLock;
pub fn lock(control: &Path) -> Result<MutationLock> {
    airk_ipc::private_record::lock(
        &control.join("http-1"),
        "issuer-rotation.lock",
        "issuer rotation already active",
    )
}
fn stage_root(control: &Path, operation: Incarnation) -> PathBuf {
    control
        .join("http-1/issuer-rotations")
        .join(operation.to_string())
}
pub fn read(control: &Path) -> Result<Option<Record>> {
    match std::fs::symlink_metadata(control.join("http-1")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        result => {
            result?;
        }
    }
    read_private(&control.join("http-1"), RECORD, LIMIT)?
        .map(|bytes| {
            let record: Record = serde_json::from_slice(&bytes)?;
            record.validate(control)?;
            Ok(record)
        })
        .transpose()
}
pub fn reviewed(control: &Path, operation: Incarnation, expected: Sha256Digest) -> Result<Record> {
    let record = read(control)?.context("issuer rotation not prepared")?;
    ensure!(
        record.operation_id == operation && record.digest()? == expected,
        "issuer rotation changed after review"
    );
    Ok(record)
}
fn archive(control: &Path, record: &Record) -> Result<()> {
    let bytes = serde_json::to_vec(record)?;
    let stage = stage_root(control, record.operation_id);
    let name = format!("receipt-{}.json", record.revision);
    if let Some(saved) = read_private(&stage, &name, LIMIT)? {
        ensure!(saved == bytes, "issuer rotation history changed");
    } else {
        write_private(&stage, &name, &bytes, LIMIT, false)?;
    }
    Ok(())
}
fn write(control: &Path, record: &Record, replace: bool) -> Result<()> {
    record.validate(control)?;
    // Retain the previous authoritative receipt before replacing it. If the
    // new history copy is interrupted, the current record itself remains the
    // durable outcome and is archived before any later transition.
    if let Some(previous) = read(control)? {
        archive(control, &previous)?;
    }
    write_private(
        &control.join("http-1"),
        RECORD,
        &serde_json::to_vec(record)?,
        LIMIT,
        replace,
    )?;
    archive(control, record)
}

pub fn prepare(
    control: &Path,
    operation: Incarnation,
    endpoint: EndpointDescriptor,
    review: PrepareReview,
) -> Result<Record> {
    let PrepareReview {
        expected_ca,
        expected_descriptor,
        overlap_seconds,
        now,
        basis,
    } = review;
    ensure!(
        (60..=86_400).contains(&overlap_seconds),
        "issuer overlap must be 60..86400 seconds"
    );
    let binding = installation_binding(control, endpoint.namespace)?;
    ensure!(
        endpoint.ca_sha256 == expected_ca && endpoint.digest()? == expected_descriptor,
        "issuer or endpoint changed after review"
    );
    if let Some(record) = read(control)? {
        if record.operation_id == operation {
            ensure!(
                record.old_endpoint.digest()? == expected_descriptor
                    && record.overlap_seconds == overlap_seconds,
                "rotation operation reused with different review"
            );
            return Ok(record);
        }
        ensure!(
            matches!(record.phase, Phase::Finalized | Phase::Aborted),
            "finish the pending issuer rotation first"
        );
    }
    let root = control.join("http-1");
    let active = TlsIdentity::inspect_bundle(&root, &binding)?;
    ensure!(
        active.ca_sha256 == expected_ca,
        "active issuer changed after review"
    );
    let parent = root.join("issuer-rotations");
    crate::local_ipc::prepare_root(&parent)?;
    // Retained lineage is bounded; exhaustion requires explicit reviewed archival.
    ensure!(
        std::fs::read_dir(&parent)?.take(33).count() <= 32,
        "issuer rotation history capacity reached"
    );
    let stage = stage_root(control, operation);
    crate::local_ipc::prepare_root(&stage)?;
    let intent =
        serde_json::to_vec(&(operation, expected_descriptor, expected_ca, overlap_seconds))?;
    if let Some(saved) = read_private(&stage, "intent.json", 1024)? {
        ensure!(saved == intent, "issuer rotation staging intent changed");
    } else {
        write_private(&stage, "intent.json", &intent, 1024, false)?;
    }
    let old = stage.join("old");
    let new = stage.join("new");
    TlsIdentity::clone_exact_bundle(&root, &old, &active)?;
    let next = TlsIdentity::create_staged_bundle(&new, binding.clone(), now)?;
    let record = Record {
        schema: 1,
        operation_id: operation,
        phase: Phase::Prepared,
        revision: 1,
        preparation_basis: basis,
        installation: binding.installation,
        user: binding.user,
        namespace: binding.namespace,
        old_ca_sha256: active.ca_sha256,
        new_ca_sha256: next.ca_sha256,
        old_ca_certificate_file: old.join("ca.pem"),
        new_ca_certificate_file: new.join("ca.pem"),
        old_endpoint: endpoint,
        new_endpoint: None,
        overlap_expires_unix_ms: None,
        overlap_seconds,
        old_bundle: active,
        activation_old_bundle: None,
        stop_proof: None,
        new_bundle: next,
    };
    super::trust::snapshot_rotation_receipt(
        &root,
        record.old_root()?,
        record.binding(),
        record.old_ca_sha256,
    )?;
    write(control, &record, read(control)?.is_some())?;
    Ok(record)
}

pub fn record_activation_predecessor(
    control: &Path,
    record: &Record,
    current: TlsBundleIdentity,
    stop_proof: serde_json::Value,
) -> Result<Record> {
    let mut next = reviewed(control, record.operation_id, record.digest()?)?;
    ensure!(
        next.phase == Phase::Stopping
            && current.binding == next.binding()
            && current.ca_sha256 == next.old_ca_sha256,
        "issuer activation predecessor changed authority"
    );
    if let Some(saved) = &next.activation_old_bundle {
        ensure!(
            *saved == current,
            "issuer activation predecessor changed after capture"
        );
        return Ok(next);
    }
    next.activation_old_bundle = Some(current);
    next.stop_proof = Some(stop_proof);
    next.revision = next
        .revision
        .checked_add(1)
        .context("issuer rotation revision exhausted")?;
    write(control, &next, true)?;
    Ok(next)
}

pub fn advance(
    control: &Path,
    record: &Record,
    phase: Phase,
    now: u64,
    endpoint: Option<EndpointDescriptor>,
) -> Result<Record> {
    let current = reviewed(control, record.operation_id, record.digest()?)?;
    ensure!(
        matches!(
            (current.phase, phase),
            (Phase::Prepared, Phase::Aborted)
                | (Phase::Prepared, Phase::Stopping)
                | (Phase::Stopping, Phase::Activating)
                | (Phase::Activating, Phase::Restarting)
                | (Phase::Restarting, Phase::Activated)
                | (Phase::Activated, Phase::Finalized)
        ),
        "invalid issuer rotation transition"
    );
    let mut next = current;
    next.phase = phase;
    next.revision = next
        .revision
        .checked_add(1)
        .context("issuer rotation revision exhausted")?;
    if phase == Phase::Restarting {
        next.overlap_expires_unix_ms = Some(
            now.checked_add(u64::from(next.overlap_seconds) * 1000)
                .context("issuer rotation time overflow")?,
        );
    }
    if phase == Phase::Activated {
        next.new_endpoint = Some(endpoint.context("rotation successor endpoint missing")?);
    }
    if phase == Phase::Finalized {
        ensure!(
            now >= next
                .overlap_expires_unix_ms
                .context("rotation overlap missing")?,
            "issuer overlap has not expired"
        );
    }
    write(control, &next, true)?;
    Ok(next)
}

pub fn require_start_allowed(control: &Path) -> Result<()> {
    let Some(record) = read(control)? else {
        return Ok(());
    };
    ensure!(
        !matches!(record.phase, Phase::Stopping | Phase::Activating),
        "issuer rotation requires exact commit recovery before service startup"
    );
    let current = TlsIdentity::inspect_bundle(&control.join("http-1"), &record.binding())?;
    let expected = if matches!(record.phase, Phase::Prepared | Phase::Aborted) {
        record.old_ca_sha256
    } else {
        record.new_ca_sha256
    };
    ensure!(
        current.ca_sha256 == expected,
        "issuer differs from its committed rotation state"
    );
    Ok(())
}

pub fn require_committed_restart(
    control: &Path,
    operation: Incarnation,
    execution: Sha256Digest,
) -> Result<()> {
    let record = read(control)?.context("issuer rotation commit missing")?;
    ensure!(
        record.operation_id == operation
            && matches!(record.phase, Phase::Restarting | Phase::Activated)
            && record.old_endpoint.executable_sha256 == execution,
        "issuer restart commit identity changed"
    );
    require_start_allowed(control)
}

pub fn require_stop_owner(
    control: &Path,
    operation: Incarnation,
    instance: Incarnation,
    execution: Sha256Digest,
) -> Result<()> {
    let record = read(control)?.context("issuer rotation stop intent missing")?;
    ensure!(
        record.operation_id == operation
            && matches!(
                record.phase,
                Phase::Stopping | Phase::Activating | Phase::Restarting
            )
            && record.old_endpoint.service_instance == instance
            && record.old_endpoint.executable_sha256 == execution,
        "issuer rotation stop owner changed"
    );
    Ok(())
}
pub fn require_cleanup_allowed(control: &Path) -> Result<()> {
    if let Some(record) = read(control)? {
        ensure!(
            matches!(record.phase, Phase::Finalized | Phase::Aborted),
            "finish or recover the issuer rotation before uninstalling trust state"
        );
    }
    Ok(())
}

pub fn require_trust_change_allowed(control: &Path) -> Result<()> {
    if let Some(record) = read(control)? {
        ensure!(
            matches!(record.phase, Phase::Finalized | Phase::Aborted),
            "use the pending issuer rotation before changing active trust"
        );
    }
    Ok(())
}

/// The `runtime http-issuer-rotation` reply (moved from
/// `runtime_install/http_rotation.rs`, U90), also the service's M18 `Status`.
pub fn now_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
pub fn saved_endpoint(control: &Path) -> Result<Option<EndpointDescriptor>> {
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    let binding = installation_binding(control, registry.incarnation()?)?;
    load_descriptor(&control.join("http-1"), &binding)
}

pub fn output(
    control: &Path,
    trust_result: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let record = read(control)?;
    let endpoint = saved_endpoint(control)?;
    let digest = record.as_ref().map(Record::digest).transpose()?;
    let descriptor_digest = endpoint
        .as_ref()
        .map(EndpointDescriptor::digest)
        .transpose()?;
    let overdue = record.as_ref().is_some_and(|r| {
        !matches!(r.phase, Phase::Finalized | Phase::Aborted)
            && r.overlap_expires_unix_ms
                .is_some_and(|expires| now_ms().is_ok_and(|now| now >= expires))
    });
    Ok(
        serde_json::json!({"schema":1,"rotation":record,"receipt_sha256":digest,
        "current_endpoint":endpoint,"current_descriptor_sha256":descriptor_digest,
        "finalize_overdue":overdue,"trust_result":trust_result,
        "guidance":"Issuer replacement requires explicit client trust enrollment and reconnect. The overlap deadline does not remove OS trust automatically. Finalize only removes exact owned trust; unknown outcomes require review."}),
    )
}

#[cfg(test)]
mod tests;
