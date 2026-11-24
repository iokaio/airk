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
            purpose == SessionPurpose::Evidence && request.native.is_none(),
            "context partition requires scoped canonical Answer or singleton Focus Search"
        );
        if partition.is_ordinary_focus() {
            ensure!(
                request.tool == "airk_search",
                "Focus-only partition requires scoped canonical Search"
            );
            let args: airk_mcp::SearchArgs =
                serde_json::from_value(serde_json::Value::Object(request.arguments.clone()))?;
            airk_core::validate_budget(args.limit, args.max_bytes)?;
            ensure!(
                partition.resources.candidates <= args.limit,
                "Focus-only partition exceeds original Search candidate allowance"
            );
        } else {
            ensure!(
                request.tool == "airk_answer",
                "context partition requires scoped canonical Answer"
            );
        }
        let attempt = request
            .read_attempt
            .as_ref()
            .context("context partition original attempt missing")?;
        ensure!(
            attempt.request_id == partition.attempt,
            "context partition attempt mismatch"
        );
        if partition.is_ordinary_focus() {
            // Decode before sampling the comparison clock: decoding an expired
            // zero-remaining budget must not produce a later Instant than an
            // already-sampled left operand and accidentally admit storage work.
            let original = airk_host::read_attempt::decode(*attempt)?;
            ensure!(
                std::time::Instant::now() < original.deadline(),
                "singleton Focus original read deadline expired"
            );
        } else {
            anyhow::bail!("a combined Answer member carries an Answer /3 partition");
        }
    }
    if let Some(partition) = &request.answer_partition {
        partition.validate()?;
        ensure!(
            purpose == SessionPurpose::Evidence
                && request.native.is_none()
                && request.context_partition.is_none()
                && request.tool == "airk_answer",
            "an Answer /3 partition requires scoped canonical Answer"
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
        let parent = &partition.parent_budget;
        ensure!(
            partition.clamp(&budget)? == budget
                && budget.rounds <= parent.rounds
                && budget.fanout <= parent.fanout
                && budget.max_parts <= parent.max_parts
                && budget.max_sub_needs <= parent.max_sub_needs
                && budget.deadline_ms <= parent.deadline_ms,
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
        let selection = if partition.is_ordinary_focus() {
            authority.selection.scope_identity
        } else {
            partition.selection
        };
        partition.validate_binding(attempt, selection, authority.index_id)?;
    }
    if let Some(partition) = &request.answer_partition {
        let authority =
            authority.context("context partition requires installed index authority")?;
        authority.validate()?;
        let attempt = request
            .read_attempt
            .as_ref()
            .context("context partition original attempt missing")?;
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
