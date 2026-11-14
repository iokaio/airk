// SPDX-License-Identifier: Apache-2.0
//! Offline receipt observation bound to the current verified installation.
use crate::{
    contract::{ErrorLine, ExitClass, SETUP_RECEIPT_OBSERVATION_SCHEMA, SetupReceiptObservation},
    install::Verified,
};
use airk_core::{
    identity::Incarnation,
    install_record::{MAX_SETUP_RECEIPT_BYTES, OperationKind, SetupPhase, SetupReceipt, canonical},
};
use std::path::PathBuf;

fn refused(message: impl std::fmt::Display) -> ErrorLine {
    ErrorLine::new(
        ExitClass::Refused,
        "setup_receipt_unverified",
        false,
        &message.to_string(),
    )
}

pub(crate) fn observe(installation: &Verified) -> Result<SetupReceiptObservation, ErrorLine> {
    let root = PathBuf::from(&installation.record.locations.state).join("setup");
    let pin = airk_process::directory::OpenedDirectory::open(&root).map_err(refused)?;
    airk_ipc::endpoint::require_private_directory_handle(pin.as_file()).map_err(refused)?;
    let operation = installation.record.installed_by.operation_id;
    let bytes = airk_ipc::private_record::read_private(
        &root,
        &format!("{operation}.json"),
        MAX_SETUP_RECEIPT_BYTES,
    )
    .map_err(refused)?
    .ok_or_else(|| refused("the installation's setup receipt is not available"))?;
    let receipt = SetupReceipt::decode(&bytes).map_err(|error| refused(error.code()))?;
    match crate::install::discover() {
        crate::install::Discovery::Verified(current)
            if current.pointer == installation.pointer && current.record == installation.record => {
        }
        _ => {
            return Err(refused(
                "the verified installation changed during receipt observation",
            ));
        }
    }
    classify(receipt, operation, installation.record.install_id)
}

fn classify(
    receipt: SetupReceipt,
    operation: Incarnation,
    installation: Incarnation,
) -> Result<SetupReceiptObservation, ErrorLine> {
    SetupReceipt::decode(&canonical(&receipt)).map_err(|error| refused(error.code()))?;
    if receipt.operation_id != operation
        || !matches!(
            receipt.kind,
            OperationKind::Install | OperationKind::Repair | OperationKind::Upgrade
        )
    {
        return Err(refused(
            "the receipt does not belong to this installation's setup",
        ));
    }
    let phase = |wanted, outcome| {
        receipt
            .steps
            .iter()
            .position(|step| step.phase == wanted && step.outcome == outcome)
    };
    let ordered_completion = match (
        phase(SetupPhase::RecordWritten, "written"),
        phase(SetupPhase::PointerWritten, "written"),
        phase(SetupPhase::Complete, "closed"),
    ) {
        (Some(record), Some(pointer), Some(complete)) => {
            record < pointer && pointer < complete && complete + 1 == receipt.steps.len()
        }
        _ => false,
    };
    Ok(SetupReceiptObservation {
        schema: SETUP_RECEIPT_OBSERVATION_SCHEMA.into(),
        operation_id: operation,
        install_id: installation,
        closed: receipt.closed,
        successful: receipt.closed && receipt.failure.is_none() && ordered_completion,
        receipt: serde_json::to_value(receipt).map_err(refused)?,
    })
}

#[cfg(test)]
mod tests;
