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
pub(crate) mod plan;
#[cfg(test)]
mod tests;

use airk_core::{
    identity::{Incarnation, Sha256Digest},
    install_record::{
        Contracts, Executable, Executables, InstallPointer, InstallRecord, InstalledBy, Installer,
        Leftover, MAX_REPLACEMENT_NOTICE_LIFETIME_MS, POINTER_SCHEMA, RECORD_FILE, RECORD_SCHEMA,
        REPLACEMENT_NOTICE_SCHEMA, ReplacementNotice, SETUP_RECEIPT_SCHEMA, SetupFailure,
        SetupOperation, SetupPhase, SetupReceipt, SetupStep, Signing, TrustChoice,
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
}

/// An admitted `setup` request (role admission checked its syntax).
#[derive(Debug, PartialEq, Eq)]
pub struct SetupRequest {
    pub operation: PathBuf,
    pub mode: SetupMode,
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
    // First admission is checked against the clock; a resume against the
    // admission time its receipt recorded (the bytes are bound by digest).
    let admitted_at = receipt
        .as_ref()
        .and_then(|receipt| receipt.steps.first())
        .map(|step| step.at_unix_ms)
        .unwrap_or_else(clock);
    let operation = SetupOperation::decode(&bytes, admitted_at)
        .map_err(|refusal| anyhow::anyhow!("setup_operation_refused: {}", refusal.code()))?;
    if let Some(receipt) = &receipt {
        plan::admit_resume(receipt, &operation, operation_sha256)
            .map_err(|code| anyhow::anyhow!("{code}: operation {}", operation.operation_id))?;
    }
    let mode = match request.mode {
        SetupMode::Plan | SetupMode::Prepare => Mode::Prepare,
        SetupMode::Complete => Mode::Complete,
    };
    if plan::removes(operation.kind) && request.mode != SetupMode::Complete {
        bail!(
            "setup_prepare_not_applicable: {:?} runs as one step",
            operation.kind
        );
    }
    let program = PathBuf::from(&operation.locations.program);
    let inventory_path = match request.mode {
        SetupMode::Complete => program.join(INVENTORY_FILE),
        SetupMode::Plan | SetupMode::Prepare => request
            .operation
            .parent()
            .context("operation folder")?
            .join(INVENTORY_FILE),
    };
    let inventory = files::read_inventory(&inventory_path, operation.payload_inventory_sha256)?;
    let existing = files::read_installation()?;
    let (own_path, own_digest) = own_image()?;
    authorize_image(
        &operation,
        &inventory,
        existing.as_ref(),
        request.mode,
        &own_path,
        own_digest,
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
    };
    if request.mode == SetupMode::Plan {
        return setup.plan_report();
    }
    let _lock = files::lock()?;
    // Re-read under the lock: another setup may have finished meanwhile.
    setup.existing = files::read_installation()?;
    let actions = plan::remaining(
        setup.operation.kind,
        &setup.operation.options,
        mode,
        &setup.recorded(),
    )
    .map_err(|code| anyhow::anyhow!("{code}"))?;
    for action in actions {
        if let Err(error) = setup.perform(action).await {
            // Editor windows reconnect to whatever the failure left in place.
            let _ = files::remove_replacement_notice();
            return setup.fail(error);
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
) -> Result<()> {
    let listed = &inventory.files[&inventory.executables.airk];
    ensure!(
        listed.sha256 == own_digest,
        "role_admission_refused: this airk image is not the payload's airk"
    );
    if mode == SetupMode::Complete {
        let program = PathBuf::from(&operation.locations.program);
        let placed = canonical_or_same(&plan::program_file(&program, &inventory.executables.airk));
        ensure!(
            placed == own_path,
            "role_admission_refused: setup must run from {}",
            placed.display()
        );
        if let Some(installed) = existing.filter(|installed| {
            plan::removes(operation.kind) && installed.record.locations == operation.locations
        }) {
            ensure!(
                installed.record.executables.airk.sha256 == own_digest,
                "role_admission_refused: this airk image is not the installed airk"
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
        files::write_receipt(&self.state(), &self.receipt)
    }

    fn leave(&mut self, item: impl Into<String>, reason: impl Into<String>, restarts: bool) {
        if self.receipt.leftovers.len() < airk_core::install_record::MAX_SETUP_LEFTOVERS {
            self.receipt.leftovers.push(Leftover {
                item: bounded(item.into()),
                reason: bounded(reason.into()),
                restarts_old_service: restarts,
            });
        }
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
            let _ = files::write_receipt(&self.state(), &self.receipt);
        }
        Err(error)
    }

    async fn perform(&mut self, action: Action) -> Result<()> {
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
            let adopted = ours.is_some_and(|ours| {
                plan::roles(ours)
                    .into_iter()
                    .any(|(other, previous)| other == role && previous == path)
            });
            if adopted {
                continue;
            }
            let acceptable = match role {
                // Placed files are this payload's; nothing else may be there.
                "program" if self.mode == Mode::Complete => {
                    let foreign =
                        plan::foreign_program_entries(&files::list_files(&path)?, &listed);
                    foreign.is_empty()
                }
                // Prepare may have created the receipt folder already.
                "state" => std::fs::read_dir(&path)?.all(|entry| {
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
        } else {
            "retired"
        };
        self.record(
            SetupPhase::PredecessorsRetired,
            outcome,
            format!("{removed} removed, {left} left"),
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
        self.record(SetupPhase::ServiceDrained, outcome, details.join("; "))
    }

    fn verify_placement(&mut self) -> Result<()> {
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
        crate::service_manager::check_executable_identity(&airkctl, &format!("airkctl {VERSION}"))
            .context("pair_mismatch: the placed airkctl reports another version")?;
        let (inventory_sha256, bundle_sha256) = crate::admin_plane::embedded_asset_digests();
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
        self.record(
            SetupPhase::Placed,
            "verified",
            format!(
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

    fn write_record(&mut self) -> Result<()> {
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
                i3_admin: crate::admin_contract::I3_CONTRACT,
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
        files::write_record(&self.program(), &record)?;
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
        let program = self.program();
        let bytes = files::record_bytes(&program)?;
        let unchecked: InstallRecord =
            serde_json::from_slice(&bytes).context("install record is unreadable")?;
        let pointer = InstallPointer {
            schema: POINTER_SCHEMA.into(),
            install_id: unchecked.install_id,
            program: self.operation.locations.program.clone(),
            record: RECORD_FILE.into(),
            record_sha256: files::digest_bytes(&bytes),
            version: unchecked.version.clone(),
            written_unix_ms: (self.clock)(),
        };
        // The pointer is written only for a record its readers will accept.
        InstallRecord::decode(&bytes, &pointer)
            .map_err(|refusal| anyhow::anyhow!("installation_unverified: {}", refusal.code()))?;
        let moved = self
            .existing
            .as_ref()
            .map(|previous| previous.record.locations.program.clone())
            .filter(|previous| *previous != self.operation.locations.program);
        if let Some(previous) = moved {
            self.leave(
                previous,
                "the earlier program folder of this AIRK Service; moving the program folder does not remove it in this build",
                false,
            );
        }
        files::write_pointer(&pointer)?;
        self.existing = files::read_installation()?;
        self.record(
            SetupPhase::PointerWritten,
            "written",
            files::pointer_path()?.display().to_string(),
        )
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
        let (phase, outcome, detail) =
            effects::register_login(self.host, &self.state(), &airk, install_id);
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
        let Some((airk, install_id)) = self.removal_target() else {
            return self.record(SetupPhase::LoginUnregistered, "absent", "no installation");
        };
        match effects::unregister_login(self.host, &self.state(), &airk, install_id) {
            Ok(outcome) => self.record(SetupPhase::LoginUnregistered, outcome, ""),
            Err(error) => {
                let reason = format!("{error:#}");
                self.leave("login startup registration", reason.clone(), false);
                self.record(SetupPhase::LoginUnregistered, "left", reason)
            }
        }
    }

    fn remove_trust(&mut self) -> Result<()> {
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
        let outcome = match &self.existing {
            Some(installed) if installed.record.locations == self.operation.locations => {
                if files::remove_pointer()? {
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
        let outcome = if files::remove_record(&self.program())? {
            "removed"
        } else {
            "absent"
        };
        let detail = if plan::removes_program_files(self.host) {
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
        let airkctl = plan::program_file(&program, &self.inventory.executables.airkctl);
        let link = self.remove_link(&program, &airkctl);
        let listed: std::collections::BTreeSet<String> =
            self.inventory.files.keys().cloned().collect();
        let order = plan::program_removal(&listed, &self.inventory.executables.airk);
        let mut removed = 0;
        for relative in &order {
            let path = plan::program_file(&program, relative);
            if std::fs::symlink_metadata(&path).is_err() {
                continue;
            }
            let observed = files::digest_file(&path).ok();
            let unchanged = match self.inventory.files.get(relative) {
                Some(listed) => observed == Some((listed.sha256, listed.bytes)),
                // The inventory itself, as the operation pinned it.
                None => observed
                    .is_some_and(|(digest, _)| digest == self.operation.payload_inventory_sha256),
            };
            if !unchanged {
                self.leave(
                    path.display().to_string(),
                    "it differs from the payload inventory, so it was not removed",
                    false,
                );
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(error) => self.leave(
                    path.display().to_string(),
                    format!("it could not be removed: {error}"),
                    false,
                ),
            }
        }
        for folder in plan::program_folders(&order) {
            let _ = std::fs::remove_dir(plan::program_file(&program, &folder));
        }
        if std::fs::remove_dir(&program).is_err() {
            self.leave(
                program.display().to_string(),
                "the program folder holds files this installation did not place; they were left",
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
        for location in [&locations.data, &locations.cache, &locations.logs] {
            removed += effects::purge(Path::new(location), &protected, &[])?;
        }
        removed += effects::purge(Path::new(&locations.state), &protected, &["setup"])?;
        self.record(
            SetupPhase::DataPurged,
            "purged",
            format!("{removed} entries removed from the data, state, cache and log locations"),
        )
    }

    fn close(&mut self) -> Result<()> {
        if let Some((code, message)) = &self.readiness {
            self.receipt.failure = Some(SetupFailure {
                code: code.clone(),
                message: bounded(message.clone()),
                at_unix_ms: (self.clock)(),
            });
        }
        self.receipt.closed = true;
        self.record(SetupPhase::Complete, "closed", "")?;
        // What setup could not remove stays installed and may still run; the
        // platform installer shows `leftovers.txt` (best effort, not removed
        // with the operation files below).
        if let (Some(notice), Some(folder)) = (
            leftover_notice(&self.receipt.leftovers),
            self.operation_path.parent(),
        ) {
            let _ = files::write_owner_file(folder, "leftovers.txt", notice.as_bytes());
        }
        // Single use: the operation file and its folder go once the receipt
        // is closed; a replay then finds the closed receipt.
        let _ = std::fs::remove_file(&self.operation_path);
        if let Some(folder) = self.operation_path.parent() {
            let _ = std::fs::remove_file(folder.join(INVENTORY_FILE));
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
pub(crate) fn installed_locations() -> Result<Option<airk_core::install_record::Locations>> {
    Ok(files::read_installation()?.map(|installed| installed.record.locations))
}

/// The `service-login` role (§5.7): the pointer's installation must be
/// `install_id`, this image must be its record's `airk` (path and digest),
/// and the login receipt must name this registration. Then the service runs
/// as `service-detached` does, in automatic mode (a saved Stop is honored).
pub async fn serve_login(install_id: Incarnation) -> Result<()> {
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
    crate::service_locations::adopt(crate::service_locations::installed(&record.locations)?)?;
    let manager = crate::service_manager::ServiceManager::for_installation(
        &state,
        &airk,
        &install_id.to_string(),
    )?;
    ensure!(
        manager.owned_registration_exists()?,
        "role_admission_refused: no login receipt names this registration"
    );
    crate::service_runtime::serve_admitted(&state, airk_core::install_record::StartMode::Automatic)
        .await
}
