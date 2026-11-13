// SPDX-License-Identifier: Apache-2.0
//! The request journal client (split step 3c.3c; contracts §2.4): keyed job
//! requests through M5–M8. A key is minted once per intent; an uncertain
//! outcome never mints another (the saved key is kept for `service requests
//! retry`), and only a definitive M5 refusal is reported as nothing done.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Incarnation,
    service::{
        ControlMethod, ControlResponse, JobKind, PendingClientRequest, ProjectBinding, RequestKey,
    },
    service_configuration::{ConfigurationDraft, ConfigurationRequest},
};
use std::{path::Path, time::Duration};

/// How long an unknown epoch outcome is retried, as the CLI did.
const EPOCH_BUDGET: Duration = Duration::from_secs(15);

/// The guidance `airk` added to a source-change refusal, naming `airkctl`.
pub fn refusal_guidance(code: &str) -> Option<&'static str> {
    (code == "source_change_review_required").then_some(
        "Review the Index's sources or retirement in Indexing, or prepare a CatalogSourceMutationDraft through 'airkctl service rpc'",
    )
}

fn refused(code: &str, message: String) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, code, false, &message)
}

/// A fresh random identity for a caller or a nonce.
pub fn mint() -> Result<Incarnation, ErrorLine> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ErrorLine::new(
            ExitClass::Internal,
            "internal",
            false,
            &format!("identity: {error}"),
        )
    })?;
    Ok(Incarnation::from_bytes(bytes))
}

/// The current `(namespace, epoch)`; only `registry_response_unknown_retry_with_key`
/// is retried, within 15 s.
pub fn epoch(control: &Path, version: &str) -> Result<(Incarnation, u64), ErrorLine> {
    let deadline = std::time::Instant::now() + EPOCH_BUDGET;
    loop {
        match crate::service::request(control, ControlMethod::RequestEpoch, version)? {
            ControlResponse::RequestEpoch { namespace, epoch } => return Ok((namespace, epoch)),
            ControlResponse::Failure { code }
                if code == "registry_response_unknown_retry_with_key"
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            other => return Err(crate::service::refusal(&other)),
        }
    }
}

/// U103: retry a failed job with a fresh request key. Retries are not saved
/// client requests (the journal holds `Enqueue` and `ConfigureProject`); a
/// lost reply is an unknown outcome, and `jobs get` reads the job.
pub fn retry(
    control: &Path,
    version: &str,
    id: airk_core::identity::Sha256Digest,
) -> Result<ControlResponse, ErrorLine> {
    let (namespace, epoch) = epoch(control, version)?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: mint()?,
    };
    crate::service::request(
        control,
        ControlMethod::Retry {
            id,
            caller: mint()?,
            key,
        },
        version,
    )
}

/// M5: journal a keyed `Enqueue` and return its nonce.
pub fn prepare(
    control: &Path,
    version: &str,
    project: ProjectBinding,
    kind: JobKind,
) -> Result<Incarnation, ErrorLine> {
    journal(control, version, |caller, key| ControlMethod::Enqueue {
        project,
        caller,
        key,
        kind,
    })
}

/// M9: the guarded configuration request for a draft.
pub fn configuration(
    control: &Path,
    version: &str,
    project: ProjectBinding,
    draft: ConfigurationDraft,
) -> Result<ConfigurationRequest, ErrorLine> {
    match crate::service::request(
        control,
        ControlMethod::PrepareConfiguration { project, draft },
        version,
    )? {
        ControlResponse::ConfigurationPrepared { request } => Ok(*request),
        response @ ControlResponse::ConfigurationRefused { .. } => {
            let ControlResponse::ConfigurationRefused { message } = &response else {
                unreachable!()
            };
            let mut line = ErrorLine::new(ExitClass::Refused, "operation_failed", false, message);
            line.native = serde_json::to_value(&response).ok();
            Err(line)
        }
        other => Err(crate::service::refusal(&other)),
    }
}

/// M9 then M5, in `airk`'s order: prepare the draft, then journal the keyed
/// `ConfigureProject`; returns the nonce.
pub fn prepare_configuration(
    control: &Path,
    version: &str,
    project: ProjectBinding,
    draft: ConfigurationDraft,
) -> Result<Incarnation, ErrorLine> {
    let request = Box::new(configuration(control, version, project, draft)?);
    journal(control, version, |caller, key| {
        ControlMethod::ConfigureProject {
            project,
            caller,
            key,
            request,
        }
    })
}

/// M5 for one keyed request: the epoch, a caller and nonce minted once.
fn journal(
    control: &Path,
    version: &str,
    request: impl FnOnce(Incarnation, RequestKey) -> ControlMethod,
) -> Result<Incarnation, ErrorLine> {
    let (namespace, epoch) = epoch(control, version)?;
    let caller = mint()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: mint()?,
    };
    let method = ControlMethod::ClientRequestPrepare {
        caller,
        key: key.clone(),
        request: Box::new(request(caller, key)),
    };
    match crate::service::request(control, method, version) {
        Ok(ControlResponse::ClientRequestPrepared { nonce, .. }) => Ok(nonce),
        Ok(ControlResponse::Failure { code }) => {
            Err(refused(&code, format!("native request refused: {code}")))
        }
        Ok(other) => Err(crate::service::refusal(&other)),
        // A lost M5 reply leaves nothing to retry here; the CLI reported it.
        Err(line) => Err(line),
    }
}

/// M7: submit the saved request by nonce. A job is the answer; a refusal or
/// an unknown outcome keeps the saved key for `service requests retry`.
pub fn submit(
    control: &Path,
    version: &str,
    nonce: Incarnation,
) -> Result<ControlResponse, ErrorLine> {
    match crate::service::request(
        control,
        ControlMethod::ClientRequestSubmit { nonce },
        version,
    ) {
        Ok(response @ ControlResponse::Job { .. }) => Ok(response),
        Ok(ControlResponse::Failure { code }) => {
            let guidance = refusal_guidance(&code)
                .map(|guidance| format!("; {guidance}"))
                .unwrap_or_default();
            Err(refused(
                &code,
                format!("native request refused: {code}{guidance}; saved key is preserved"),
            ))
        }
        Ok(_) => Err(ErrorLine::new(
            ExitClass::OutcomeUnknown,
            "service_request_outcome_unknown",
            false,
            "native request outcome unknown; saved key is preserved",
        )),
        Err(mut line) => {
            if line.class == ExitClass::Internal {
                line.class = ExitClass::OutcomeUnknown;
                line.code = "service_request_outcome_unknown".into();
                line.message = format!("{}; saved key is preserved", line.message);
            }
            Err(line)
        }
    }
}

/// M6: the journaled requests.
pub fn list(control: &Path, version: &str) -> Result<Vec<PendingClientRequest>, ErrorLine> {
    match crate::service::request(control, ControlMethod::ClientRequestList {}, version)? {
        ControlResponse::ClientRequests { pending, .. } => Ok(pending),
        other => Err(crate::service::refusal(&other)),
    }
}

/// M8: forget the saved request; never cancels native work.
pub fn forget(control: &Path, version: &str, nonce: Incarnation) -> Result<bool, ErrorLine> {
    match crate::service::request(
        control,
        ControlMethod::ClientRequestForget { nonce },
        version,
    )? {
        ControlResponse::ClientRequestForgotten { existed, .. } => Ok(existed),
        other => Err(crate::service::refusal(&other)),
    }
}
