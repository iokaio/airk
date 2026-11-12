// SPDX-License-Identifier: Apache-2.0
//! Same-user local transport for AIRK (split plan §10.1, U27): owner-only
//! endpoints with peer checks (`endpoint`), the length-prefixed JSON frame codec
//! (`frame`) and the I1 control request (`control`). See `CONTRACT.md`.
//! Transport only: no registry, store, kernel or MCP dependency.
pub mod control;
pub mod endpoint;
pub mod frame;
pub mod private_record;
