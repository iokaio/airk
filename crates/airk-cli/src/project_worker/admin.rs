// SPDX-License-Identifier: Apache-2.0
//! Authoritative native-session purpose checks. There is no wire role flag,
//! second execution queue, additional kernel or unrestricted evidence bearer.
use super::{State, protocol, scopes};
use airk_core::service::{ConnectionGrant, ScopedReadRef};
use airk_kernel::service_broker::SessionPurpose;
use anyhow::{Result, ensure};

pub(super) fn redeem_scope(
    scopes: &mut scopes::Scopes,
    grant: &ConnectionGrant,
    purpose: SessionPurpose,
) -> Result<Option<airk_kernel::scoped_reads::ScopedReadAuthority>> {
    match purpose {
        SessionPurpose::Evidence => scopes.redeem(grant),
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
    if purpose == SessionPurpose::NativeAdministrativeRead {
        ensure!(
            request.native.is_some() && request.tool.is_empty() && request.arguments.is_empty(),
            "native administrative connection accepts only canonical NativeRead requests"
        );
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
    Option<airk_kernel::scoped_reads::ScopedReadAuthority>,
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
