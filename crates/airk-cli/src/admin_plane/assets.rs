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

/// The I4 record's `admin_assets` digests of this image's embedded bundle
/// (split Phase 7; `docs/split/contracts.md` §5.3). Both frame every asset in
/// name order with `airk_core::identity::framed_digest`:
/// `inventory_sha256` its name, SHA-256 and content type; `bundle_sha256` its
/// name and bytes. `scripts/build-local-service-installer.py` computes the
/// same values from `crates/airk-cli/admin-assets/` for the payload inventory.
pub(crate) fn digests() -> (
    airk_core::identity::Sha256Digest,
    airk_core::identity::Sha256Digest,
) {
    let mut sorted: Vec<_> = ADMIN_ASSETS.iter().collect();
    sorted.sort_by_key(|(name, ..)| *name);
    let mut inventory: Vec<&[u8]> = Vec::new();
    let mut bundle: Vec<&[u8]> = Vec::new();
    for (name, sha256, content_type, bytes) in &sorted {
        inventory.extend([name.as_bytes(), sha256.as_bytes(), content_type.as_bytes()]);
        bundle.extend([name.as_bytes(), *bytes]);
    }
    (
        airk_core::identity::framed_digest(b"airk/admin-asset-inventory/1", &inventory),
        airk_core::identity::framed_digest(b"airk/admin-asset-bundle/1", &bundle),
    )
}
