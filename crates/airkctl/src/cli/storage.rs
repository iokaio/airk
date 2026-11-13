// SPDX-License-Identifier: Apache-2.0
//! `airkctl service inventory|purge-plan|purge|login` (M22/M23 U100, M21
//! U101): the same I1 methods the admin Storage and Service pages call, each
//! answer printed complete as one compact JSON line.
//!
//! A purge applies exactly the plan its digest names (`purge-plan` prints
//! it) and needs `--confirm <path>`: the location's path as the service's
//! inventory lists it now, the same typed text the admin page checks (U98).
use super::{Context, Selection, VERSION, fail, installed, print_control, usage};
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Sha256Digest,
    service::{ControlMethod, ControlResponse},
    storage_admin::{LoginStartupAction, StorageLocationKind},
};
use std::path::{Path, PathBuf};

fn control(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
) -> Result<PathBuf, u8> {
    if let Some(control) = runtime_dir.or_else(|| selection.control.clone()) {
        return Ok(control);
    }
    installed(cx).map(|verified| PathBuf::from(&verified.record.locations.state))
}

fn send(cx: &mut Context, control: &Path, method: ControlMethod) -> Result<ControlResponse, u8> {
    crate::service::request(control, method, VERSION).map_err(|line| fail(cx, &line))
}

fn kind(cx: &mut Context, text: &str) -> Result<StorageLocationKind, u8> {
    serde_json::from_value(serde_json::Value::String(text.to_owned()))
        .map_err(|_| usage(cx, &format!("invalid --kind: {text}")))
}

/// `service inventory`.
pub(super) fn inventory(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
) -> u8 {
    let run = || -> Result<u8, u8> {
        let control = control(cx, selection, runtime_dir)?;
        let response = send(cx, &control, ControlMethod::StorageInventory {})?;
        Ok(print_control(cx, &response))
    };
    run().unwrap_or_else(|code| code)
}

/// `folders [PATH] [--hidden] [--after NAME]` (M24): the listing AIRK
/// Admin's Browse dialog shows, printed complete.
pub(super) fn folders(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
    path: Option<String>,
    show_hidden: bool,
    after: Option<String>,
) -> u8 {
    let run = || -> Result<u8, u8> {
        let control = control(cx, selection, runtime_dir)?;
        let response = send(
            cx,
            &control,
            ControlMethod::FolderBrowse {
                path,
                show_hidden,
                after,
            },
        )?;
        Ok(print_control(cx, &response))
    };
    run().unwrap_or_else(|code| code)
}

/// `service purge-plan --kind cache|logs`.
pub(super) fn purge_plan(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
    location: &str,
) -> u8 {
    let run = || -> Result<u8, u8> {
        let control = control(cx, selection, runtime_dir)?;
        let kind = kind(cx, location)?;
        let response = send(cx, &control, ControlMethod::StoragePurgePlan { kind })?;
        Ok(print_control(cx, &response))
    };
    run().unwrap_or_else(|code| code)
}

/// `service purge --kind … --plan-digest … --confirm <path>`.
pub(super) fn purge(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
    location: &str,
    plan_digest: &str,
    confirm: Option<&str>,
) -> u8 {
    let run = || -> Result<u8, u8> {
        let control = control(cx, selection, runtime_dir)?;
        let kind = kind(cx, location)?;
        let plan_digest: Sha256Digest = plan_digest
            .parse()
            .map_err(|_| usage(cx, "invalid --plan-digest"))?;
        // U98: the inventory's current path of this location.
        let ControlResponse::StorageInventory { inventory } =
            send(cx, &control, ControlMethod::StorageInventory {})?
        else {
            return Err(fail(
                cx,
                &ErrorLine::new(
                    ExitClass::Refused,
                    "storage_unavailable",
                    false,
                    "the storage inventory could not be read",
                ),
            ));
        };
        let path = inventory
            .locations
            .iter()
            .find(|entry| entry.kind == kind)
            .map(|entry| entry.path.clone())
            .unwrap_or_default();
        if confirm != Some(path.as_str()) {
            return Err(fail(
                cx,
                &ErrorLine::new(
                    ExitClass::Refused,
                    "confirmation_mismatch",
                    false,
                    &format!("type --confirm \"{path}\" to purge this location"),
                ),
            ));
        }
        let response = send(
            cx,
            &control,
            ControlMethod::StoragePurge { kind, plan_digest },
        )?;
        Ok(print_control(cx, &response))
    };
    run().unwrap_or_else(|code| code)
}

/// `service login status|install|uninstall`.
pub(super) fn login(
    cx: &mut Context,
    selection: &Selection,
    runtime_dir: Option<PathBuf>,
    action: &str,
) -> u8 {
    let run = || -> Result<u8, u8> {
        let control = control(cx, selection, runtime_dir)?;
        let action: LoginStartupAction =
            serde_json::from_value(serde_json::Value::String(action.to_owned()))
                .map_err(|_| usage(cx, "invalid login action"))?;
        let response = send(cx, &control, ControlMethod::LoginStartup { action })?;
        Ok(print_control(cx, &response))
    };
    run().unwrap_or_else(|code| code)
}
