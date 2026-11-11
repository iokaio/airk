// SPDX-License-Identifier: Apache-2.0
//! Host-injected embedding transport. Native process/IPC adapters remain in the CLI.
use crate::model::{EmbeddingCacheStats, PlanningTokens};
use airk_core::model_objects::ModelObjectSet;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub const PROTOCOL: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// Transport availability only; protocol, scope and model-integrity errors do
/// not carry this marker and must not become a degraded search success.
#[derive(Debug)]
pub struct HelperUnavailable;
impl std::fmt::Display for HelperUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("shared embedding helper temporarily unavailable")
    }
}
impl std::error::Error for HelperUnavailable {}

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

/// The connector slot of one in-process worker, shared by every thread it
/// binds. A child-process worker uses the process slot instead.
#[derive(Clone, Default)]
pub struct ConnectorScope(Arc<OnceLock<Arc<dyn Connector>>>);
thread_local! { static SCOPE: RefCell<Option<ConnectorScope>> = const { RefCell::new(None) }; }

/// Restores the thread's previous binding when dropped.
pub struct ConnectorBinding(Option<ConnectorScope>);
impl Drop for ConnectorBinding {
    fn drop(&mut self) {
        let previous = self.0.take();
        // A thread-exit binding may drop after this thread's slot is gone.
        let _ = SCOPE.try_with(|slot| *slot.borrow_mut() = previous);
    }
}

impl ConnectorScope {
    pub fn new() -> Self {
        Self::default()
    }
    /// The in-process equivalent of [`install_connector`]: once per scope.
    pub fn install(&self, connector: Arc<dyn Connector>) -> Result<()> {
        self.0
            .set(connector)
            .map_err(|_| anyhow::anyhow!("embedding adapter already installed"))
    }
    /// Bind this scope to the calling thread until the returned guard drops.
    pub fn bind_thread(&self) -> ConnectorBinding {
        ConnectorBinding(SCOPE.with(|slot| slot.borrow_mut().replace(self.clone())))
    }
    /// The calling thread's binding, for helper threads of the same work.
    pub fn current() -> Option<Self> {
        SCOPE.with(|slot| slot.borrow().clone())
    }
}

/// Install once from the trusted native adapter, never from workspace configuration.
pub fn install_connector(connector: Arc<dyn Connector>) -> Result<()> {
    CONNECTOR
        .set(connector)
        .map_err(|_| anyhow::anyhow!("embedding adapter already installed"))
}

/// The calling thread's connector: its bound scope's, otherwise the process
/// connector. A bound thread never falls back to the process connector.
pub fn connector() -> Result<Arc<dyn Connector>> {
    SCOPE
        .with(|slot| match slot.borrow().as_ref() {
            Some(scope) => scope.0.get().cloned(),
            None => CONNECTOR.get().cloned(),
        })
        .ok_or_else(|| {
            anyhow::anyhow!("embedding worker is unavailable; native adapter not installed")
        })
}
