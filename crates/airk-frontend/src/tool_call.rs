// SPDX-License-Identifier: Apache-2.0
//! The tool call path of the pre-split CLI (split step 3a.2b), shared by
//! `airk tools call`, the `airk` evidence reads and the service's M3
//! `ToolsCall` handler (`service_runtime::tools_call`;
//! `docs/split/contracts.md` §2.2). It depends only on `airk-mcp`, so the
//! service runs the same steps the CLI ran.
use airk_mcp::{remote::RemoteTools, response::ResponseMode};
use anyhow::{Result, ensure};
use serde_json::{Map, Value};
use std::sync::Arc;

/// Reuse HTTP's public routing and argument validation without attaching an
/// ambient Explorer window or changing the native selection authority.
pub fn project_backend(
    backend: Arc<dyn RemoteTools>,
    binding: String,
    collection: &str,
) -> Result<Arc<dyn RemoteTools>> {
    Ok(Arc::new(airk_mcp::unified::Unified::new(
        vec![airk_mcp::unified::Project {
            binding: binding.clone(),
            name: collection.into(),
            collection: collection.into(),
            backend,
        }],
        Some(binding),
        None,
    )?))
}

/// One call: `response_mode` is inserted for tools that take one unless the
/// arguments already name it, then the call runs under the shared capacity
/// observation.
pub async fn call(
    backend: &dyn RemoteTools,
    tool: String,
    mut arguments: Map<String, Value>,
    mode: ResponseMode,
) -> Result<Value> {
    if backend.uses_response_mode(&tool) {
        arguments
            .entry("response_mode")
            .or_insert_with(|| serde_json::to_value(mode).expect("scalar response mode"));
    }
    airk_mcp::remote::call_observed(backend, tool, arguments).await
}

/// Same canonical projection under an already-owned original read allowance.
pub async fn call_with_context(
    backend: &dyn RemoteTools,
    tool: String,
    mut arguments: Map<String, Value>,
    mode: ResponseMode,
    context: airk_mcp::read_attempt::ReadAttemptContext,
) -> Result<Value> {
    if backend.uses_response_mode(&tool) {
        arguments
            .entry("response_mode")
            .or_insert_with(|| serde_json::to_value(mode).expect("scalar response mode"));
    }
    airk_mcp::remote::call_observed_with_context(backend, tool, arguments, context).await
}

/// Whether the CLI answers `tool` in process: its help tool, for any call
/// without a catalog or combined session, before resolving a project.
pub fn is_local_help(tool: &str) -> bool {
    matches!(tool, "airk_help" | "help")
}

/// The in-process help answer.
pub fn local_help(arguments: Map<String, Value>) -> Result<Value> {
    Ok(serde_json::to_value(airk_mcp::help::call(Value::Object(
        arguments,
    )))?)
}

/// A combined session's narrowing defaults added to one call's arguments; an
/// argument naming the same selection is refused, never overridden.
pub fn with_defaults(
    mut arguments: Map<String, Value>,
    defaults: &Map<String, Value>,
) -> Result<Map<String, Value>> {
    for (key, value) in defaults {
        ensure!(!arguments.contains_key(key), "conflicting {key} selections");
        arguments.insert(key.clone(), value.clone());
    }
    Ok(arguments)
}

/// A unified envelope is an object with a `content` array.
pub fn require_envelope(result: &Value) -> Result<()> {
    ensure!(
        result.is_object() && result["content"].is_array(),
        "invalid native tool response"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
