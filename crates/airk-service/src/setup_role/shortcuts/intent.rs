// SPDX-License-Identifier: Apache-2.0
//! Original-operation alias selection. Probes do not reserve or adopt output.
use super::*;
use crate::setup_role::CliAliasSelection;
use airk_ipc::private_record;
use airk_process::directory::OpenedDirectory;
use std::{ffi::OsStr, io::Read};

const INTENT: &str = "cli-alias-intent.json";

pub(in crate::setup_role) struct AliasContext {
    pub program: PathBuf,
    pub home: PathBuf,
    pub target: PathBuf,
    pub install_id: Incarnation,
    pub allow_owned: bool,
    pub lookup_directory: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::setup_role) struct Intent {
    schema: String,
    operation_sha256: Sha256Digest,
    pub alias: Option<String>,
    pub search_path: String,
    pub path_extensions: Vec<String>,
    pub lookup_directory: PathBuf,
}

pub(in crate::setup_role) fn extensions() -> Result<Vec<String>> {
    #[cfg(windows)]
    {
        let mut values = vec![".exe".into(), ".com".into(), ".bat".into(), ".cmd".into()];
        if let Some(raw) = std::env::var_os("PATHEXT") {
            let raw = raw
                .into_string()
                .map_err(|_| anyhow::anyhow!("PATHEXT must be UTF-8"))?;
            ensure!(raw.len() <= 4096, "PATHEXT exceeds bound");
            for value in raw.split(';').filter(|s| !s.is_empty()) {
                ensure!(
                    value.starts_with('.')
                        && value.len() <= 32
                        && value[1..].bytes().all(|b| b.is_ascii_alphanumeric()),
                    "unsafe PATHEXT"
                );
                let value = value.to_ascii_lowercase();
                if !values.contains(&value) {
                    values.push(value);
                }
            }
        }
        ensure!(values.len() <= 32, "too many executable extensions");
        Ok(values)
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

fn occupied(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    if a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
    {
        return true;
    }
    false
}

pub(in crate::setup_role) fn probe(
    context: &AliasContext,
    alias: &str,
    search_path: &str,
    path_extensions: &[String],
) -> Result<&'static str> {
    validate_alias(alias)?;
    ensure!(
        search_path.len() <= LIMIT && !search_path.contains('\0'),
        "PATH exceeds bound"
    );
    let wanted = expected(&context.home, &context.target, context.install_id, alias)?;
    let program = match OpenedDirectory::open(&context.program) {
        Ok(program) => Some(program),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let slot_exists = occupied(&wanted.launcher_path)?;
    let owned = if slot_exists && context.allow_owned {
        match &program {
            Some(program) => available(
                program,
                &context.home,
                &context.target,
                context.install_id,
                alias,
            )?,
            None => false,
        }
    } else {
        false
    };
    if slot_exists && !owned {
        return Ok("collision");
    }
    // A stage collision is relevant even before a destination exists.
    if let Some(program) = &program
        && !available(
            program,
            &context.home,
            &context.target,
            context.install_id,
            alias,
        )?
    {
        return Ok("collision");
    }
    let mut folders: Vec<PathBuf> = std::env::split_paths(OsStr::new(search_path)).collect();
    ensure!(folders.len() <= 256, "PATH has too many entries");
    // Check future installation bin too, even before it is put on PATH.
    folders.push(
        wanted
            .launcher_path
            .parent()
            .context("alias parent missing")?
            .to_owned(),
    );
    let mut suffixes = vec![String::new()];
    suffixes.extend_from_slice(path_extensions);
    ensure!(suffixes.len() <= 33, "too many executable extensions");
    for folder in folders {
        let folder = if folder.is_absolute() {
            folder
        } else {
            context.lookup_directory.join(folder)
        };
        for suffix in &suffixes {
            ensure!(
                suffix.is_empty()
                    || suffix.starts_with('.')
                        && suffix.len() <= 32
                        && suffix[1..].bytes().all(|b| b.is_ascii_alphanumeric()),
                "invalid extension snapshot"
            );
            let path = folder.join(format!("{alias}{suffix}"));
            if same_path(&path, &wanted.launcher_path) && owned {
                continue;
            }
            if occupied(&path)? {
                return Ok("collision");
            }
        }
    }
    Ok(if owned { "owned" } else { "available" })
}

pub(in crate::setup_role) fn select(
    context: &AliasContext,
    choice: &CliAliasSelection,
    search_path: &str,
) -> Result<Intent> {
    let path_extensions = if matches!(choice, CliAliasSelection::None) {
        Vec::new()
    } else {
        extensions()?
    };
    let alias = match choice {
        CliAliasSelection::None => None,
        CliAliasSelection::Named(name) => {
            ensure!(
                probe(context, name, search_path, &path_extensions)? != "collision",
                "cli_alias_collision: selected alias is occupied"
            );
            Some(name.clone())
        }
        CliAliasSelection::Auto => {
            let mut chosen = None;
            for index in 0..10000 {
                let name = if index == 0 {
                    "rk".into()
                } else {
                    format!("rk{index}")
                };
                if probe(context, &name, search_path, &path_extensions)? != "collision" {
                    chosen = Some(name);
                    break;
                }
            }
            Some(chosen.context("cli_alias_exhausted: rk through rk9999 are occupied")?)
        }
    };
    Ok(Intent {
        schema: "airk-cli-alias-intent/1".into(),
        operation_sha256: Sha256Digest::from_bytes([0; 32]),
        alias,
        search_path: search_path.into(),
        path_extensions,
        lookup_directory: context.lookup_directory.clone(),
    })
}

pub(in crate::setup_role) fn load_intent(
    operation_path: &Path,
    operation_sha256: Sha256Digest,
) -> Result<Option<Intent>> {
    let root = OpenedDirectory::open(
        operation_path
            .parent()
            .context("operation parent missing")?,
    )?;
    let Some(bytes) = private_record::read_private_at(&root, INTENT, LIMIT)? else {
        return Ok(None);
    };
    let saved: Intent = serde_json::from_slice(&bytes)?;
    ensure!(
        saved.schema == "airk-cli-alias-intent/1" && saved.operation_sha256 == operation_sha256,
        "CLI alias intent differs from original operation"
    );
    ensure!(
        saved.lookup_directory.is_absolute(),
        "alias lookup directory must be absolute"
    );
    if let Some(alias) = &saved.alias {
        validate_alias(alias)?;
    }
    ensure!(
        saved.search_path.len() <= LIMIT && !saved.search_path.contains('\0'),
        "invalid intent PATH"
    );
    Ok(Some(saved))
}

pub(in crate::setup_role) fn bind(
    operation_path: &Path,
    operation_sha256: Sha256Digest,
    context: &AliasContext,
    choice: Option<&CliAliasSelection>,
    search_path: Option<&str>,
    admitted: bool,
) -> Result<()> {
    if load_intent(operation_path, operation_sha256)?.is_some() {
        ensure!(
            choice.is_none() && search_path.is_none(),
            "CLI alias choice overrides original intent"
        );
        return Ok(());
    }
    let Some(choice) = choice else {
        ensure!(search_path.is_none(), "alias search path without selection");
        return Ok(()); // Historical operation: no implicit alias.
    };
    ensure!(
        !admitted,
        "legacy admitted operation cannot acquire a new alias choice"
    );
    let search_path = match choice {
        CliAliasSelection::None => {
            ensure!(
                search_path.is_none(),
                "no-alias does not accept search path"
            );
            ""
        }
        _ => search_path.context("alias selection requires original PATH")?,
    };
    let mut intent = select(context, choice, search_path)?;
    intent.operation_sha256 = operation_sha256;
    let root = OpenedDirectory::open(
        operation_path
            .parent()
            .context("operation parent missing")?,
    )?;
    private_record::write_private_at(&root, INTENT, &serde_json::to_vec(&intent)?, LIMIT, false)
}

/// Reopen a receipt through its held protected installation parent.
pub(super) fn read_owner(root: &OpenedDirectory, name: &str) -> Result<Option<Vec<u8>>> {
    airk_ipc::endpoint::require_protected_parent_handle(root.as_file())?;
    let file = match root.open_file(OsStr::new(name)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    airk_ipc::endpoint::require_private_metadata_handle(&file)?;
    #[cfg(windows)]
    root.require_default_stream(OsStr::new(name), &file)?;
    let mut bytes = Vec::new();
    file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= LIMIT,
        "alias ownership metadata exceeds bound"
    );
    Ok(Some(bytes))
}
