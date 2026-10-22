// SPDX-License-Identifier: Apache-2.0
//! Provider admission callback; storage owns reservations.
use anyhow::Result;
use std::sync::Arc;

pub type Charge = Arc<dyn Fn(u64) -> Result<()> + Send + Sync>;
