// SPDX-License-Identifier: Apache-2.0
//! Actual service selector supplies the original singleton allocation.
use super::*;
use airk_core::{
    arbiter::{FocusProfileLimits, focus::ordinary_search_allocation},
    combined_context::{PartitionAllowance, Resources},
    identity::Sha256Digest,
    index_catalog::IndexId,
};

pub(in crate::service_runtime) fn admit(
    state: &Arc<super::super::State>,
    tool: &str,
    arguments: &Map<String, Value>,
    context: ReadAttemptContext,
    selection: Sha256Digest,
    indexes: &[IndexId],
) -> Result<ReadAttemptContext> {
    admit_resolved(tool, arguments, context, selection, indexes, |index| {
        state
            .arbiter
            .as_ref()
            .map_or(Ok(None), |owner| owner.singleton_focus_limits(index))
    })
}

fn admit_resolved(
    tool: &str,
    arguments: &Map<String, Value>,
    context: ReadAttemptContext,
    selection: Sha256Digest,
    indexes: &[IndexId],
    limits_for: impl FnOnce(IndexId) -> Result<Option<FocusProfileLimits>>,
) -> Result<ReadAttemptContext> {
    if !matches!(tool, "search" | "airk_search")
        || context.context_partition().is_some()
        || uses_combined_arguments(arguments)
    {
        return Ok(context);
    }
    let [index] = indexes else {
        return Ok(context);
    };
    // The canonical parser owns supported arguments/defaults. Invalid source
    // requests remain canonical refusals, not extra native model work.
    let Ok(args) = serde_json::from_value::<airk_mcp::SearchArgs>(Value::Object(arguments.clone()))
    else {
        return Ok(context);
    };
    if airk_core::validate_budget(args.limit, args.max_bytes).is_err() {
        return Ok(context);
    }
    let Some(limits) = limits_for(*index).ok().flatten() else {
        return Ok(context);
    };
    capture(
        context,
        selection,
        *index,
        args.limit,
        airk_core::service::MAX_FRAME_BYTES,
        &limits,
    )
}
fn capture(
    context: ReadAttemptContext,
    selection: Sha256Digest,
    index: IndexId,
    limit: usize,
    frame_bytes: usize,
    limits: &FocusProfileLimits,
) -> Result<ReadAttemptContext> {
    require_live(&context)?;
    let id = match context.request_id() {
        Some(id) => id,
        None => airk_kernel::service_broker::new_instance_id()?,
    };
    let context = context.with_request_id(id)?;
    let attempt = airk_host::read_attempt::encode(&context, id)?;
    let sampled = std::time::Instant::now();
    let Some(parent_budget) = ordinary_search_allocation(
        attempt,
        attempt.clock,
        airk_process::monotonic::now_ns()?,
        limit,
        limit,
        limits,
        frame_bytes,
    )?
    else {
        return Ok(context);
    };
    let deadline = sampled
        .checked_add(std::time::Duration::from_millis(parent_budget.deadline_ms))
        .context("singleton Focus deadline overflow")?;
    // The original absolute attempt identity remains unchanged. Narrowed
    // Instant/relative transport carriers consume the same original allowance.
    let context = context
        .narrowed(deadline)
        .capture_transport_attempt(attempt)?;
    let allocation = PartitionAllowance {
        schema: airk_core::combined_context::ALLOWANCE_SCHEMA.into(),
        attempt: attempt.request_id,
        selection,
        index_id: index,
        resources: Resources::budget(&parent_budget),
        parent_budget,
        assessment_ordinals: vec![],
    };
    context.with_context_partition(allocation)
}

#[cfg(test)]
mod tests;
