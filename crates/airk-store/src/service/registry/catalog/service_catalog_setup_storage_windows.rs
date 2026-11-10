// SPDX-License-Identifier: Apache-2.0
//! Test host for the Windows setup regressions. The HANDLE-relative directory
//! implementation moved to `crate::store_fs` in step S7.5. These tests also
//! drive setup storage internals, so they stay here under their original names.
#[cfg(test)]
use crate::store_fs::{SetupDirectory, SetupFileMode, native_file_identity, nt_result};
#[cfg(test)]
use anyhow::{Context, Result, ensure};
#[cfg(test)]
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    path::Path,
};

#[cfg(test)]
#[path = "service_catalog_setup_storage_windows_tests.rs"]
mod tests;
