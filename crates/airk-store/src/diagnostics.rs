// SPDX-License-Identifier: Apache-2.0
//! Content-free cause codes for durable job receipts. Never format raw exceptions.
pub fn failure_code(error: &anyhow::Error) -> Option<String> {
    if error.is::<crate::quota::QuotaContention>() {
        return Some("quota_ledger_busy".into());
    }
    if let Some(rusqlite::Error::SqliteFailure(code, _)) = error.downcast_ref::<rusqlite::Error>() {
        return Some(format!(
            "sqlite_error_{}",
            code.extended_code.unsigned_abs()
        ));
    }
    let io = error.downcast_ref::<std::io::Error>().or_else(|| {
        match error.downcast_ref::<std::fs::TryLockError>() {
            Some(std::fs::TryLockError::Error(io)) => Some(io),
            _ => None,
        }
    });
    if let Some(io) = io {
        return Some(if let Some(code) = io.raw_os_error() {
            format!("filesystem_error_{}", code.unsigned_abs())
        } else {
            match io.kind() {
                std::io::ErrorKind::NotFound => "filesystem_not_found",
                std::io::ErrorKind::PermissionDenied => "filesystem_permission_denied",
                std::io::ErrorKind::WouldBlock => "filesystem_busy",
                std::io::ErrorKind::InvalidData => "filesystem_invalid_data",
                _ => "filesystem_error",
            }
            .into()
        });
    }
    if matches!(
        error.downcast_ref::<std::fs::TryLockError>(),
        Some(std::fs::TryLockError::WouldBlock)
    ) {
        return Some("native_lock_busy".into());
    }
    if let Some(error) = error.downcast_ref::<serde_json::Error>() {
        return Some(
            match error.classify() {
                serde_json::error::Category::Io => "metadata_read_failed",
                serde_json::error::Category::Syntax => "metadata_json_invalid",
                serde_json::error::Category::Data => "metadata_schema_invalid",
                serde_json::error::Category::Eof => "metadata_incomplete",
            }
            .into(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn cause_codes_preserve_native_codes_without_paths_sql_or_exception_text() {
        let sql: anyhow::Error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("private SQL".into()),
        )
        .into();
        assert_eq!(
            super::failure_code(&sql.context("private path")),
            Some("sqlite_error_5".into())
        );
        let io: anyhow::Error = std::io::Error::from_raw_os_error(32).into();
        assert_eq!(super::failure_code(&io), Some("filesystem_error_32".into()));
        assert_eq!(
            super::failure_code(&anyhow::anyhow!("private source text")),
            None
        );
    }
}
