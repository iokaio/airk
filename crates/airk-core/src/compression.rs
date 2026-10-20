// SPDX-License-Identifier: Apache-2.0
//! Physical encoding is separate from logical source and embedding identity.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionMode {
    #[default]
    Off,
    Automatic,
    Space,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncodedObject {
    pub version: u32,
    pub logical_digest: String,
    pub stored_digest: String,
    pub stored_bytes: u64,
    pub index_digest: String,
}
