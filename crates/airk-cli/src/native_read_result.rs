// SPDX-License-Identifier: Apache-2.0
//! The classification of a complete native read result: a refusal keeps its
//! complete body as a typed error. Moved from `usage_client` (split step 2b);
//! `usage_client` re-exports both items at their original paths.
use anyhow::Result;
use serde_json::Value;

#[derive(Debug)]
pub struct NativeReadFailure(Value);
impl std::fmt::Display for NativeReadFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("native project read failed; see the complete result")
    }
}
impl std::error::Error for NativeReadFailure {}
impl NativeReadFailure {
    pub(crate) fn from_response(value: Value) -> Self {
        Self(value)
    }
    pub fn body(&self) -> &Value {
        &self.0
    }
}
pub fn require_native_success(value: Value) -> Result<Value> {
    if value["isError"] == true || value.get("error").is_some() {
        return Err(NativeReadFailure(value).into());
    }
    Ok(value)
}
