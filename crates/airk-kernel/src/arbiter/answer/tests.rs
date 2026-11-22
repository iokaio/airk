// SPDX-License-Identifier: Apache-2.0
//! Shared synthetic fixtures for the Answer /3 loop tests: canonical replies
//! shaped like the native reads, with no index, model or transport.
use super::*;
use airk_core::{
    DocumentLocation,
    evidence::RevisionEnvelope,
    identity::{Incarnation, RevisionToken},
    index_catalog::IndexId,
};

pub(crate) fn scope() -> ScopeBinding {
    ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        authorization: Sha256Digest::from_bytes([4; 32]),
        configuration_revision: 1,
        publication: RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([5; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([6; 32]),
            },
        ),
    }
}

/// A hit whose content hash is a real digest, as native Focus requires.
pub(crate) fn hit(path: &str, start: usize, end: usize, text: &str) -> Value {
    let hash = framed_digest(b"fixture-hash", &[path.as_bytes(), text.as_bytes()]);
    json!({"citation":{"path":path,"content_hash":hash.to_string(),"start_line":start,"end_line":end,
        "location":DocumentLocation::Lines {start,end}},"text":text,"truncated":false})
}

pub(crate) fn reply(scope: &ScopeBinding, hits: Vec<Value>) -> Value {
    json!({"publication":scope.publication,"generation":1,"collection":"fixture","truncated":false,"hits":hits})
}
