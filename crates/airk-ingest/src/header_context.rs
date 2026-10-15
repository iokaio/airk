// SPDX-License-Identifier: Apache-2.0
//! Captured compilation arguments, never filesystem discovery or compiler execution.
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CONFIGURATION_BYTES: usize = 131_072;
pub const MAX_CONFIGURATIONS: usize = 16;
pub const CONTEXT_VERSION: &str = "compilation-database-arguments/1";
pub type CapturedMap = BTreeMap<String, CapturedContext>;

#[derive(Clone, Debug)]
pub struct CapturedContext {
    path: String,
    digest: Sha256Digest,
    headers: BTreeMap<String, &'static str>,
    ambiguous: BTreeSet<String>,
}
impl CapturedContext {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn digest(&self) -> Sha256Digest {
        self.digest
    }
    pub fn adapter(&self, header: &str) -> Option<&'static str> {
        (!self.ambiguous.contains(header))
            .then(|| self.headers.get(header).copied())
            .flatten()
    }
}

#[derive(Deserialize)]
struct Entry {
    file: String,
    #[serde(default)]
    directory: Option<String>,
    #[serde(default)]
    arguments: Option<Vec<String>>,
}

/// Caller has already verified registered-root eligibility and charged the read.
/// The digest is computed from actual captured bytes, never a declared checksum.
pub fn capture_configuration(logical_path: &str, bytes: &[u8]) -> Result<CapturedContext> {
    crate::adapters::profile::validate_relative_path(logical_path)?;
    ensure!(
        bytes.len() <= MAX_CONFIGURATION_BYTES,
        "header configuration exceeds 128 KiB"
    );
    let entries: Vec<Entry> = serde_json::from_slice(bytes)?;
    ensure!(
        entries.len() <= 1024,
        "header configuration exceeds entry limit"
    );
    let parent = logical_path.rsplit_once('/').map(|(p, _)| p);
    let mut result = CapturedContext {
        path: logical_path.into(),
        digest: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
        headers: BTreeMap::new(),
        ambiguous: BTreeSet::new(),
    };
    for entry in entries {
        ensure!(
            entry.file.len() <= 4096,
            "compilation file spelling exceeds limit"
        );
        if !entry.file.to_ascii_lowercase().ends_with(".h") {
            continue;
        }
        crate::adapters::profile::validate_relative_path(&entry.file)?;
        let header = parent.map_or_else(
            || entry.file.clone(),
            |parent| format!("{parent}/{}", entry.file),
        );
        crate::adapters::profile::validate_relative_path(&header)?;
        if entry
            .directory
            .as_deref()
            .is_some_and(|d| !matches!(d, "" | "."))
        {
            result.ambiguous.insert(header);
            continue;
        }
        let Some(arguments) = entry.arguments else {
            result.ambiguous.insert(header);
            continue;
        };
        ensure!(
            arguments.len() <= 128 && arguments.iter().all(|a| a.len() <= 4096),
            "compilation arguments exceed limits"
        );
        let mut languages = BTreeSet::new();
        let mut unsupported = false;
        for (index, argument) in arguments.iter().enumerate() {
            let language = if argument == "-x" {
                arguments.get(index + 1).map(String::as_str)
            } else {
                argument.strip_prefix("-x").filter(|v| !v.is_empty())
            };
            if argument == "-x" && language.is_none() {
                unsupported = true;
            }
            if let Some(language) = language {
                match language {
                    "c" => {
                        languages.insert("c");
                    }
                    "c++" => {
                        languages.insert("cpp");
                    }
                    _ => unsupported = true,
                }
            }
        }
        if unsupported || languages.len() != 1 {
            result.ambiguous.insert(header);
            continue;
        }
        let adapter = *languages.first().unwrap();
        if result
            .headers
            .insert(header.clone(), adapter)
            .is_some_and(|old| old != adapter)
        {
            result.ambiguous.insert(header);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_bytes_and_configuration_parent_bind_header_context() -> Result<()> {
        let bytes=br#"[{"directory":".","file":"include/beacon.h","arguments":["compiler","-x","c++","include/beacon.h"]}]"#;
        let context = capture_configuration("@vendor/build/compile_commands.json", bytes)?;
        assert_eq!(
            context.adapter("@vendor/build/include/beacon.h"),
            Some("cpp")
        );
        assert_eq!(context.adapter("include/beacon.h"), None);
        assert_eq!(
            context.digest(),
            Sha256Digest::from_bytes(Sha256::digest(bytes).into())
        );
        assert!(
            capture_configuration(
                "commands.json",
                br#"[{"file":"../escape.h","arguments":["cc","-x","c"]}]"#
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn command_strings_conflicts_and_unsupported_directories_never_guess() -> Result<()> {
        for bytes in [
            r#"[{"file":"a.h","command":"clang++ -x c++ a.h"}]"#,
            r#"[{"file":"a.h","directory":"/outside","arguments":["cc","-x","c++"]}]"#,
            r#"[{"file":"a.h","arguments":["clang++","a.h"]}]"#,
            r#"[{"file":"a.h","arguments":["cc","-x","c","-x","c++"]}]"#,
            r#"[{"file":"a.h","arguments":["cc","-x","c"]},{"file":"a.h","arguments":["cc","-x","c++"]}]"#,
        ] {
            assert_eq!(
                capture_configuration("commands.json", bytes.as_bytes())?.adapter("a.h"),
                None
            );
        }
        Ok(())
    }
}
