// SPDX-License-Identifier: Apache-2.0
//! The sealed admin bundle: files under `crates/airk-cli/admin-assets/`
//! listed in its `manifest.json`. `build.rs` refuses a missing or stale file
//! and embeds verified bytes; lookup is by exact name only, so no request
//! path ever reaches the filesystem.
include!(concat!(env!("OUT_DIR"), "/admin_assets.rs"));

/// The one shell document; `/admin/` and `/admin/launch` both serve it (M1).
pub(super) const SHELL: &str = "index.html";

pub(super) struct Asset {
    pub(super) content_type: &'static str,
    pub(super) bytes: &'static [u8],
}

pub(super) fn find(name: &str) -> Option<Asset> {
    ADMIN_ASSETS
        .iter()
        .find(|(candidate, ..)| *candidate == name)
        .map(|&(_, _, content_type, bytes)| Asset {
            content_type,
            bytes,
        })
}
