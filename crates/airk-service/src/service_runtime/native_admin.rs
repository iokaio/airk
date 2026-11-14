// SPDX-License-Identifier: Apache-2.0
//! Same-user native Admin reaches canonical handlers without nested owner IPC.
use super::*;
use std::sync::atomic::Ordering;

fn transport(method: &ControlMethod) -> bool {
    matches!(
        method,
        ControlMethod::AdminNativeOpen { .. }
            | ControlMethod::AdminNativeCall { .. }
            | ControlMethod::AdminNativeRead { .. }
            | ControlMethod::AdminNativeCancel { .. }
            | ControlMethod::AdminNativeClose { .. }
    )
}
pub(super) fn checkpoint_required(state: &State, method: &ControlMethod) -> Result<bool> {
    if transport(method)
        || matches!(
            method,
            ControlMethod::Health
                | ControlMethod::HttpEndpoint
                | ControlMethod::Shutdown
                | ControlMethod::PrepareUpgradeCheckpoint { .. }
                | ControlMethod::ObserveUpgradeCheckpoint { .. }
                | ControlMethod::UpgradeRecoveryProbe { .. }
                | ControlMethod::ReleaseUpgradeAdmission { .. }
        )
    {
        return Ok(false);
    }
    Ok(!(matches!(method, ControlMethod::RequestEpoch)
        && state.checkpoint.recovery_probe(state.instance)?))
}
fn draining(state: &State) -> bool {
    state.draining.load(Ordering::Acquire)
        || state.checkpoint.admission_closed().unwrap_or(true)
        || state.lifecycle.lock().map_or(true, |record| {
            record.as_ref().is_some_and(|record| {
                record.phase == airk_core::service_lifecycle::LifecyclePhase::Draining
                    && record.request.expected_instance == state.instance
            })
        })
}
fn failure(code: &str) -> ControlResponse {
    ControlResponse::Failure { code: code.into() }
}

fn dispatch(state: Arc<State>) -> airk_https::admin_plane::NativeDispatch {
    Arc::new(move |method, check| {
        let state = state.clone();
        Box::pin(async move {
            if transport(&method) {
                return Ok(failure("console_private_setup_forbidden"));
            }
            // Transport admission owns no checkpoint permit. The real operation
            // is admitted exactly once at this live canonical boundary.
            let _admission = if checkpoint_required(&state, &method)? {
                match state.checkpoint.admit() {
                    Ok(guard) => Some(guard),
                    Err(_) => return Ok(failure("runtime_upgrade_in_progress")),
                }
            } else {
                None
            };
            if draining(&state) {
                return Ok(failure("service_draining"));
            }
            if let Err(code) = check() {
                return Ok(failure(code));
            }
            let budget = method.response_frame_budget();
            let (response, shutdown) =
                canonical_control(state.clone(), method, Some(check)).await?;
            // As on the pipe, accepted lifecycle effects outlive reply delivery.
            if shutdown {
                state.shutdown.notify_one();
            }
            // In-process composition must retain the exact canonical I1 method
            // ceiling that the HTTP dispatch's owner-pipe adapter applies.
            airk_ipc::frame::write_frame_limited(&mut std::io::sink(), &response, budget)?;
            Ok(response)
        })
    })
}

pub(super) async fn response(state: Arc<State>, method: ControlMethod) -> ControlResponse {
    match method {
        ControlMethod::AdminNativeOpen { deadline_ns } => {
            if draining(&state) {
                failure("service_draining")
            } else {
                state.admin.native_open(deadline_ns)
            }
        }
        ControlMethod::AdminNativeCall {
            session,
            request_id,
            operation,
            body,
            deadline_ns,
        } => {
            let live_state = state.clone();
            let live: airk_https::admin_plane::NativeServiceLive =
                Arc::new(move || !draining(&live_state));
            state
                .admin
                .native_call(
                    session,
                    request_id,
                    operation,
                    body,
                    deadline_ns,
                    dispatch(state.clone()),
                    live,
                )
                .await
        }
        ControlMethod::AdminNativeRead {
            session,
            request_id,
            offset,
        } => state.admin.native_read(session, request_id, offset),
        ControlMethod::AdminNativeCancel {
            session,
            request_id,
        } => state.admin.native_cancel(session, request_id),
        ControlMethod::AdminNativeClose { session } => state.admin.native_close(session),
        _ => failure("console_private_setup_forbidden"),
    }
}

/// Pure liveness/phase/deadline check at the actual blocking executor entry.
pub(super) fn check_effect(
    check: Option<&airk_https::admin_plane::NativeDispatchCheck>,
) -> Result<()> {
    if let Some(check) = check {
        check().map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
