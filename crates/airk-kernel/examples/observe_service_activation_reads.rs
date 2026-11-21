// SPDX-License-Identifier: Apache-2.0
//! Time read-only kernel calls used before an existing service worker acknowledges activation.
use airk_core::{identity::RevisionToken, service::ProjectRegistration};
use airk_kernel::Kernel;
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::time::Instant;

fn observed<T>(name: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    eprintln!("starting {name}");
    let start = Instant::now();
    let result = operation();
    println!(
        "{}",
        json!({"phase": name, "elapsed_ns": start.elapsed().as_nanos(), "ok": result.is_ok()})
    );
    result.with_context(|| format!("reading {name}"))
}

fn main() -> Result<()> {
    let [root, data, collection, binding, revision]: [String; 5] = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected SOURCE DATA COLLECTION BINDING REVISION"))?;
    let project = ProjectRegistration::new(root, data, collection)?;
    let expected: RevisionToken = revision.parse()?;
    ensure!(
        project.binding.to_string() == binding,
        "project binding differs"
    );
    let kernel = observed("open_existing_service_observer", || {
        Kernel::open_existing_service_observer(&project)
    })?;
    let rebuilt = observed("service_rebuild_receipt", || {
        kernel.service_rebuild_receipt()
    })?;
    let roots = observed("refresh_service_snapshot_and_configured_roots", || {
        kernel.refresh_service_snapshot()?.configured_roots()
    })?;
    let inspection = observed("inspect_service", || {
        kernel.inspect_service(project.binding)
    })?;
    ensure!(
        inspection.publication.as_ref() == Some(&expected),
        "inspection publication differs"
    );
    let status = observed("status", || kernel.status())?;
    let publication = status.publication.context("status lacks a publication")?;
    ensure!(publication.token == expected, "status publication differs");
    let recovery = observed("service_recovery_required", || {
        kernel.service_recovery_required()
    })?;
    ensure!(
        inspection.recovery_required == Some(recovery),
        "recovery observation changed"
    );
    println!(
        "{}",
        json!({"project": project.binding, "revision": expected, "files": status.files,
            "roots": roots.len(), "has_rebuild_receipt": rebuilt.is_some(),
            "recovery_required": recovery, "inspection": inspection})
    );
    Ok(())
}
