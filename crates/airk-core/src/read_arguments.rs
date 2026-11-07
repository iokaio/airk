// SPDX-License-Identifier: Apache-2.0
//! Machine-readable read-input validation shared by native transports.
#[derive(Clone, Debug)]
pub struct ReadArgumentError {
    pub code: &'static str,
    pub field: String,
    pub message: String,
}
impl ReadArgumentError {
    pub fn new(code: &'static str, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            field: field.into(),
            message: message.into(),
        }
    }
    pub fn receipt(&self) -> serde_json::Value {
        let (field, field_truncated) = bounded(&self.field, 256);
        let (message, message_truncated) = bounded(&self.message, 1024);
        let mut body = serde_json::json!({"error":{"code":self.code,"field":field,
            "message":message,"retryable":false}});
        if field_truncated || message_truncated {
            body["error"]["details_truncated"] = true.into();
            body["error"]["field_truncated"] = field_truncated.into();
            body["error"]["message_truncated"] = message_truncated.into();
        }
        body
    }
}
fn bounded(value: &str, maximum: usize) -> (&str, bool) {
    let mut end = value.len().min(maximum);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (&value[..end], end < value.len())
}
impl std::fmt::Display for ReadArgumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (field, field_truncated) = bounded(&self.field, 256);
        let (message, message_truncated) = bounded(&self.message, 1024);
        write!(
            formatter,
            "{field}: {message}{}",
            if field_truncated || message_truncated {
                " [truncated]"
            } else {
                ""
            }
        )
    }
}
impl std::error::Error for ReadArgumentError {}
