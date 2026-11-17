// SPDX-License-Identifier: Apache-2.0
//! Exact GPU receipt execution tags; no native admission authority.
use crate::arbiter::RUNTIME;

/// Syntax/identity contract only; this string never grants native admission.
/// GPU receipts bind exact execution digest, not a bare enabled-provider label.
pub fn valid_advice_runtime(value: &str) -> bool {
    if value == RUNTIME {
        return true;
    }
    [
        "onnxruntime-cuda/1.30.0/device-execution/",
        "onnxruntime-directml/1.30.0/device-execution/",
        "onnxruntime-coreml/1.30.0/device-execution/",
    ]
    .iter()
    .any(|prefix| {
        value.strip_prefix(prefix).is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    })
}
