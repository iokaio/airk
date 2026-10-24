// SPDX-License-Identifier: Apache-2.0
//! Host-injected embedding transport. Native process/IPC adapters remain in the CLI.
use crate::model::{EmbeddingCacheStats, PlanningTokens};
use airk_core::model_objects::ModelObjectSet;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub const PROTOCOL: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelLocation {
    Shared {
        objects: ModelObjectSet,
    },
    Directory {
        path: PathBuf,
    },
    Retained {
        collection: PathBuf,
        objects: ModelObjectSet,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    ProvisionBundled,
    ProvisionModel {
        directory: PathBuf,
    },
    TokenizerOpen {
        protocol: u32,
        version: String,
        path: PathBuf,
    },
    TokenCount {
        text: String,
    },
    Extract {
        directory: PathBuf,
    },
    Open {
        protocol: u32,
        version: String,
        location: ModelLocation,
        identity: String,
    },
    Plan {
        text: String,
    },
    Embed {
        texts: Vec<String>,
        workers: usize,
        memory_bytes: usize,
    },
    Peak,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Provisioned {
        objects: ModelObjectSet,
    },
    TokenizerOpened {
        identity: airk_core::usage::TokenizerIdentity,
        description: serde_json::Value,
    },
    TokenCount {
        count: u64,
    },
    Extracted,
    Opened {
        protocol: u32,
        version: String,
        identity: String,
    },
    Planned {
        tokens: PlanningTokens,
    },
    Embedded {
        vectors: Vec<Option<Vec<f32>>>,
        execution: Box<EmbeddingCacheStats>,
    },
    Peak {
        bytes: u64,
    },
    Failed {
        reason: String,
    },
}

pub trait RemoteSession: Send + Sync {
    fn request(&self, request: Request) -> Result<Response>;
}

pub trait Connector: Send + Sync {
    fn provision_bundled(&self) -> Result<ModelObjectSet> {
        anyhow::bail!("shared model provisioning is unavailable")
    }
    fn provision_model(&self, _source: &std::path::Path) -> Result<ModelObjectSet> {
        anyhow::bail!("shared custom model provisioning is unavailable")
    }
    fn tokenizer(&self, _path: &std::path::Path) -> Result<Arc<dyn RemoteSession>> {
        anyhow::bail!("tokenizer worker is unavailable")
    }
    fn open(&self, location: ModelLocation, identity: &str) -> Result<Arc<dyn RemoteSession>>;
}

static CONNECTOR: OnceLock<Arc<dyn Connector>> = OnceLock::new();

/// Install once from the trusted native adapter, never from workspace configuration.
pub fn install_connector(connector: Arc<dyn Connector>) -> Result<()> {
    CONNECTOR
        .set(connector)
        .map_err(|_| anyhow::anyhow!("embedding adapter already installed"))
}

pub fn connector() -> Result<&'static Arc<dyn Connector>> {
    CONNECTOR.get().ok_or_else(|| {
        anyhow::anyhow!("embedding worker is unavailable; native adapter not installed")
    })
}
