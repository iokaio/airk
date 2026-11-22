// SPDX-License-Identifier: Apache-2.0
//! The Core semantics a loop card task policy binds: card classes and their
//! stages, card advice and its acceptance shape, loop calibration, the batched
//! Focus receipt, the decision log's roster and partition rules, and the
//! private native card and batched-Focus inputs, plus the Focus Core
//! semantics that batched Focus scores under. The retrieval task policies
//! hash this digest, so any change here moves every loop task identity.
use super::*;

pub fn loop_semantic_policy_digest() -> Result<Sha256Digest> {
    fn source(bytes: &[u8]) -> Result<Sha256Digest> {
        let canonical = std::str::from_utf8(bytes)?.replace("\r\n", "\n");
        ensure!(!canonical.contains('\r'), "lone CR in loop Core semantics");
        Ok(crate::identity::framed_digest(
            b"airk.arbiter.loop-core-source-lf/1",
            &[canonical.as_bytes()],
        ))
    }
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.loop-core-semantics/1",
        &[
            source(include_bytes!("cards.rs"))?.as_bytes(),
            source(include_bytes!("calibration.rs"))?.as_bytes(),
            source(include_bytes!("focus_batch.rs"))?.as_bytes(),
            source(include_bytes!("log.rs"))?.as_bytes(),
            source(include_bytes!("semantics.rs"))?.as_bytes(),
            source(include_bytes!("../../native_loop.rs"))?.as_bytes(),
            super::super::focus::semantic_policy_digest()?.as_bytes(),
        ],
    ))
}
