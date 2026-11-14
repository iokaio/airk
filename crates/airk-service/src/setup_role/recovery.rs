// SPDX-License-Identifier: Apache-2.0
//! Native-owned scalar projection for original-operation wrapper recovery.
use super::*;
use airk_core::install_record::setup_recovery::active::Stage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryField {
    RequestedBy,
    Kind,
    OperationId,
    InventorySha256,
    Program,
    Data,
    State,
    Cache,
    Logs,
    LoginStartup,
    Trust,
    Start,
    PlacementRequired,
    CompletionRequired,
    WrapperLockName,
    WrapperLockHandle,
    WrapperLeaseHeld,
}
impl std::str::FromStr for RecoveryField {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "requested-by" => Self::RequestedBy,
            "kind" => Self::Kind,
            "operation-id" => Self::OperationId,
            "inventory-sha256" => Self::InventorySha256,
            "program" => Self::Program,
            "data" => Self::Data,
            "state" => Self::State,
            "cache" => Self::Cache,
            "logs" => Self::Logs,
            "login-startup" => Self::LoginStartup,
            "trust" => Self::Trust,
            "start" => Self::Start,
            "placement-required" => Self::PlacementRequired,
            "completion-required" => Self::CompletionRequired,
            "wrapper-lock-name" => Self::WrapperLockName,
            "wrapper-lock-handle" => Self::WrapperLockHandle,
            "wrapper-lease-held" => Self::WrapperLeaseHeld,
            _ => bail!("role_admission_refused: unknown recovery field"),
        })
    }
}
impl RecoveryField {
    pub fn name(self) -> &'static str {
        match self {
            Self::RequestedBy => "requested-by",
            Self::Kind => "kind",
            Self::OperationId => "operation-id",
            Self::InventorySha256 => "inventory-sha256",
            Self::Program => "program",
            Self::Data => "data",
            Self::State => "state",
            Self::Cache => "cache",
            Self::Logs => "logs",
            Self::LoginStartup => "login-startup",
            Self::Trust => "trust",
            Self::Start => "start",
            Self::PlacementRequired => "placement-required",
            Self::CompletionRequired => "completion-required",
            Self::WrapperLockName => "wrapper-lock-name",
            Self::WrapperLockHandle => "wrapper-lock-handle",
            Self::WrapperLeaseHeld => "wrapper-lease-held",
        }
    }
}
fn scalar(value: impl serde::Serialize) -> Result<String> {
    Ok(match serde_json::to_value(value)? {
        Value::String(text) => text,
        Value::Bool(value) => value.to_string(),
        _ => bail!("recovery scalar type unsupported"),
    })
}

pub(super) fn review(request: &SetupRequest, clock: Clock) -> Result<Setup> {
    let raw = files::read_operation(&request.operation)?;
    let digest = files::digest_bytes(&raw);
    let preview: SetupOperation = serde_json::from_slice(&raw)?;
    let pointer = files::pointer_path()?;
    let mut owner = transaction::Owner::observe(&pointer)?;
    if let Some(current) = &owner
        && current.active.operation != preview
    {
        ensure!(
            current.active.stage == Stage::Settled,
            "installation_in_progress: another operation owns setup"
        );
        let receipt = files::read_receipt(
            Path::new(&current.active.operation.locations.state),
            &current.active.operation.operation_id,
        )?
        .context("settled owner receipt missing")?;
        current
            .active
            .validate_settlement(&receipt)
            .map_err(|error| anyhow::anyhow!("{}", error.code()))?;
        current.existing(&pointer)?;
        owner = None;
    }
    let at = owner
        .as_ref()
        .map_or_else(clock, |owner| owner.active.admitted_unix_ms);
    let operation = SetupOperation::decode(&raw, at)
        .map_err(|error| anyhow::anyhow!("setup_operation_refused: {}", error.code()))?;
    if let Some(owner) = &owner {
        owner
            .active
            .admit_same_operation(&operation)
            .map_err(|error| anyhow::anyhow!("{}", error.code()))?;
    }
    let receipt = files::read_receipt(
        Path::new(&operation.locations.state),
        &operation.operation_id,
    )?;
    if let Some(receipt) = &receipt {
        ensure!(
            receipt.operation_sha256 == digest && receipt.operation_id == operation.operation_id,
            "original receipt binding changed"
        );
    }
    let external = request
        .operation
        .parent()
        .context("operation parent missing")?
        .join(INVENTORY_FILE);
    let inventory_path = if external.try_exists()? {
        external
    } else {
        Path::new(&operation.locations.program).join(INVENTORY_FILE)
    };
    let inventory = files::read_inventory(&inventory_path, operation.payload_inventory_sha256)?;
    ensure!(
        inventory.version == VERSION,
        "recovery image/payload version mismatch"
    );
    let existing = match &owner {
        Some(owner) => owner.existing(&pointer)?,
        None => files::read_installation()?,
    };
    let own = own_image()?;
    authorize_image(
        &operation,
        &inventory,
        existing.as_ref(),
        if plan::removes(operation.kind) {
            SetupMode::Complete
        } else {
            SetupMode::Prepare
        },
        &own.0,
        own.1,
        owner.is_some(),
    )?;
    let receipt = receipt.unwrap_or_else(|| SetupReceipt {
        schema: SETUP_RECEIPT_SCHEMA.into(),
        operation_id: operation.operation_id,
        operation_sha256: digest,
        kind: operation.kind,
        expires_unix_ms: operation.expires_unix_ms,
        steps: vec![],
        retired: vec![],
        leftovers: vec![],
        failure: None,
        closed: false,
    });
    Ok(Setup {
        operation,
        operation_path: request.operation.clone(),
        mode: Mode::Complete,
        inventory,
        existing,
        clock,
        host: Host::current(),
        readiness: None,
        owner,
        retained: None,
        receipt,
    })
}

fn completed(setup: &Setup) -> Result<bool> {
    let Some(owner) = &setup.owner else {
        return Ok(false);
    };
    if owner.active.stage != Stage::Settled {
        return Ok(false);
    }
    owner
        .active
        .validate_settlement(&setup.receipt)
        .map_err(|error| anyhow::anyhow!("original completion unproven: {}", error.code()))?;
    owner.existing(&files::pointer_path()?)?;
    if !plan::removes(setup.operation.kind) {
        setup.verify_payload()?;
        let main =
            Path::new(&setup.operation.locations.program).join(&setup.inventory.executables.airk);
        airk_host::runtime_pair::verify_installed_pair(&main, VERSION)?;
    }
    Ok(true)
}

pub(super) fn emit(request: &SetupRequest, field: RecoveryField, clock: Clock) -> Result<()> {
    let _lock = files::lock()?;
    let setup = review(request, clock)?;
    let folder = airk_process::directory::OpenedDirectory::open(
        request
            .operation
            .parent()
            .context("operation parent missing")?,
    )?;
    let output = format!("recovery-{}.txt", field.name());
    if field == RecoveryField::WrapperLockHandle {
        ensure!(
            airk_ipc::private_record::read_private_at(&folder, &output, 4096)?.is_none(),
            "opaque handle projection requires a fresh private context"
        );
    }
    let value = match field {
        RecoveryField::RequestedBy => setup.operation.requested_by.clone(),
        RecoveryField::Kind => scalar(setup.operation.kind)?,
        RecoveryField::OperationId => setup.operation.operation_id.to_string(),
        RecoveryField::InventorySha256 => setup.operation.payload_inventory_sha256.to_string(),
        RecoveryField::Program => setup.operation.locations.program.clone(),
        RecoveryField::Data => setup.operation.locations.data.clone(),
        RecoveryField::State => setup.operation.locations.state.clone(),
        RecoveryField::Cache => setup.operation.locations.cache.clone(),
        RecoveryField::Logs => setup.operation.locations.logs.clone(),
        RecoveryField::LoginStartup => setup.operation.options.login_startup.to_string(),
        RecoveryField::Trust => scalar(setup.operation.options.trust)?,
        RecoveryField::Start => setup.operation.options.start.to_string(),
        RecoveryField::PlacementRequired => {
            if plan::removes(setup.operation.kind) {
                "false".into()
            } else if setup.receipt.reached(SetupPhase::Placed) {
                setup.verify_payload()?;
                let main = Path::new(&setup.operation.locations.program)
                    .join(&setup.inventory.executables.airk);
                airk_host::runtime_pair::verify_installed_pair(&main, VERSION)?;
                "false".into()
            } else {
                "true".into()
            }
        }
        RecoveryField::CompletionRequired => (!completed(&setup)?).to_string(),
        RecoveryField::WrapperLockName => wrapper_lease::name()?,
        RecoveryField::WrapperLockHandle => wrapper_lease::windows_handle()?.to_string(),
        RecoveryField::WrapperLeaseHeld => wrapper_lease::inherited()?.to_string(),
    };
    ensure!(
        !value.chars().any(char::is_control),
        "recovery projection refuses control characters"
    );
    airk_ipc::private_record::write_private_at(
        &folder,
        &output,
        value.as_bytes(),
        airk_core::install_record::MAX_SETUP_OPERATION_BYTES,
        field != RecoveryField::WrapperLockHandle,
    )?;
    print!("{value}");
    Ok(())
}
