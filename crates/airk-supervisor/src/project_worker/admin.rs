// SPDX-License-Identifier: Apache-2.0
//! Authoritative native-session purpose checks. There is no wire role flag,
//! second execution queue, additional kernel or unrestricted evidence bearer.
use super::{State, protocol, scopes};
use airk_core::service::{ConnectionGrant, ScopedReadRef};
use airk_kernel::service_broker::SessionPurpose;
use anyhow::{Context, Result, ensure};

pub(super) fn redeem_scope(
    scopes: &mut scopes::Scopes,
    grant: &ConnectionGrant,
    purpose: SessionPurpose,
) -> Result<Option<scopes::WorkerAuthority>> {
    match purpose {
        SessionPurpose::Evidence => scopes.redeem_authority(grant),
        SessionPurpose::NativeAdministrativeRead => {
            ensure!(
                grant.scope.is_none(),
                "native administrative connection cannot assert an evidence scope"
            );
            Ok(None)
        }
    }
}
pub(super) fn check_scope(
    scopes: &scopes::Scopes,
    scope: Option<ScopedReadRef>,
    purpose: SessionPurpose,
) -> Result<()> {
    match purpose {
        SessionPurpose::Evidence => scopes.check(scope),
        SessionPurpose::NativeAdministrativeRead => {
            ensure!(
                scope.is_none(),
                "native administrative connection cannot assert an evidence scope"
            );
            Ok(())
        }
    }
}
pub(super) fn validate_request(purpose: SessionPurpose, request: &protocol::Query) -> Result<()> {
    if let Some(partition) = &request.context_partition {
        partition.validate()?;
        ensure!(
            purpose == SessionPurpose::Evidence
                && request.native.is_none()
                && request.tool == "airk_answer",
            "context partition requires scoped canonical Answer"
        );
        let attempt = request
            .read_attempt
            .as_ref()
            .context("context partition original attempt missing")?;
        ensure!(
            attempt.request_id == partition.attempt,
            "context partition attempt mismatch"
        );
        let args: airk_mcp::AnswerArgs =
            serde_json::from_value(serde_json::Value::Object(request.arguments.clone()))?;
        args.request.validate()?;
        let budget = args.request.budget;
        ensure!(
            budget.calls <= partition.resources.calls
                && budget.candidates <= partition.resources.candidates
                && budget.max_bytes <= partition.resources.bytes
                && budget.input_tokens <= partition.resources.input_tokens.max(1)
                && budget.rounds <= partition.parent_budget.rounds
                && budget.fanout <= partition.parent_budget.fanout
                && budget.deadline_ms <= partition.parent_budget.deadline_ms,
            "context partition request exceeds reserved budget"
        );
    }
    if purpose == SessionPurpose::NativeAdministrativeRead {
        ensure!(
            request.native.is_some() && request.tool.is_empty() && request.arguments.is_empty(),
            "native administrative connection accepts only canonical NativeRead requests"
        );
    }
    Ok(())
}

pub(super) fn validate_context_partition(
    request: &protocol::Query,
    authority: Option<airk_core::service::ScopedReadRef>,
) -> Result<()> {
    if let Some(partition) = &request.context_partition {
        let authority =
            authority.context("context partition requires installed index authority")?;
        authority.validate()?;
        let attempt = request
            .read_attempt
            .as_ref()
            .context("context partition original attempt missing")?;
        // The selected-scope digest is inert frontend-owned identity. The
        // installed lease, not that digest, grants this exact local index.
        partition.validate_binding(attempt, partition.selection, authority.index_id)?;
    }
    Ok(())
}
/// Redeem the native broker role before scope validation. Any subsequent refusal
/// disconnects that exact resource session; never invert broker/scope lock order.
pub(super) fn redeem(
    state: &State,
    grant: &ConnectionGrant,
) -> Result<(
    airk_core::identity::Incarnation,
    SessionPurpose,
    Option<scopes::WorkerAuthority>,
)> {
    let (session, purpose) = state
        .broker
        .lock()
        .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
        .redeem_with_purpose(
            grant,
            state.identity,
            crate::local_ipc::caller_identity()?,
            std::time::Instant::now(),
            std::time::Duration::from_secs(60),
        )?;
    let authority = (|| {
        let mut scopes = state
            .scopes
            .lock()
            .map_err(|_| anyhow::anyhow!("scope policy unavailable"))?;
        redeem_scope(&mut scopes, grant, purpose)
    })();
    match authority {
        Ok(authority) => Ok((session, purpose, authority)),
        Err(error) => {
            state
                .broker
                .lock()
                .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                .disconnect(state.identity, session, std::time::Instant::now())?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
