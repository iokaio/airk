// SPDX-License-Identifier: Apache-2.0
//! Transformer tokenization runs in the verified helper; byte-BPE remains local.
use crate::usage::{LocalByteBpe, read_usage_input};
use airk_core::usage::TokenizerIdentity;
use airk_retrieval::embedding_worker::{self, RemoteSession, Request, Response};
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

pub(crate) enum LocalTokenizer {
    Byte(LocalByteBpe),
    Remote {
        session: Arc<dyn RemoteSession>,
        identity: TokenizerIdentity,
        description: Value,
    },
}

impl LocalTokenizer {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        Self::open_checked(path, None)
    }
    pub(crate) fn open_checked(
        path: &Path,
        expected: Option<airk_core::identity::Sha256Digest>,
    ) -> Result<Self> {
        let bytes = read_usage_input(path, 1_048_576)?;
        ensure!(
            expected.is_none_or(|expected| airk_core::identity::Sha256Digest::from_bytes(
                Sha256::digest(&bytes).into()
            ) == expected),
            "selected calibration definition changed before setup"
        );
        let value: Value = serde_json::from_slice(&bytes)?;
        if value["schema"] == 1 {
            return Ok(Self::Byte(LocalByteBpe::parse(&bytes)?));
        }
        ensure!(
            value["schema"] == 2,
            "unsupported tokenizer manifest schema"
        );
        let session = embedding_worker::connector()?.tokenizer(&path.canonicalize()?)?;
        let response = session.request(Request::TokenizerOpen {
            protocol: embedding_worker::PROTOCOL,
            version: env!("CARGO_PKG_VERSION").into(),
            path: path.canonicalize()?,
        })?;
        match response {
            Response::TokenizerOpened {
                identity,
                description,
            } => {
                let mut hash = Sha256::new();
                hash.update(b"airk-local-calibration/2\0tokenizers/0.23.2\0");
                hash.update(&bytes);
                ensure!(
                    identity.definition_sha256 == format!("{:x}", hash.finalize()),
                    "calibration definition changed during helper setup"
                );
                Ok(Self::Remote {
                    session,
                    identity,
                    description,
                })
            }
            _ => bail!("invalid tokenizer worker response"),
        }
    }
    pub(crate) fn identity(&self) -> &TokenizerIdentity {
        match self {
            Self::Byte(bpe) => &bpe.identity,
            Self::Remote { identity, .. } => identity,
        }
    }
    pub(crate) fn count(&self, text: &str) -> Result<u64> {
        ensure!(text.len() <= 262_144, "tokenizer text exceeds bound");
        match self {
            Self::Byte(bpe) => bpe.count(text),
            Self::Remote { session, .. } => {
                match session.request(Request::TokenCount { text: text.into() })? {
                    Response::TokenCount { count } => Ok(count),
                    _ => bail!("invalid tokenizer worker response"),
                }
            }
        }
    }
    pub(crate) fn description(&self) -> Value {
        match self {
            Self::Byte(_) => {
                json!({"runtime":"utf8-byte-bpe/1", "framing":"none", "special_tokens":false})
            }
            Self::Remote { description, .. } => description.clone(),
        }
    }
}
