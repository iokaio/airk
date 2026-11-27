// SPDX-License-Identifier: Apache-2.0
//! Content-free status memory for one open MCP session, outside per-call workers.
use super::{ResponseMode, Session, explicit_status};
use serde_json::{Map, Value};
use std::sync::Mutex;

/// One open protocol session's bounded display observations. The service's
/// built-in local connection may be shared by unrelated clients, so this owner
/// belongs to the HTTP backend session, never that service-global connection.
/// It only elides unchanged top-level status on validated single-index results.
#[derive(Default)]
pub struct OpenStatusSession(Mutex<Option<Session>>);

impl OpenStatusSession {
    /// Apply canonical status memory after native open-read rendering. Indexed
    /// evidence, source verification, scope metadata and bound combined bodies
    /// are preserved. Missing, unrecognized or disagreeing representations are
    /// left unchanged rather than guessed or repaired at this display boundary.
    pub fn display(&self, tool: &str, arguments: &Map<String, Value>, result: &mut Value) {
        let text = match result.get("content") {
            Some(Value::Array(blocks)) if blocks.len() == 1 => {
                let block = &blocks[0];
                if block.get("type").and_then(Value::as_str) != Some("text") {
                    return;
                }
                let Some(body) = block
                    .get("text")
                    .and_then(Value::as_str)
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    .filter(Value::is_object)
                else {
                    return;
                };
                Some(body)
            }
            None => None,
            Some(Value::Array(blocks)) if blocks.is_empty() => None,
            _ => return,
        };
        let structured = result.get("structuredContent");
        let original = match (&text, structured) {
            (Some(text), Some(structured)) if text == structured => text,
            (Some(text), None) => text,
            (None, Some(structured)) if structured.is_object() => structured,
            _ => return,
        };
        if original.get("delivery_observation").is_some()
            || original.get("schema").and_then(Value::as_str)
                == Some(airk_core::combined_reads::COMBINED_READ_SCHEMA)
        {
            return;
        }
        let Some(scope) = original
            .get("scope_observation")
            .and_then(|scope| {
                serde_json::from_value::<airk_core::scoped_reads::ScopeObservation>(scope.clone())
                    .ok()
            })
            .filter(|scope| scope.members.len() == 1 && scope.validate().is_ok())
        else {
            return;
        };
        let mut value = original.clone();
        let object = value.as_object_mut().expect("validated native object");
        let full = arguments
            .get("response_mode")
            .and_then(|mode| serde_json::from_value::<ResponseMode>(mode.clone()).ok())
            .unwrap_or_else(ResponseMode::from_environment)
            == ResponseMode::Full;
        let explicit = full
            || result.get("isError").and_then(Value::as_bool) == Some(true)
            || matches!(
                airk_core::tool_names::operation(tool),
                "status" | "diagnostics" | "capabilities" | "changes"
            )
            || explicit_status(
                object,
                arguments.get("include_status").and_then(Value::as_bool) == Some(true),
            );
        let Ok(mut current) = self.0.lock() else {
            return;
        };
        if current
            .as_ref()
            .is_none_or(|session| session.scope != Some(scope.fingerprint))
        {
            *current = Some(Session::for_scope(Some(scope.fingerprint)));
        }
        current
            .as_ref()
            .expect("current scope initialized")
            .status(object, explicit);
        if value == *original {
            return;
        }
        if structured.is_some() {
            result["structuredContent"] = value.clone();
        }
        if text.is_some() {
            result["content"][0]["text"] = value.to_string().into();
        }
    }
}
