// SPDX-License-Identifier: Apache-2.0
//! Same-user local transport (split plan Phase 2): owner-only endpoints with
//! peer checks (`endpoint`) and the length-prefixed JSON frame codec (`frame`).
//! See `CONTRACT.md`. Transport only: no application policy lives here.
pub mod endpoint;
pub mod frame;
