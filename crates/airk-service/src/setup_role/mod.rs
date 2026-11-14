// SPDX-License-Identifier: Apache-2.0
//! The private `setup` and `service-login` roles of `airk` (split plan §9,
//! Phase 7; U13–U16, U20, U30–U33, U39, U50; `docs/split/contracts.md`
//! §5.5–§5.7). See `CONTRACT.md`.
//!
//! `setup` consumes one owner-private `airk-setup-operation/1` file and runs
//! the §5.6 transaction, recording every phase in the setup receipt
//! `<state>/setup/<operation_id>.json`. Decisions are pure (`plan`); file
//! adapters are in `files`; side effects call their existing owners
//! (`effects`). `service-login` is the login registration's entry: it serves
//! like `service-detached` in automatic mode after its authority check.
mod effects;
mod files;
mod inno;
mod mac_login;
mod predecessor_roster;
mod recovery;
mod transaction;
mod wrapper_lease;
pub use recovery::RecoveryField;
pub(crate) mod plan;
#[cfg(test)]
mod purge_tests;
#[cfg(test)]
mod tests;

use airk_core::{
    identity::{Incarnation, Sha256Digest},
    install_record::{
        Contracts, Executable, Executables, InstallPointer, InstallRecord, InstalledBy, Installer,
        Leftover, MAX_REPLACEMENT_NOTICE_LIFETIME_MS, POINTER_SCHEMA, RECORD_FILE, RECORD_SCHEMA,
        REPLACEMENT_NOTICE_SCHEMA, ReplacementNotice, SETUP_RECEIPT_SCHEMA, SetupFailure,
        SetupOperation, SetupPhase, SetupReceipt, SetupStep, Signing, TrustChoice, canonical,
    },
};
use anyhow::{Context, Result, bail, ensure};
use files::{Installed, PayloadInventory};
use plan::{Action, Host, INVENTORY_FILE, Mode};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The `airkctl` I2 contract this `airk` pairs with (`airkctl::contract::I2_CONTRACT`).
const I2_CONTRACT: u32 = 1;
/// The I4 record version this writer emits.
const I4_RECORD: u32 = 1;
/// The persisted state contract the service keeps (U26).
const STATE_CONTRACT: &str = "airk-state/1";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the `setup` role was asked to do with its operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupMode {
    /// Read-only: report predecessors, location problems and the existing
    /// installation (`<operation folder>/plan.json` and `plan.txt`). Takes no
    /// lock, writes no receipt and does not consume the operation.
    Plan,
    /// The steps before file placement (lock, retirement, drain).
    Prepare,
    /// Every remaining step.
    Complete,
    /// Seal finalized Inno EXE/DAT custody for this successful original setup.
    SealInno,
    /// Capture exact original removal ownership before a detached wrapper handoff.
    AdmitRemoval,
    RecoveryField(RecoveryField),
    WrapperLease,
}

/// An admitted `setup` request (role admission checked its syntax).
#[derive(Debug, PartialEq, Eq)]
pub struct SetupRequest {
    pub operation: PathBuf,
    pub mode: SetupMode,
    pub wrapper_script: Option<PathBuf>,
    pub wrapper_silent: bool,
}

/// The injected clock (Unix milliseconds).
pub(crate) type Clock = fn() -> u64;

fn system_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// The canonical image of this process and its SHA-256.
fn own_image() -> Result<(PathBuf, Sha256Digest)> {
    let path = std::env::current_exe()?.canonicalize()?;
    let (digest, _) = files::digest_file(&path)?;
    Ok((path, digest))
}

fn canonical_or_same(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Run the `setup` role. A failure is also written to `error.txt` beside the
/// operation file (best effort), where a platform installer that cannot read
/// the role's standard error shows it.
pub async fn run(request: SetupRequest) -> Result<()> {
    let folder = request.operation.parent().map(Path::to_path_buf);
    let result = run_with(request, system_clock).await;
    if let (Err(error), Some(folder)) = (&result, folder) {
        let message = format!("{error:#}\n");
        let _ = files::write_owner_file(&folder, "error.txt", message.as_bytes());
    }
    result
}

async fn run_with(request: SetupRequest, clock: Clock) -> Result<()> {
    wrapper_lease::protect_inherited()?;
    if let SetupMode::RecoveryField(field) = request.mode {
        return recovery::emit(&request, field, clock);
    }
    if request.mode == SetupMode::WrapperLease {
        return wrapper_lease::exec_script(&request, clock);
    }
    if request.mode == SetupMode::SealInno {
        return inno::seal(&request);
    }
    let bytes = files::read_operation(&request.operation)?;
    let operation_sha256 = files::digest_bytes(&bytes);
    // The receipt is found through the operation's own state location; the
    // operation is fully decoded (schema, lifetime, locations) right after.
    let preview: SetupOperation = serde_json::from_slice(&bytes)
        .context("setup_operation_refused: install_file_malformed")?;
    let state = PathBuf::from(&preview.locations.state);
    ensure!(
        state.is_absolute(),
        "setup_operation_refused: install_location_relative"
    );
    let receipt = files::read_receipt(&state, &preview.operation_id)?;
    let pointer_path = files::pointer_path()?;
    let active_admission = transaction::admission_time(&pointer_path, &preview, operation_sha256)?;
    // First admission is checked against the clock; a resume against the
    // admission time its receipt recorded (the bytes are bound by digest).
    let admitted_at = active_admission
        .or_else(|| {
            receipt
                .as_ref()
                .and_then(|receipt| receipt.steps.first())
                .map(|step| step.at_unix_ms)
        })
        .unwrap_or_else(clock);
    let operation = SetupOperation::decode(&bytes, admitted_at)
        .map_err(|refusal| anyhow::anyhow!("setup_operation_refused: {}", refusal.code()))?;
    if let Some(receipt) = &receipt {
        let mut observation = receipt.clone();
        if active_admission.is_some() && observation.failure.is_some() {
            observation.closed = false;
        }
        plan::admit_resume(&observation, &operation, operation_sha256)
            .map_err(|code| anyhow::anyhow!("{code}: operation {}", operation.operation_id))?;
    }
    let mode = match request.mode {
        SetupMode::Plan | SetupMode::Prepare | SetupMode::SealInno => Mode::Prepare,
        SetupMode::RecoveryField(_) | SetupMode::WrapperLease => {
            unreachable!("recovery dispatched before setup")
        }
        SetupMode::Complete | SetupMode::AdmitRemoval => Mode::Complete,
    };
    if plan::removes(operation.kind)
        && !matches!(request.mode, SetupMode::Complete | SetupMode::AdmitRemoval)
    {
        bail!(
            "setup_prepare_not_applicable: {:?} runs as one step",
            operation.kind
        );
    }
    ensure!(
        request.mode != SetupMode::AdmitRemoval || plan::removes(operation.kind),
        "setup_admission_not_applicable: admission-only is for removal"
    );
    let program = PathBuf::from(&operation.locations.program);
    let inventory_path = match request.mode {
        SetupMode::Complete | SetupMode::AdmitRemoval => program.join(INVENTORY_FILE),
        SetupMode::RecoveryField(_) | SetupMode::WrapperLease => {
            unreachable!("recovery dispatched before setup")
        }
        SetupMode::Plan | SetupMode::Prepare | SetupMode::SealInno => request
            .operation
            .parent()
            .context("operation folder")?
            .join(INVENTORY_FILE),
    };
    let external_inventory = request
        .operation
        .parent()
        .context("operation folder")?
        .join(INVENTORY_FILE);
    let inventory_path = if external_inventory.try_exists()? {
        external_inventory
    } else {
        inventory_path
    };
    let inventory = files::read_inventory(&inventory_path, operation.payload_inventory_sha256)?;
    let _lock = if request.mode == SetupMode::Plan {
        None
    } else {
        Some(files::lock()?)
    };
    let owner = if request.mode == SetupMode::Plan {
        None
    } else {
        transaction::Owner::load(&pointer_path, &operation)?
    };
    let existing = match &owner {
        Some(owner) => owner.existing(&pointer_path)?,
        None => files::read_installation()?,
    };
    let (own_path, own_digest) = own_image()?;
    authorize_image(
        &operation,
        &inventory,
        existing.as_ref(),
        request.mode,
        &own_path,
        own_digest,
        owner.is_some(),
    )?;
    let mut setup = Setup {
        receipt: receipt.unwrap_or_else(|| SetupReceipt {
            schema: SETUP_RECEIPT_SCHEMA.into(),
            operation_id: operation.operation_id,
            operation_sha256,
            kind: operation.kind,
            expires_unix_ms: operation.expires_unix_ms,
            steps: Vec::new(),
            retired: Vec::new(),
            leftovers: Vec::new(),
            failure: None,
            closed: false,
        }),
        operation,
        operation_path: request.operation.clone(),
        mode,
        inventory,
        existing,
        clock,
        host: Host::current(),
        readiness: None,
        owner,
        retained: None,
    };
    if request.mode == SetupMode::Plan {
        if setup.existing.is_none() {
            setup.retained = transaction::retained_for(&pointer_path, &setup.operation.locations)?;
        }
        if setup.operation.requested_by == "airkctl-portable" {
            setup.verify_payload()?;
        }
        return setup.plan_report();
    }
    if request.mode == SetupMode::AdmitRemoval {
        use airk_core::install_record::setup_recovery::active::{
            EXPECTED_POINTER_FILE, EXPECTED_RECORD_FILE,
        };
        let folder = setup.operation_path.parent().context("operation folder")?;
        let pointer = InstallPointer::decode(&files::read_bounded(
            &folder.join(EXPECTED_POINTER_FILE),
            airk_core::install_record::MAX_POINTER_BYTES,
            true,
        )?)
        .map_err(|error| anyhow::anyhow!("removal_ownership_unproven: {}", error.code()))?;
        let record = InstallRecord::decode(
            &files::read_bounded(
                &folder.join(EXPECTED_RECORD_FILE),
                airk_core::install_record::MAX_RECORD_BYTES,
                true,
            )?,
            &pointer,
        )
        .map_err(|error| anyhow::anyhow!("removal_ownership_unproven: {}", error.code()))?;
        let actual = setup
            .existing
            .as_ref()
            .context("removal_ownership_unproven: original installation missing")?;
        ensure!(
            actual.pointer == pointer && actual.record == record,
            "removal_ownership_unproven: the verified original I4 image changed before admission"
        );
    }
    if setup.receipt.closed && setup.receipt.failure.is_some() {
        setup
            .owner
            .as_mut()
            .context("failed_setup_ownership_unproven: active owner missing")?
            .reopen_failed(&mut setup.receipt)?;
    }
    if let Some(owner) = &setup.owner
        && !plan::removes(setup.operation.kind)
    {
        let saved = &owner.candidate()?.record;
        let mut expected = setup.candidate_record()?;
        expected.installed_by.installed_unix_ms = saved.installed_by.installed_unix_ms;
        ensure!(
            &expected == saved,
            "setup_owner_refused: candidate differs from the actual approved inventory"
        );
    }
    if !plan::removes(setup.operation.kind) && setup.receipt.reached(SetupPhase::Placed) {
        setup.verify_payload()?;
    }
    if setup.owner.is_none() {
        let admission_roots = transaction::admission_roots(&setup.operation.locations)?;
        if setup.existing.is_none() && !plan::removes(setup.operation.kind) {
            setup.retained = transaction::retained_for(&pointer_path, &setup.operation.locations)?;
        }
        setup.compatibility_preflight()?;
        let problems = setup.location_problems()?;
        ensure!(
            problems.is_empty(),
            "location_refused: {}",
            problems.join("; ")
        );
        if let Some(installed) = &setup.existing
            && plan::removes(setup.operation.kind)
            && let Some(approved) = transaction::approved_inventory(&pointer_path, installed)?
        {
            ensure!(
                approved == setup.operation.payload_inventory_sha256,
                "payload_inventory_mismatch: removal inventory differs from the original completed setup"
            );
        }
        let candidate = if plan::removes(setup.operation.kind) {
            None
        } else {
            Some(setup.candidate_image()?)
        };
        setup.owner = Some(transaction::Owner::admit(
            &pointer_path,
            &setup.operation,
            admitted_at,
            setup.existing.as_ref(),
            candidate,
            Some(&admission_roots),
        )?);
    }
    setup
        .owner
        .as_mut()
        .context("setup owner missing")?
        .ensure_roots()?;
    if request.mode == SetupMode::AdmitRemoval {
        println!(
            "{}",
            json!({"schema":"airk-setup-result/1", "operation_id": setup.operation.operation_id,
            "kind":setup.operation.kind, "status":"admitted", "receipt":files_receipt_path(&state, setup.operation.operation_id),
            "retired":0,"leftovers":[]})
        );
        return Ok(());
    }
    let actions = plan::remaining(
        setup.operation.kind,
        &setup.operation.options,
        mode,
        &setup.recorded(),
    )
    .map_err(|code| anyhow::anyhow!("{code}"))?;
    for action in actions {
        setup
            .owner
            .as_mut()
            .context("setup owner missing")?
            .revalidate()?;
        if let Err(error) = setup.perform(action).await {
            // Editor windows reconnect to whatever the failure left in place.
            let _ = files::remove_replacement_notice();
            return setup.fail(error);
        }
        if action == Action::Drain
            && setup.owner.as_ref().is_some_and(|owner| {
                owner.active.stage
                    == airk_core::install_record::setup_recovery::active::Stage::Admitted
            })
        {
            setup
                .owner
                .as_mut()
                .context("setup owner missing")?
                .advance(airk_core::install_record::setup_recovery::active::Stage::Prepared)?;
        }
    }
    if setup.receipt.closed {
        // Kept through placement (`awaiting_placement`); withdrawn once the
        // operation is closed. A failed removal expires on its own.
        let _ = files::remove_replacement_notice();
    }
    let status = if setup.receipt.closed {
        "complete"
    } else {
        "awaiting_placement"
    };
    println!(
        "{}",
        json!({
            "schema": "airk-setup-result/1",
            "operation_id": setup.receipt.operation_id,
            "kind": setup.receipt.kind,
            "status": status,
            "receipt": files_receipt_path(&state, setup.receipt.operation_id),
            "retired": setup.receipt.retired.len(),
            "leftovers": setup.receipt.leftovers,
        })
    );
    match setup.readiness.take() {
        Some((code, message)) => bail!("{code}: {message}"),
        None => Ok(()),
    }
}

/// Pure: the notice an installer shows when earlier installations were left
/// in place, or `None` when nothing was left.
pub(crate) fn leftover_notice(leftovers: &[Leftover]) -> Option<String> {
    if leftovers.is_empty() {
        return None;
    }
    let mut text = String::from(
        "AIRK Service is installed, but setup could not remove these earlier AIRK items. They are still installed and may still be running:\n",
    );
    for leftover in leftovers {
        text.push_str(&format!("  - {}: {}\n", leftover.item, leftover.reason));
        if leftover.restarts_old_service {
            text.push_str("    (this can start the old AIRK service again at sign-in)\n");
        }
    }
    Some(text)
}

fn files_receipt_path(state: &Path, operation_id: Incarnation) -> String {
    state
        .join("setup")
        .join(format!("{operation_id}.json"))
        .display()
        .to_string()
}

/// The role's own image must be the payload's `airk` (§5.7 "own signature or
/// pinned digest verified"): its digest equals the inventory the operation
/// pins; once placed, it also runs from the program folder; and when an
/// installation record exists for these locations, it is that record's `airk`.
fn authorize_image(
    operation: &SetupOperation,
    inventory: &PayloadInventory,
    existing: Option<&Installed>,
    mode: SetupMode,
    own_path: &Path,
    own_digest: Sha256Digest,
    prepared_custody: bool,
) -> Result<()> {
    let listed = &inventory.files[&inventory.executables.airk];
    ensure!(
        listed.sha256 == own_digest,
        "role_admission_refused: this airk image is not the payload's airk"
    );
    let signing = inventory
        .signing
        .as_ref()
        .context("payload_unsigned: missing signing identity")?;
    airk_process::signature::verify(signature_claim(signing), &[own_path])
        .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))?;
    if matches!(mode, SetupMode::Complete | SetupMode::AdmitRemoval) {
        let program = PathBuf::from(&operation.locations.program);
        let placed = canonical_or_same(&plan::program_file(&program, &inventory.executables.airk));
        if plan::removes(operation.kind) {
            let installed = existing
                .filter(|installed| installed.record.locations == operation.locations)
                .context("removal_ownership_unproven: no exact original installation")?;
            ensure!(
                installed.record.executables.airk.sha256 == own_digest,
                "role_admission_refused: this airk image is not the installed airk"
            );
            airk_process::signature::verify(
                signature_claim(&installed.record.signing),
                &[own_path],
            )
            .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))?;
        } else {
            ensure!(
                placed == own_path || prepared_custody,
                "role_admission_refused: setup must run from {} or its exact prepared owner",
                placed.display()
            );
        }
    }
    Ok(())
}

/// A record's signing as the shared signature check reads it.
fn signature_claim(signing: &Signing) -> airk_process::signature::Claim<'_> {
    use airk_process::signature::Claim;
    match signing {
        Signing::Windows {
            publisher,
            identity_eku,
        } => Claim::Windows {
            publisher,
            identity_eku,
        },
        Signing::Macos { team_id } => Claim::Macos { team_id },
        Signing::Linux { signed, integrity } => Claim::Linux {
            signed: *signed,
            integrity,
        },
    }
}

fn verify_existing(pointer: &Path) -> Result<()> {
    let discovery = airk_ipc::install_reader::read(pointer, |signing, images| {
        airk_process::signature::verify(signature_claim(signing), images)
            .map(|_| ())
            .map_err(|error| (error.code.into(), error.message))
    });
    match discovery {
        airk_ipc::install_reader::Discovery::Verified(_) => Ok(()),
        airk_ipc::install_reader::Discovery::NotFound => {
            bail!("removal_ownership_unproven: installation disappeared")
        }
        airk_ipc::install_reader::Discovery::Unverified { code, message } => {
            bail!("{code}: {message}")
        }
    }
}

/// One running setup operation.
struct Setup {
    operation: SetupOperation,
    operation_path: PathBuf,
    mode: Mode,
    inventory: PayloadInventory,
    existing: Option<Installed>,
    receipt: SetupReceipt,
    clock: Clock,
    host: Host,
    /// A start/health failure: reported at the end, after the remaining
    /// independent steps, never hidden (§9.6.4).
    readiness: Option<(String, String)>,
    owner: Option<transaction::Owner>,
    retained: Option<airk_core::install_record::setup_recovery::RetainedInstallation>,
}

fn bounded(text: String) -> String {
    let limit = airk_core::install_record::MAX_SETUP_TEXT;
    if text.len() <= limit {
        return text;
    }
    let mut end = limit - 3;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

impl Setup {
    fn recorded(&self) -> Vec<SetupPhase> {
        self.receipt.steps.iter().map(|step| step.phase).collect()
    }

    fn state(&self) -> PathBuf {
        PathBuf::from(&self.operation.locations.state)
    }

    fn program(&self) -> PathBuf {
        PathBuf::from(&self.operation.locations.program)
    }

    /// Append one step and make the receipt durable.
    fn record(
        &mut self,
        phase: SetupPhase,
        outcome: &str,
        detail: impl Into<String>,
    ) -> Result<()> {
        ensure!(
            self.receipt.steps.len() < airk_core::install_record::MAX_SETUP_STEPS,
            "setup receipt step budget exhausted"
        );
        self.receipt.steps.push(SetupStep {
            phase,
            at_unix_ms: (self.clock)(),
            outcome: bounded(outcome.to_owned()),
            detail: bounded(detail.into()),
        });
        self.save_receipt()
    }

    fn save_receipt(&self) -> Result<()> {
        let state = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .pinned_root("state")?
            .context("setup state missing")?;
        files::write_receipt(&state, &self.receipt)
    }

    fn leave(&mut self, item: impl Into<String>, reason: impl Into<String>, restarts: bool) {
        let context = self
            .state()
            .join("setup")
            .join(format!("{}.json", self.operation.operation_id));
        predecessor_roster::retain_bounded(
            &mut self.receipt.leftovers,
            Leftover {
                item: item.into(),
                reason: reason.into(),
                restarts_old_service: restarts,
            },
            &context,
        );
    }

    /// Close the receipt with the failure and return it.
    fn fail(&mut self, error: anyhow::Error) -> Result<()> {
        let message = format!("{error:#}");
        let code = message
            .split(':')
            .next()
            .filter(|code| {
                !code.is_empty()
                    && code
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            })
            .unwrap_or("setup_failed")
            .to_owned();
        self.receipt.failure = Some(SetupFailure {
            code: bounded(code),
            message: bounded(message),
            at_unix_ms: (self.clock)(),
        });
        self.receipt.closed = true;
        // A receipt that cannot be written leaves the original error to report.
        if !self.receipt.steps.is_empty() || std::fs::symlink_metadata(self.state()).is_ok() {
            let _ = self.save_receipt();
        }
        Err(error)
    }

    async fn perform(&mut self, action: Action) -> Result<()> {
        if inno::resume_seal_required(self.receipt.reached(SetupPhase::PointerWritten), action) {
            inno::seal_published(self)?;
        }
        match action {
            Action::Lock => self.lock(),
            Action::RetirePredecessors => self.retire(),
            Action::Drain => self.drain().await,
            Action::VerifyPlacement => self.verify_placement(),
            Action::WriteRecord => self.write_record(),
            Action::WritePointer => self.write_pointer(),
            Action::RegisterLogin => self.register_login(),
            Action::Start => self.start(),
            Action::ApplyTrust => self.apply_trust(),
            Action::UnregisterLogin => self.unregister_login(),
            Action::RemoveTrust => self.remove_trust(),
            Action::RemovePointer => self.remove_pointer(),
            Action::RemoveRecord => self.remove_record(),
            Action::PurgeData => self.purge_data(),
            Action::Close => self.close(),
        }
    }

    /// U39/U50 location checks (placing kinds) or the installation match
    /// (removing kinds); problems are returned, not raised.
    fn location_problems(&self) -> Result<Vec<String>> {
        let locations = &self.operation.locations;
        if plan::removes(self.operation.kind) {
            return Ok(match &self.existing {
                Some(installed) if installed.record.locations != *locations => vec![format!(
                    "operation_installation_mismatch: the operation names other locations than the installation at {}",
                    installed.pointer.program
                )],
                _ => Vec::new(),
            });
        }
        let mut problems =
            plan::location_conflicts(locations, &effects::reserved_roots()?, cfg!(windows));
        let ours = self
            .existing
            .as_ref()
            .map(|installed| &installed.record.locations);
        let listed: std::collections::BTreeSet<String> =
            self.inventory.files.keys().cloned().collect();
        for (role, path) in plan::roles(locations) {
            let owner = match files::existing_ancestor(&path) {
                Some(owner) => owner,
                None => {
                    problems.push(format!(
                        "{role} location {} has no existing folder",
                        path.display()
                    ));
                    continue;
                }
            };
            let checked = if owner == path {
                files::owned_folder(&owner)
            } else {
                files::safe_parent(&owner)
            };
            if let Err(error) = checked {
                problems.push(format!("{role} location: {error:#}"));
                continue;
            }
            if owner != path {
                continue;
            }
            let adopted = self.owner.is_some()
                || ours.is_some_and(|ours| {
                    plan::roles(ours)
                        .into_iter()
                        .any(|(other, previous)| other == role && previous == path)
                })
                || (role != "program"
                    && self.retained.as_ref().is_some_and(|retained| {
                        plan::roles(&retained.installation.record.locations)
                            .into_iter()
                            .any(|(other, previous)| other == role && previous == path)
                    }));
            if adopted {
                continue;
            }
            let acceptable = match role {
                // Placed files are this payload's; nothing else may be there.
                "program"
                    if self.mode == Mode::Complete
                        || self.operation.requested_by == "airkctl-portable" =>
                {
                    let entries = files::list_files(&path)?;
                    let exact = entries
                        .iter()
                        .all(|name| listed.contains(name) || name == INVENTORY_FILE);
                    if exact {
                        if entries.iter().any(|name| name == INVENTORY_FILE) {
                            let inventory = files::read_bounded(
                                &path.join(INVENTORY_FILE),
                                airk_core::install_record::payload_inventory::MAX_INVENTORY_BYTES,
                                false,
                            )?;
                            ensure!(
                                files::digest_bytes(&inventory)
                                    == self.operation.payload_inventory_sha256,
                                "candidate_payload_unverified: existing inventory differs from the approved operation"
                            );
                        }
                        for (relative, expected) in &self.inventory.files {
                            let (digest, bytes) = files::digest_file(&path.join(relative))?;
                            ensure!(
                                digest == expected.sha256 && bytes == expected.bytes,
                                "candidate_payload_unverified: {relative} differs from the approved inventory"
                            );
                        }
                    }
                    exact
                }
                // Prepare may have created the receipt folder already.
                "state" if self.owner.is_some() => std::fs::read_dir(&path)?.all(|entry| {
                    entry.is_ok_and(|entry| entry.file_name() == std::ffi::OsStr::new("setup"))
                }),
                _ => files::absent_or_empty(&path)?,
            };
            if !acceptable {
                problems.push(format!(
                    "{role} location {} is not empty and is not this installation's; choose an empty folder",
                    path.display()
                ));
            }
        }
        Ok(problems)
    }

    fn lock(&mut self) -> Result<()> {
        self.compatibility_preflight()?;
        let problems = self.location_problems()?;
        ensure!(
            problems.is_empty(),
            "location_refused: {}",
            problems.join("; ")
        );
        if self.mode == Mode::Prepare
            && !plan::removes(self.operation.kind)
            && let Some(blocker) = effects::old_extension_blocker(VERSION)
        {
            bail!("{blocker}");
        }
        // The I4 readers require the program folder itself to be private;
        // Inno would create it with the parent's inherited ACL, so setup
        // creates it owner-only before placement (Windows).
        #[cfg(windows)]
        if self.mode == Mode::Prepare && !plan::removes(self.operation.kind) {
            let program = self.program();
            if !program.try_exists()? {
                airk_ipc::endpoint::prepare_root(&program).with_context(|| {
                    format!(
                        "location_refused: AIRK could not create its program folder {}",
                        program.display()
                    )
                })?;
            }
        }
        if !self.receipt.reached(SetupPhase::Locked) {
            self.record(SetupPhase::Locked, "locked", format!("{:?}", self.mode))?;
        }
        Ok(())
    }

    fn compatibility_preflight(&self) -> Result<()> {
        if plan::removes(self.operation.kind) {
            return Ok(());
        }
        ensure!(
            self.inventory.version == VERSION,
            "candidate_build_mismatch: inventory {} differs from setup {VERSION}",
            self.inventory.version
        );
        let original = self
            .existing
            .as_ref()
            .map(|installed| &installed.record)
            .or_else(|| {
                self.retained
                    .as_ref()
                    .map(|retained| &retained.installation.record)
            });
        if let Some(original) = original {
            plan::preserve_state(&original.version, &self.inventory.version, &original.contracts.state, STATE_CONTRACT,
                self.operation.acknowledgements.index_reset, plan::purges(self.operation.kind, &self.operation.options))
                .map_err(|code| anyhow::anyhow!("{code}: downgrade or incompatible state requires explicit index reset and purge"))?;
        }
        if plan::purges(self.operation.kind, &self.operation.options) {
            ensure!(
                self.operation.acknowledgements.index_reset,
                "state_reset_required: purge requires explicit reset acknowledgement"
            );
        }
        Ok(())
    }

    fn retire(&mut self) -> Result<()> {
        let candidate = self.operation.payload_inventory_sha256.to_string();
        let operation = effects::retirement_operation(self.operation.operation_id);
        let found = effects::predecessors(&candidate, &operation)?;
        let required = found
            .iter()
            .filter(|found| !matches!(found.plan, Ok((false, _))))
            .count();
        plan::acknowledged(&self.operation, required).map_err(|code| {
            anyhow::anyhow!(
                "{code}: {required} earlier AIRK installation(s) will be stopped and removed with their indexes; acknowledge predecessor removal and the index reset"
            )
        })?;
        let (mut removed, mut left) = (0, 0);
        for predecessor in found {
            let install = predecessor.install.display().to_string();
            if matches!(predecessor.plan, Ok((false, _))) {
                continue;
            }
            match effects::retire(&predecessor.install, &candidate, &operation) {
                Ok(_) => {
                    removed += 1;
                    if self.receipt.retired.len() < airk_core::install_record::MAX_SETUP_LEFTOVERS {
                        self.receipt.retired.push(bounded(install));
                    }
                }
                Err(error) => {
                    left += 1;
                    let reason = format!("{error:#}");
                    let restarts = reason.contains("login startup");
                    self.leave(install, reason, restarts);
                }
            }
        }
        for item in effects::unretired_items()? {
            left += 1;
            self.leave(item.item, item.reason, item.restarts_old_service);
        }
        // Report a leftover that can restart an old service first (§9.3).
        self.receipt
            .leftovers
            .sort_by_key(|leftover| !leftover.restarts_old_service);
        let outcome = if removed + left == 0 {
            "none_found"
        } else if left > 0 {
            "leftovers_preserved"
        } else {
            "retired"
        };
        self.record(
            SetupPhase::PredecessorsRetired,
            outcome,
            format!("{removed} owned roots retired; {left} leftover observations made (not an exact artifact count); inspect the bounded list and any uncertainty/omission markers"),
        )
    }

    async fn drain(&mut self) -> Result<()> {
        let mut controls = vec![self.state()];
        let mut images = vec![self.inventory.files[&self.inventory.executables.airk].sha256];
        let mut paths = vec![
            plan::program_file(&self.program(), &self.inventory.executables.airk),
            plan::program_file(&self.program(), &self.inventory.executables.airkctl),
        ];
        if let Some(installed) = &self.existing {
            let record = &installed.record;
            controls.push(PathBuf::from(&record.locations.state));
            images.push(record.executables.airk.sha256);
            let program = PathBuf::from(&record.locations.program);
            paths.push(plan::program_file(&program, &record.executables.airk.path));
            paths.push(plan::program_file(
                &program,
                &record.executables.airkctl.path,
            ));
        }
        controls.sort();
        controls.dedup();
        paths.sort();
        paths.dedup();
        let replacing = self.mode == Mode::Prepare || plan::removes(self.operation.kind);
        if replacing && paths.iter().any(|path| path.is_file()) {
            // Before the service stops: editor windows end their sessions on
            // the installed images instead of waiting to reconnect (§9.6.2).
            let now = (self.clock)();
            let program = self
                .existing
                .as_ref()
                .map(|installed| installed.record.locations.program.clone())
                .unwrap_or_else(|| self.program().display().to_string());
            files::write_replacement_notice(&ReplacementNotice {
                schema: REPLACEMENT_NOTICE_SCHEMA.into(),
                operation_id: self.operation.operation_id,
                program,
                created_unix_ms: now,
                expires_unix_ms: now + MAX_REPLACEMENT_NOTICE_LIFETIME_MS,
            })
            .context("replacement_notice_unwritten: open editor windows could not be told to release AIRK")?;
        }
        let mut details = Vec::new();
        let mut outcome = "not_running";
        for control in &controls {
            let (observed, detail) = effects::drain_service(control, &images).await?;
            if observed == "stopped" {
                outcome = "stopped";
            }
            details.push(detail);
        }
        // Before files are overwritten or removed, every process running an
        // installed image must have exited by itself (§9.6.2).
        if self.mode == Mode::Prepare || plan::removes(self.operation.kind) {
            details.push(effects::await_sessions(&paths)?);
        }
        if self.mode != Mode::Prepare
            && !plan::removes(self.operation.kind)
            && let Some((prior, _)) = self
                .owner
                .as_ref()
                .context("setup owner missing")?
                .moved_prior()?
        {
            let program = Path::new(&prior.record.locations.program);
            details.push(effects::await_sessions(&[
                program.join(&prior.record.executables.airk.path),
                program.join(&prior.record.executables.airkctl.path),
            ])?);
        }
        if let Some((prior, _)) = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .moved_prior()?
            && prior.record.installed_by.installer != Installer::Portable
        {
            let airk = Path::new(&prior.record.locations.program)
                .join(&prior.record.executables.airk.path);
            effects::unregister_login(
                self.host,
                Path::new(&prior.record.locations.state),
                &airk,
                prior.record.install_id,
                Some(&self.operation_path),
            )
            .context(
                "old_login_startup_removal_failed: old program must not restart after relocation",
            )?;
        }
        self.record(SetupPhase::ServiceDrained, outcome, details.join("; "))
    }

    fn verify_payload(&self) -> Result<&'static str> {
        let program = self.program();
        files::no_links(&program)?;
        for (relative, listed) in &self.inventory.files {
            let path = plan::program_file(&program, relative);
            let (digest, bytes) = files::digest_file(&path)
                .with_context(|| format!("installation_incomplete: {relative}"))?;
            ensure!(
                digest == listed.sha256 && bytes == listed.bytes,
                "installation_incomplete: {relative} differs from the payload inventory"
            );
        }
        ensure!(
            self.inventory.version == VERSION,
            "pair_mismatch: the payload is version {}, this airk is {VERSION}",
            self.inventory.version
        );
        let airkctl = plan::program_file(&program, &self.inventory.executables.airkctl);
        let (inventory_sha256, bundle_sha256) = airk_https::admin_plane::embedded_asset_digests();
        ensure!(
            self.inventory.admin_assets.inventory_sha256 == inventory_sha256
                && self.inventory.admin_assets.bundle_sha256 == bundle_sha256,
            "admin_assets_mismatch: the payload inventory names other admin assets than this airk embeds"
        );
        let signing = self.inventory.signing.as_ref().context(
            "payload_unsigned: this payload carries no signing identity; install a signed build",
        )?;
        // The I4 readers' own check (contracts §5.4.3), so the record is
        // written only for a pair they will accept (plan §9.6.3).
        let airk = plan::program_file(&program, &self.inventory.executables.airk);
        let verdict = airk_process::signature::verify(
            signature_claim(signing),
            &[airkctl.as_path(), airk.as_path()],
        )
        .map_err(|refusal| anyhow::anyhow!("{}: {}", refusal.code, refusal.message))?;
        let signatures = match verdict {
            airk_process::signature::Verdict::Signed => "the pair's signatures verify against C10",
            airk_process::signature::Verdict::DigestOnly => {
                "the pair is unsigned (Linux: integrity rests on the record digest)"
            }
        };
        airk_installer::service_manager::check_executable_identity(
            &airkctl,
            &format!("airkctl {VERSION}"),
        )
        .context("pair_mismatch: the placed airkctl reports another version")?;
        Ok(signatures)
    }

    fn verify_placement(&mut self) -> Result<()> {
        let signatures = self.verify_payload()?;
        self.record(
            SetupPhase::Placed, "verified", format!(
                "{} files match the payload inventory; the pair reports version {VERSION}; {signatures}",
                self.inventory.files.len()
            ),
        )
    }

    /// The installation identity: kept for the same program folder (repair,
    /// upgrade), otherwise derived from the operation so a resume rewrites
    /// the same record.
    fn install_id(&self) -> Incarnation {
        if let Some(installed) = &self.existing
            && installed.record.locations.program == self.operation.locations.program
        {
            return installed.record.install_id;
        }
        let digest = airk_core::identity::framed_digest(
            b"airk/install-id/1",
            &[self.operation.operation_id.as_bytes()],
        );
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Incarnation::from_bytes(bytes)
    }

    fn candidate_record(&self) -> Result<InstallRecord> {
        let executable = |relative: &str| {
            let listed = &self.inventory.files[relative];
            Executable {
                path: relative.to_owned(),
                sha256: listed.sha256,
                bytes: listed.bytes,
            }
        };
        let installer = match self.operation.requested_by.as_str() {
            "inno" => Installer::Inno,
            "dmg" => Installer::Dmg,
            "app-first-launch" => Installer::AppFirstLaunch,
            "install-sh" => Installer::InstallSh,
            "airkctl-portable" => Installer::Portable,
            other => bail!("setup_operation_refused: unknown installer {other:?}"),
        };
        let record = InstallRecord {
            schema: RECORD_SCHEMA.into(),
            install_id: self.install_id(),
            version: self.inventory.version.clone(),
            channel: self.inventory.channel.clone(),
            target: self.inventory.target.clone(),
            contracts: Contracts {
                i1_protocol: airk_core::service::PROTOCOL_VERSION,
                i2_contract: I2_CONTRACT,
                i3_admin: airk_endpoint::admin_contract::I3_CONTRACT,
                i4_record: I4_RECORD,
                state: STATE_CONTRACT.into(),
            },
            executables: Executables {
                airkctl: executable(&self.inventory.executables.airkctl),
                airk: executable(&self.inventory.executables.airk),
            },
            admin_assets: self.inventory.admin_assets.clone(),
            locations: self.operation.locations.clone(),
            signing: self
                .inventory
                .signing
                .clone()
                .context("payload_unsigned: no signing identity")?,
            installed_by: InstalledBy {
                operation_id: self.operation.operation_id,
                installer,
                installed_unix_ms: (self.clock)(),
            },
        };
        Ok(record)
    }

    fn candidate_image(
        &self,
    ) -> Result<airk_core::install_record::setup_recovery::InstallationImage> {
        let record = self.candidate_record()?;
        let pointer = InstallPointer {
            schema: POINTER_SCHEMA.into(),
            install_id: record.install_id,
            program: record.locations.program.clone(),
            record: RECORD_FILE.into(),
            record_sha256: files::digest_bytes(&airk_core::install_record::canonical(&record)),
            version: record.version.clone(),
            written_unix_ms: record.installed_by.installed_unix_ms,
        };
        let image =
            airk_core::install_record::setup_recovery::InstallationImage { pointer, record };
        image
            .validate()
            .map_err(|error| anyhow::anyhow!("installation_unverified: {}", error.code()))?;
        Ok(image)
    }

    fn write_record(&mut self) -> Result<()> {
        let record = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .candidate()?
            .record
            .clone();
        let external = self
            .operation_path
            .parent()
            .context("operation folder")?
            .join(INVENTORY_FILE);
        if external.try_exists()? {
            let bytes = files::read_bounded(
                &external,
                airk_core::install_record::payload_inventory::MAX_INVENTORY_BYTES,
                false,
            )?;
            ensure!(
                files::digest_bytes(&bytes) == self.operation.payload_inventory_sha256,
                "payload_inventory_mismatch: original operation inventory changed"
            );
            let program = self
                .owner
                .as_ref()
                .context("setup owner missing")?
                .pinned_root("program")?
                .context("program root missing")?;
            airk_ipc::private_record::write_owner_at(
                &program,
                INVENTORY_FILE,
                &bytes,
                airk_core::install_record::payload_inventory::MAX_INVENTORY_BYTES,
                true,
            )?;
        }
        // The service keeps index storage under `locations.data` and its cache
        // under `locations.cache`; an index setup review needs the data folder
        // to exist. A fresh install, and a reinstall after "delete data", have
        // neither. They are created owner-only, as the state folder is.
        files::prepare_locations(&self.operation.locations)?;
        let program = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .pinned_root("program")?
            .context("program root missing")?;
        files::write_record(&program, &record)?;
        self.record(
            SetupPhase::RecordWritten,
            "written",
            format!(
                "install {} in {}",
                record.install_id, self.operation.locations.program
            ),
        )
    }

    fn write_pointer(&mut self) -> Result<()> {
        let pointer = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .candidate()?
            .pointer
            .clone();
        let bytes = files::record_bytes(&self.program())?;
        InstallRecord::decode(&bytes, &pointer)
            .map_err(|error| anyhow::anyhow!("installation_unverified: {}", error.code()))?;
        let authority = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .authority();
        files::write_pointer(authority, &pointer)?;
        self.owner
            .as_mut()
            .context("setup owner missing")?
            .advance(airk_core::install_record::setup_recovery::active::Stage::PairPublished)?;
        inno::seal_published(self)?;
        self.cleanup_moved_program()?;
        self.existing = files::read_installation()?;
        self.record(
            SetupPhase::PointerWritten,
            "written",
            files::pointer_path()?.display().to_string(),
        )
    }

    fn cleanup_moved_program(&mut self) -> Result<()> {
        let Some((prior, root)) = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .moved_prior()?
        else {
            return Ok(());
        };
        let program = PathBuf::from(&prior.record.locations.program);
        let Some(expected) = transaction::approved_inventory(&files::pointer_path()?, &prior)?
        else {
            self.leave(program.display().to_string(),"old payload retained: no settled original inventory proof; remove only through its original installer/recovery",false);
            return Ok(());
        };
        let inventory = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .prior_inventory(&root, expected)?;
        let old_cli = plan::program_file(&program, &prior.record.executables.airkctl.path);
        if prior.record.installed_by.installer != Installer::Portable {
            let _ = self.remove_link(&program, &old_cli);
        }
        let listed = inventory.files.keys().cloned().collect();
        for relative in plan::program_removal(&listed, &inventory.executables.airk) {
            let file = inventory.files.get(&relative);
            match effects::program_files::remove(
                &root,
                &relative,
                file.map_or(expected, |file| file.sha256),
                file.map(|file| file.bytes),
            ) {
                Ok(
                    effects::program_files::Removal::Removed
                    | effects::program_files::Removal::Absent,
                ) => {}
                Ok(effects::program_files::Removal::Changed) => self.leave(
                    program.join(&relative).display().to_string(),
                    "old payload changed; preserved",
                    false,
                ),
                Err(error) => self.leave(
                    program.join(&relative).display().to_string(),
                    format!("old payload preserved: {error:#}"),
                    false,
                ),
            }
        }
        self.leave(program.display().to_string(),"old exact I4 record and directory retained for publication recovery; listed matching payload files retired, other files preserved",false);
        Ok(())
    }

    /// This operation's installation as the pointer now names it.
    fn installed(&self) -> Result<&Installed> {
        self.existing
            .as_ref()
            .filter(|installed| installed.record.locations == self.operation.locations)
            .context("installation_incomplete: the install pointer does not name this installation")
    }

    fn installed_airk(&self) -> Result<(PathBuf, Incarnation)> {
        let installed = self.installed()?;
        Ok((
            plan::program_file(&self.program(), &installed.record.executables.airk.path),
            installed.record.install_id,
        ))
    }

    fn register_login(&mut self) -> Result<()> {
        let (airk, install_id) = self.installed_airk()?;
        let (phase, outcome, detail) = effects::register_login(
            self.host,
            &self.state(),
            &airk,
            install_id,
            Some(&self.operation_path),
        );
        if phase == SetupPhase::LoginFailed {
            self.readiness = Some(("login_startup_failed".into(), detail.clone()));
        }
        self.record(phase, outcome, detail)
    }

    fn start(&mut self) -> Result<()> {
        let installed = self.installed()?;
        let airkctl =
            plan::program_file(&self.program(), &installed.record.executables.airkctl.path);
        match effects::start(&airkctl) {
            Ok(health) => {
                self.record(SetupPhase::Started, "started", "airkctl service start")?;
                self.record(SetupPhase::Healthy, "healthy", health)
            }
            Err(error) => {
                let message = format!("{error:#}");
                self.readiness = Some(("service_not_healthy".into(), message.clone()));
                self.record(SetupPhase::Started, "failed", message)
            }
        }
    }

    fn apply_trust(&mut self) -> Result<()> {
        if self.operation.options.trust == TrustChoice::Skip {
            return self.record(SetupPhase::TrustDeclined, "skipped", "trust: skip");
        }
        if self.host == Host::Linux {
            return self.record(
                SetupPhase::TrustUnsupported,
                "unsupported",
                "per-user certificate trust is not automatic on Linux",
            );
        }
        match effects::apply_trust(&self.state()) {
            Ok(Some(detail)) => self.record(SetupPhase::TrustApplied, "applied", detail),
            Ok(None) => self.record(
                SetupPhase::TrustDeclined,
                "deferred",
                "the HTTPS issuer exists after the service's first start; run `airkctl service trust install` then",
            ),
            Err(error) => self.record(SetupPhase::TrustDeclined, "failed", format!("{error:#}")),
        }
    }

    /// The installation identity a removal acts on (`None`: nothing installed).
    fn removal_target(&self) -> Option<(PathBuf, Incarnation)> {
        self.existing.as_ref().map(|installed| {
            (
                plan::program_file(&self.program(), &installed.record.executables.airk.path),
                installed.record.install_id,
            )
        })
    }

    fn unregister_login(&mut self) -> Result<()> {
        if self
            .existing
            .as_ref()
            .is_some_and(|installed| installed.record.installed_by.installer == Installer::Portable)
        {
            return self.record(
                SetupPhase::LoginUnregistered,
                "absent",
                "portable installation owns no login integration",
            );
        }
        let Some((airk, install_id)) = self.removal_target() else {
            return self.record(SetupPhase::LoginUnregistered, "absent", "no installation");
        };
        match effects::unregister_login(
            self.host,
            &self.state(),
            &airk,
            install_id,
            Some(&self.operation_path),
        ) {
            Ok(outcome) => self.record(SetupPhase::LoginUnregistered, outcome, ""),
            Err(error) => {
                let reason = format!("{error:#}");
                self.leave("login startup registration", reason.clone(), true);
                Err(anyhow::anyhow!("login_startup_removal_failed: {reason}"))
            }
        }
    }

    fn remove_trust(&mut self) -> Result<()> {
        if self
            .existing
            .as_ref()
            .is_some_and(|installed| installed.record.installed_by.installer == Installer::Portable)
        {
            return self.record(
                SetupPhase::TrustRemoved,
                "absent",
                "portable installation owns no OS trust integration",
            );
        }
        if self.existing.is_none() {
            return self.record(SetupPhase::TrustRemoved, "absent", "no installation");
        }
        match effects::remove_trust(&self.state()) {
            Ok(detail) => self.record(SetupPhase::TrustRemoved, "removed", detail),
            Err(error) => {
                let reason = format!("{error:#}");
                self.leave("user certificate trust", reason.clone(), false);
                self.record(SetupPhase::TrustRemoved, "left", reason)
            }
        }
    }

    fn remove_pointer(&mut self) -> Result<()> {
        self.owner
            .as_mut()
            .context("setup owner missing")?
            .retain_before_removal()?;
        let outcome = match &self.existing {
            Some(installed) if installed.record.locations == self.operation.locations => {
                if files::remove_pointer(
                    self.owner
                        .as_ref()
                        .context("setup owner missing")?
                        .authority(),
                    &self
                        .existing
                        .as_ref()
                        .context("original installation missing")?
                        .pointer,
                )? {
                    "removed"
                } else {
                    "absent"
                }
            }
            Some(_) => "names_other_installation",
            None => "absent",
        };
        self.record(SetupPhase::PointerRemoved, outcome, "")
    }

    /// The record, then (macOS, Linux) the placed files and the owned
    /// `airkctl` link: there `airkctl service uninstall` is the uninstaller.
    fn remove_record(&mut self) -> Result<()> {
        let program = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .pinned_root("program")?;
        let removed = if let Some(program) = program {
            files::remove_record(
                &program,
                &self
                    .existing
                    .as_ref()
                    .context("original installation missing")?
                    .record,
            )?
        } else {
            false
        };
        let outcome = if removed { "removed" } else { "absent" };
        self.owner
            .as_mut()
            .context("setup owner missing")?
            .advance(airk_core::install_record::setup_recovery::active::Stage::RemovalApplied)?;
        let detail = if self.existing.as_ref().is_some_and(|installed| {
            plan::removes_owned_program(self.host, installed.record.installed_by.installer)
        }) {
            self.remove_program()?
        } else {
            String::new()
        };
        self.record(SetupPhase::RecordRemoved, outcome, detail)
    }

    /// Delete exactly the files the payload inventory lists while they still
    /// match it (receipts only, §9.3), the inventory and the owned link, then
    /// the folders they leave empty. Anything else is left and reported.
    fn remove_program(&mut self) -> Result<String> {
        let program = self.program();
        if std::fs::symlink_metadata(&program).is_err() {
            return Ok("the program folder is absent".into());
        }
        files::no_links(&program)?;
        let program_pin = self
            .owner
            .as_ref()
            .context("setup owner missing")?
            .pinned_root("program")?
            .context("original program root not pinned")?;
        let airkctl = plan::program_file(&program, &self.inventory.executables.airkctl);
        let link =
            if self.existing.as_ref().is_some_and(|installed| {
                installed.record.installed_by.installer == Installer::Portable
            }) {
                "portable installation owns no PATH link".into()
            } else {
                self.remove_link(&program, &airkctl)
            };
        let installed = self
            .existing
            .as_ref()
            .context("original installation missing")?;
        let approved = transaction::approved_inventory(&files::pointer_path()?, installed)?;
        let full_proof = approved == Some(self.operation.payload_inventory_sha256);
        ensure!(
            approved.is_none() || full_proof,
            "original removal inventory proof changed"
        );
        let entries = plan::cleanup_candidates(
            &self.inventory,
            &installed.record,
            self.operation.payload_inventory_sha256,
            full_proof,
        );
        if !full_proof {
            self.leave(program.display().to_string(),"unproven legacy inventory: only exact I4 executables retired; support files/inventory preserved for original-installer recovery",false);
        }
        let order = entries
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect::<Vec<_>>();
        let mut removed = 0;
        for (relative, digest, bytes) in entries {
            let path = plan::program_file(&program, &relative);
            match effects::program_files::remove(&program_pin, &relative, digest, bytes) {
                Ok(effects::program_files::Removal::Removed) => removed += 1,
                Ok(effects::program_files::Removal::Absent) => {}
                Ok(effects::program_files::Removal::Changed) => self.leave(
                    path.display().to_string(),
                    "it differs from the payload inventory, so it was not removed",
                    false,
                ),
                Err(error) => self.leave(
                    path.display().to_string(),
                    format!("it could not be removed: {error}"),
                    false,
                ),
            }
        }
        #[cfg(windows)]
        for folder in plan::program_folders(&order) {
            let _ = effects::program_files::empty_folder(&program_pin, &folder);
        }
        drop(program_pin);
        if let Err(error) = self
            .owner
            .as_mut()
            .context("setup owner missing")?
            .remove_program_root(&program)
        {
            self.leave(
                program.display().to_string(),
                format!("the program folder was preserved: {error:#}"),
                false,
            );
        }
        Ok(format!("{removed} program files removed; {link}"))
    }

    /// Remove the `airkctl` link named by the installer's link receipt when
    /// it still points at this installation's `airkctl`, then the receipt.
    fn remove_link(&mut self, program: &Path, airkctl: &Path) -> String {
        let receipt = program.join(plan::LINK_RECEIPT);
        if std::fs::symlink_metadata(&receipt).is_err() {
            return "no airkctl link was owned".into();
        }
        let link = files::read_bounded(&receipt, 4096, false)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|text| plan::owned_link(&text, airkctl));
        let Some(Ok(link)) = link else {
            self.leave(
                receipt.display().to_string(),
                "the airkctl link receipt does not name this installation's airkctl, so it and its link were left",
                false,
            );
            return "the airkctl link receipt was left".into();
        };
        let observed = match std::fs::symlink_metadata(&link) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                plan::LinkObservation::Absent
            }
            Ok(metadata) if metadata.file_type().is_symlink() => std::fs::read_link(&link)
                .map(plan::LinkObservation::Link)
                .unwrap_or(plan::LinkObservation::Other),
            _ => plan::LinkObservation::Other,
        };
        let outcome = match plan::link_action(&observed, airkctl) {
            plan::LinkAction::Remove => match std::fs::remove_file(&link) {
                Ok(()) => "the airkctl link was removed",
                Err(error) => {
                    self.leave(
                        link.display().to_string(),
                        format!("the airkctl link could not be removed: {error}"),
                        false,
                    );
                    return "the airkctl link was left".into();
                }
            },
            plan::LinkAction::AlreadyGone => "the airkctl link was already absent",
            plan::LinkAction::Leave(reason) => {
                self.leave(link.display().to_string(), reason, false);
                "the airkctl link was left"
            }
        };
        let _ = std::fs::remove_file(&receipt);
        outcome.into()
    }

    fn purge_data(&mut self) -> Result<()> {
        let locations = self.operation.locations.clone();
        let protected = vec![
            self.program(),
            files::pointer_path()?
                .parent()
                .context("install pointer folder")?
                .to_path_buf(),
        ];
        let mut removed = 0;
        for (role, location) in [
            ("data", &locations.data),
            ("cache", &locations.cache),
            ("logs", &locations.logs),
            ("state", &locations.state),
        ] {
            if let Some(held) = self
                .owner
                .as_ref()
                .context("setup owner missing")?
                .pinned_root(role)?
            {
                let keep = if role == "state" {
                    &["setup"][..]
                } else {
                    &[][..]
                };
                removed += effects::purge_held(&held, Path::new(location), &protected, keep)?;
            }
        }
        self.owner
            .as_mut()
            .context("setup owner missing")?
            .revalidate()?;
        self.record(
            SetupPhase::DataPurged,
            "purged",
            format!("{removed} entries removed from the data, state, cache and log locations"),
        )
    }

    fn close(&mut self) -> Result<()> {
        let custody = airk_process::directory::OpenedDirectory::open(
            self.operation_path
                .parent()
                .context("operation parent missing")?,
        )?;
        let original = canonical(&self.operation);
        ensure!(
            files::digest_bytes(&original) == self.receipt.operation_sha256,
            "original operation bytes changed before custody retention"
        );
        if let Some(saved) = airk_ipc::private_record::read_private_at(
            &custody,
            "operation-request.json",
            airk_core::install_record::MAX_SETUP_OPERATION_BYTES,
        )? {
            ensure!(
                saved == original,
                "original recovery backup conflicts with admitted request"
            );
        } else {
            airk_ipc::private_record::write_private_at(
                &custody,
                "operation-request.json",
                &original,
                airk_core::install_record::MAX_SETUP_OPERATION_BYTES,
                false,
            )?;
        }
        if let Some((code, message)) = &self.readiness {
            self.receipt.failure = Some(SetupFailure {
                code: code.clone(),
                message: bounded(message.clone()),
                at_unix_ms: (self.clock)(),
            });
        }
        self.receipt.closed = true;
        self.record(SetupPhase::Complete, "closed", "")?;
        if self.receipt.failure.is_some() {
            return Ok(());
        }
        self.owner
            .as_mut()
            .context("setup owner missing")?
            .settle(&self.receipt)?;
        // What setup could not remove stays installed and may still run; the
        // platform installer shows `leftovers.txt` (best effort, not removed
        // with the operation files below).
        if let (Some(notice), Some(folder)) = (
            leftover_notice(&self.receipt.leftovers),
            self.operation_path.parent(),
        ) {
            let _ = files::write_owner_file(folder, "leftovers.txt", notice.as_bytes());
        }
        // Consume the ordinary admission filename. Retain the exact backup and
        // raw inventory for outer cleanup reconciliation under the same key.
        if self.operation_path.file_name() != Some(std::ffi::OsStr::new("operation-request.json")) {
            let _ = std::fs::remove_file(&self.operation_path);
        }
        if let Some(folder) = self.operation_path.parent() {
            let _ = std::fs::remove_file(folder.join("plan.json"));
            let _ = std::fs::remove_file(folder.join("plan.txt"));
            let _ = std::fs::remove_file(folder.join("error.txt"));
            let _ = std::fs::remove_dir(folder);
        }
        Ok(())
    }

    /// `--plan`: the disclosure an installer shows before any change (U13).
    fn plan_report(&self) -> Result<()> {
        let candidate = self.operation.payload_inventory_sha256.to_string();
        let operation = effects::retirement_operation(self.operation.operation_id);
        let predecessors = effects::predecessors(&candidate, &operation)?;
        let mut unretired = effects::unretired_items()?;
        let problems = self.location_problems()?;
        let blockers: Vec<String> = if plan::removes(self.operation.kind) {
            Vec::new()
        } else {
            effects::old_extension_blocker(VERSION)
                .into_iter()
                .collect()
        };
        let mut text = String::new();
        let found: Vec<Value> = predecessors
            .iter()
            .map(|found| match &found.plan {
                Ok((required, plan)) => json!({
                    "install": found.install, "removed": required, "plan": plan,
                }),
                Err(error) => json!({"install": found.install, "removed": false, "refused": error}),
            })
            .collect();
        let removed: Vec<&effects::Predecessor> = predecessors
            .iter()
            .filter(|found| !matches!(found.plan, Ok((false, _))))
            .collect();
        if removed.is_empty() {
            text.push_str("No earlier AIRK installation was found.\r\n");
        } else {
            text.push_str("Setup will stop and remove these earlier AIRK installations, including their indexes. Indexes must be rebuilt afterwards; no source file is read, changed or deleted; coding agents must be reconnected:\r\n");
            for found in &removed {
                text.push_str(&format!("  - {}", found.install.display()));
                if let Err(error) = &found.plan {
                    text.push_str(&format!(" (cannot be removed automatically: {error})"));
                }
                text.push_str("\r\n");
            }
        }
        for item in predecessors
            .iter()
            .filter_map(|found| found.plan.as_ref().err().map(|error| (found, error)))
        {
            unretired.push(Leftover {
                item: item.0.install.display().to_string(),
                reason: item.1.clone(),
                restarts_old_service: item.1.contains("login startup"),
            });
        }
        if !unretired.is_empty() {
            text.push_str("These items will be left and reported:\r\n");
            for item in &unretired {
                text.push_str(&format!("  - {}: {}\r\n", item.item, item.reason));
            }
        }
        if !problems.is_empty() {
            text.push_str("The chosen locations cannot be used:\r\n");
            for problem in &problems {
                text.push_str(&format!("  - {problem}\r\n"));
            }
        }
        if !blockers.is_empty() {
            text.push_str("Setup cannot continue yet:\r\n");
            for blocker in &blockers {
                text.push_str(&format!("  - {blocker}\r\n"));
            }
        }
        let report = json!({
            "schema": "airk-setup-plan/1",
            "operation_id": self.operation.operation_id,
            "kind": self.operation.kind,
            "predecessors": found,
            "leftovers": unretired,
            "location_problems": problems,
            "blockers": blockers,
            "existing_installation": self.existing.as_ref().map(|installed| json!({
                "install_id": installed.record.install_id,
                "version": installed.record.version,
                "program": installed.record.locations.program,
            })),
        });
        let folder = self.operation_path.parent().context("operation folder")?;
        files::write_owner_file(folder, "plan.json", format!("{report}\n").as_bytes())?;
        files::write_owner_file(folder, "plan.txt", text.as_bytes())?;
        println!("{report}");
        Ok(())
    }
}

/// The locations of this user's installation as its pointer and record name
/// them (`None` without a pointer); used by `service_locations` (U89).
pub fn installed_locations() -> Result<Option<airk_core::install_record::Locations>> {
    Ok(files::read_installation()?.map(|installed| installed.record.locations))
}

/// Shared by every foreground/detached/login/OS-manager service entry before
/// opening control state. The exact published setup candidate may start under
/// its completing owner's lock; normal starts hold exclusion until ready.
pub(crate) fn start_gate(control: &Path) -> Result<Option<airk_ipc::private_record::PrivateLock>> {
    let pointer = files::pointer_path()?;
    match transaction::owns_control(&pointer, control)? {
        Some(false) => return Ok(None),
        None => {
            let installed = files::read_installation()?;
            if installed.as_ref().is_none_or(|installed| {
                canonical_or_same(Path::new(&installed.record.locations.state))
                    != canonical_or_same(control)
            }) {
                return Ok(None);
            }
        }
        Some(true) => {}
    }
    match files::lock() {
        Ok(lock) => {
            transaction::startup_allowed(&pointer, control)?;
            if let Some(installed) = files::read_installation()? {
                inno::require_start_custody(
                    &airk_core::install_record::setup_recovery::InstallationImage {
                        pointer: installed.pointer,
                        record: installed.record,
                    },
                )?;
            }
            Ok(Some(lock))
        }
        Err(error) => {
            if transaction::startup_allowed(&pointer, control)? {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

/// U101: the login registration of this user's installation, as the service
/// manages it for the admin Service page and `airkctl service login`. The
/// identity (`ServiceManager::for_installation`) and the register/unregister
/// effects are the ones the setup transaction uses; changes hold the setup
/// lock, so they never interleave with a setup or removal.
pub(crate) struct LoginTarget {
    state: PathBuf,
    airk: PathBuf,
    install_id: Incarnation,
}

impl LoginTarget {
    /// What the registration starts (`service-login` role).
    pub(crate) fn command(&self) -> String {
        if cfg!(target_os = "macos") {
            return format!(
                "\"{}\" __airk_role service-login-current",
                self.airk.display()
            );
        }
        format!(
            "\"{}\" __airk_role service-login --install-id {}",
            self.airk.display(),
            self.install_id
        )
    }

    fn manager(&self) -> Result<airk_installer::service_manager::ServiceManager> {
        airk_installer::service_manager::ServiceManager::for_installation(
            &self.state,
            &self.airk,
            &self.install_id.to_string(),
        )
    }
}

/// The installation whose `locations.state` is `control` (the serving
/// service's control root), or `None` (no installation, or another one).
pub(crate) fn login_target(control: &Path) -> Result<Option<LoginTarget>> {
    let Some(installed) = files::read_installation()? else {
        return Ok(None);
    };
    let record = installed.record;
    let normal = |path: &Path| -> Result<PathBuf> {
        airk_host::user_paths::canonical_location(&std::path::absolute(path)?)
    };
    let state = PathBuf::from(&record.locations.state);
    if normal(control)? != normal(&state)? {
        return Ok(None);
    }
    Ok(Some(LoginTarget {
        airk: plan::program_file(
            Path::new(&record.locations.program),
            &record.executables.airk.path,
        ),
        install_id: record.install_id,
        state,
    }))
}

/// Whether the installation's own login registration exists (read-only).
pub(crate) fn login_registered(target: &LoginTarget) -> Result<bool> {
    if cfg!(target_os = "macos") {
        return Ok(mac_login::direct(target.install_id, "status")?
            == airk_installer::service_manager::MacLoginState::Enabled);
    }
    target.manager()?.owned_registration_exists()
}

fn require_current_login_target(target: &LoginTarget) -> Result<()> {
    verify_existing(&files::pointer_path()?)?;
    let current = login_target(&target.state)?
        .context("login_startup_identity_mismatch: installation changed")?;
    ensure!(
        current.install_id == target.install_id
            && canonical_or_same(&current.airk) == canonical_or_same(&target.airk)
            && canonical_or_same(&current.state) == canonical_or_same(&target.state),
        "login_startup_identity_mismatch: installation changed before OS action"
    );
    Ok(())
}

/// Register login startup (idempotent: `already_registered`).
pub(crate) fn login_install(target: &LoginTarget) -> Result<&'static str> {
    let _lock = files::lock()?;
    transaction::ordinary_management_allowed(&files::pointer_path()?)?;
    require_current_login_target(target)?;
    let (phase, outcome, detail) = effects::register_login(
        Host::current(),
        &target.state,
        &target.airk,
        target.install_id,
        None,
    );
    ensure!(
        phase == SetupPhase::LoginRegistered,
        "login_startup_failed: {detail}"
    );
    Ok(outcome)
}

/// Remove the login registration (idempotent: `absent`). Removing it stops
/// the instance the registration started (`ServiceManager::stop`), which is
/// the serving service itself, so that case is refused: the reply could
/// never be delivered.
pub(crate) fn login_uninstall(target: &LoginTarget) -> Result<&'static str> {
    let _lock = files::lock()?;
    transaction::ordinary_management_allowed(&files::pointer_path()?)?;
    require_current_login_target(target)?;
    if cfg!(target_os = "macos") {
        ensure!(
            !login_registered(target)?,
            "login_startup_in_use: stop the registered service before removing login startup"
        );
        return effects::unregister_login(
            Host::current(),
            &target.state,
            &target.airk,
            target.install_id,
            None,
        );
    }
    let manager = target.manager()?;
    if manager.owned_registration_exists()? {
        ensure!(
            login_uninstall_allowed(manager.status()?.running),
            "login_startup_in_use: removing login startup ends the instance it started, which is this service"
        );
    }
    effects::unregister_login(
        Host::current(),
        &target.state,
        &target.airk,
        target.install_id,
        None,
    )
}

/// Pure: the registration's own instance must not be running (it is the
/// serving service); an unknown state is left to `ServiceManager::stop`.
pub(crate) fn login_uninstall_allowed(running: Option<bool>) -> bool {
    running != Some(true)
}

/// The `service-login` role (§5.7): the pointer's installation must be
/// `install_id`, this image must be its record's `airk` (path and digest),
/// and the login receipt must name this registration. Then the service runs
/// as `service-detached` does, in automatic mode (a saved Stop is honored).
pub async fn serve_login(install_id: Incarnation) -> Result<()> {
    verify_existing(&files::pointer_path()?)?;
    let installed = files::read_installation()
        .context("role_admission_refused: the installation could not be verified")?
        .context("role_admission_refused: no AIRK Service installation")?;
    ensure!(
        installed.pointer.install_id == install_id,
        "role_admission_refused: the installation is not {install_id}"
    );
    let record = &installed.record;
    let (own_path, own_digest) = own_image()?;
    let airk = plan::program_file(
        Path::new(&record.locations.program),
        &record.executables.airk.path,
    );
    ensure!(
        canonical_or_same(&airk) == own_path && record.executables.airk.sha256 == own_digest,
        "role_admission_refused: this airk image is not the installation's airk"
    );
    let state = PathBuf::from(&record.locations.state);
    // U89: this service writes its logs to the record's `locations.logs`.
    airk_host::service_locations::adopt(airk_host::service_locations::installed(
        &record.locations,
    )?)?;
    let manager = airk_installer::service_manager::ServiceManager::for_installation(
        &state,
        &airk,
        &install_id.to_string(),
    )?;
    ensure!(
        if cfg!(target_os = "macos") {
            mac_login::direct(install_id, "status")?
                == airk_installer::service_manager::MacLoginState::Enabled
        } else {
            manager.owned_registration_exists()?
        },
        "role_admission_refused: no owned enabled login registration"
    );
    crate::service_runtime::serve_admitted(&state, airk_core::install_record::StartMode::Automatic)
        .await
}

pub async fn serve_login_current() -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "role_admission_refused: static current login is macOS-only"
    );
    let installed =
        files::read_installation()?.context("role_admission_refused: no current installation")?;
    serve_login(installed.record.install_id).await
}

pub fn control_login(install_id: Incarnation, action: &str, operation: &Path) -> Result<()> {
    mac_login::control(install_id, action, operation)
}
