// SPDX-License-Identifier: Apache-2.0
//! Factory-reset configuration ownership, independent of service liveness.
//!
//! Since local trust plan B no installation keeps a credential ledger (the
//! service deletes a retired one on upgrade), so this never reads one. A
//! predecessor's agent-config entries are recognised by their known shapes,
//! which name the connection id; this read identifies the predecessor's
//! endpoint and whether a newer installation could claim it.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http_cleanup::{
        HttpCleanupFailure, HttpCleanupQualification, PredecessorHttpLookup,
        PredecessorHttpSnapshot,
    },
};

pub(super) fn capture(root: &Path, predecessor: &str, control: &Path) -> PredecessorHttpSnapshot {
    let mut snapshot = PredecessorHttpSnapshot {
        schema: 1,
        purpose: "predecessor_cleanup".into(),
        predecessor: predecessor.into(),
        control_dir: control.to_string_lossy().into_owned(),
        namespace: None,
        endpoint: None,
        ca_certificate_file_sha256: None,
        complete: false,
        failure: None,
    };
    let captured = (|| -> Result<()> {
        let (main, _) = verify_runtime(root, predecessor)?;
        let namespace =
            airk_store::service::registry::ServiceRegistry::open_saved(control)?.incarnation()?;
        snapshot.namespace = Some(namespace);
        let binding = airk_endpoint::http_endpoint::installation_binding(control, namespace)?;
        let private = control.join("http-1");
        let endpoint = airk_endpoint::http_endpoint::load_descriptor(&private, &binding)?
            .context("predecessor HTTP endpoint metadata is unavailable")?;
        ensure!(
            endpoint.executable_sha256.to_string() == runtime_pair::digest_file(&main)?,
            "predecessor HTTP endpoint executable differs from approved predecessor"
        );
        let ca_hash = if let Some(bytes) =
            airk_endpoint::http_endpoint::read_private(&private, "ca.pem", 16 * 1024)?
        {
            use rustls::pki_types::{CertificateDer, pem::PemObject};
            let certs = CertificateDer::pem_slice_iter(&bytes)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ensure!(
                certs.len() == 1
                    && Sha256Digest::from_bytes(Sha256::digest(certs[0].as_ref()).into())
                        == endpoint.ca_sha256,
                "predecessor public CA differs from saved HTTP endpoint"
            );
            Some(Sha256Digest::from_bytes(Sha256::digest(&bytes).into()))
        } else {
            None
        };
        snapshot.endpoint = Some(endpoint);
        snapshot.ca_certificate_file_sha256 = ca_hash;
        snapshot.complete = true;
        snapshot.validate()
    })();
    if let Err(error) = captured {
        snapshot.complete = false;
        snapshot.failure = Some(HttpCleanupFailure {
            code: "predecessor_http_metadata_unavailable".into(),
            message: failure_message(&error),
        });
    }
    snapshot
}

fn failure_message(error: &anyhow::Error) -> String {
    let mut message = error.to_string();
    if message.len() > 1024 {
        let end = (0..=1024)
            .rev()
            .find(|end| message.is_char_boundary(*end))
            .unwrap_or(0);
        message.truncate(end);
    }
    message
}

fn current_namespace_ownership(
    root: &Path,
    snapshot: &PredecessorHttpSnapshot,
) -> (
    Option<Incarnation>,
    &'static str,
    Option<HttpCleanupFailure>,
) {
    let mut namespace = None;
    let observed = (|| -> Result<&'static str> {
        let control = airk_host::install_layout::saved_control(root)?;
        let current =
            airk_store::service::registry::ServiceRegistry::open_saved(&control)?.incarnation()?;
        namespace = Some(current);
        if control == Path::new(&snapshot.control_dir) {
            ensure!(
                Some(current) == snapshot.namespace,
                "HTTP predecessor namespace changed at the saved control path"
            );
            return Ok("same_predecessor");
        }
        ensure!(
            Some(current) != snapshot.namespace,
            "new HTTP control reused predecessor namespace"
        );
        // A newer installation in its own namespace issues no connection (local
        // trust plan B). It can still serve the predecessor's exact URL again;
        // an entry there may then be its own, so that is never cleared.
        let binding = airk_endpoint::http_endpoint::installation_binding(&control, current)?;
        let served =
            airk_endpoint::http_endpoint::load_descriptor(&control.join("http-1"), &binding)?;
        ensure!(
            served.as_ref().map(|endpoint| &endpoint.url)
                != snapshot.endpoint.as_ref().map(|endpoint| &endpoint.url),
            "the current installation serves the predecessor's HTTP endpoint URL"
        );
        Ok("not_owned")
    })();
    match observed {
        Ok(status) => (namespace, status, None),
        Err(error) => (
            namespace,
            "unavailable",
            Some(HttpCleanupFailure {
                code: "current_http_ownership_unavailable".into(),
                message: failure_message(&error),
            }),
        ),
    }
}

pub fn lookup(
    root: &Path,
    expected_active: &str,
    predecessor: &str,
    id: Incarnation,
) -> Result<serde_json::Value> {
    let _lock = existing_install_lock(root)?;
    let active = selection_record(root)?.active;
    ensure!(
        active == expected_active,
        "HTTP cleanup selected runtime changed"
    );
    let (operation_id, snapshot, source) = if let Some((operation, saved)) =
        activation::rebuild::predecessor_http_snapshot(root, predecessor)?
    {
        (Some(operation), saved, "saved_predecessor_snapshot")
    } else {
        ensure!(
            active == predecessor,
            "HTTP cleanup requires the exact selected predecessor or its retained reset snapshot"
        );
        // Validate saved state identity without restricting disposal-only
        // ownership reads to installations that still require a baseline reset.
        let _ = state_contract::required(root)?;
        let control = airk_host::install_layout::saved_control(root)?;
        (
            None,
            capture(root, predecessor, &control),
            "current_predecessor_metadata",
        )
    };
    snapshot.validate()?;
    let (current_namespace, current_ownership, current_failure) =
        current_namespace_ownership(root, &snapshot);
    let current_clear = matches!(current_ownership, "same_predecessor" | "not_owned");
    let owned = snapshot.complete && current_clear;
    let private = Path::new(&snapshot.control_dir).join("http-1");
    let reply = PredecessorHttpLookup {
        schema: 1,
        purpose: "predecessor_cleanup".into(),
        active,
        predecessor: predecessor.into(),
        operation_id,
        control_dir: snapshot.control_dir.clone(),
        namespace: snapshot.namespace,
        connection_id: id,
        status: if owned { "owned" } else { "unavailable" }.into(),
        endpoint: snapshot.endpoint.clone(),
        ca_certificate_file: owned.then(|| private.join("ca.pem").to_string_lossy().into_owned()),
        ca_certificate_file_sha256: snapshot.ca_certificate_file_sha256,
        qualification: HttpCleanupQualification {
            live_authority: false,
            source: source.into(),
            snapshot_complete: snapshot.complete,
            current_namespace,
            current_namespace_ownership: current_ownership.into(),
            current_namespace_failure: current_failure,
        },
        snapshot,
    };
    reply.validate()?;
    Ok(serde_json::to_value(reply)?)
}
