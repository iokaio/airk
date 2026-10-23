// SPDX-License-Identifier: Apache-2.0
//! Explicit quota-reserved model provisioning through the selected runtime.
use anyhow::{Result, ensure};

pub fn manifest() -> Result<serde_json::Value> {
    airk_cli::bundled_assets::manifest()
}

pub fn install(kernel: &airk_kernel::Kernel) -> Result<airk_kernel::vectors::VectorStatus> {
    ensure!(
        kernel.vectors_enabled()?,
        "vector search is disabled for this collection"
    );
    let source = tempfile::Builder::new()
        .prefix("airk-bundled-model-")
        .tempdir_in(std::env::temp_dir().canonicalize()?)?;
    #[cfg(feature = "bundled-model")]
    airk_cli::bundled_assets::extract(source.path())?;
    #[cfg(not(feature = "bundled-model"))]
    airk_cli::embedding_client::extract_bundled(source.path())?;
    kernel.install_default_vector_model(source.path())
}
