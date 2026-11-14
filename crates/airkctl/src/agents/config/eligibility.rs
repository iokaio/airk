// SPDX-License-Identifier: Apache-2.0
//! Conservative live Git/npm eligibility. No ignore or package input is changed.
use airk_core::identity::Incarnation;
use airk_process::bounded_command::{Invocation, execute_with_timeout};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) struct Eligibility {
    parent: PathBuf,
    root: PathBuf,
    pub transaction: Incarnation,
    manifests: Vec<(PathBuf, Vec<u8>)>,
}

fn refusal() -> String {
    "Automatic editing cannot safely retain a recovery copy here. Apply the agent configuration edit manually.".into()
}

pub(super) fn git_program(at: &Path) -> Result<PathBuf, String> {
    let paths = std::env::var_os("PATH").ok_or_else(refusal)?;
    resolve_git_path(at, std::env::split_paths(&paths))
}

fn resolve_git_path(at: &Path, search: impl Iterator<Item = PathBuf>) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|_| refusal())?;
    let mut worktrees = Vec::new();
    for start in [at, cwd.as_path()] {
        for directory in start.ancestors().take(65) {
            if fs::symlink_metadata(directory.join(".git")).is_ok() {
                worktrees.push(directory.canonicalize().map_err(|_| refusal())?);
                break;
            }
        }
    }
    for directory in search.take(256) {
        if !directory.is_absolute() {
            continue;
        }
        let candidate = directory.join(if cfg!(windows) { "git.exe" } else { "git" });
        if !candidate.is_file() {
            continue;
        }
        let candidate = candidate.canonicalize().map_err(|_| refusal())?;
        if worktrees.iter().any(|root| candidate.starts_with(root)) {
            continue;
        }
        return Ok(candidate);
    }
    Err(refusal())
}

fn git(at: &Path, args: &[&str], input: Option<Vec<u8>>) -> Result<String, String> {
    if std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with("GIT_")) {
        return Err(refusal());
    }
    let directory = at.to_str().ok_or_else(refusal)?;
    let directory = if cfg!(windows) {
        directory.strip_prefix(r"\\?\").unwrap_or(directory)
    } else {
        directory
    };
    let mut arguments = vec![
        "--no-optional-locks",
        "-c",
        "core.fsmonitor=false",
        "-C",
        directory,
    ];
    arguments.extend_from_slice(args);
    let mut invocation = Invocation::new(git_program(at)?, &arguments);
    invocation.input = input;
    let output =
        execute_with_timeout(&invocation, Duration::from_secs(5)).map_err(|_| refusal())?;
    if !output.success {
        return Err(refusal());
    }
    Ok(output.text)
}

fn literal(path: &str) -> Result<String, String> {
    let path = path.trim_end_matches('/');
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\\', ':', '*', '?', '[', ']', '{', '}', '!', '\0'])
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(refusal());
    }
    Ok(path.to_owned())
}

fn intersects(left: &str, right: &str) -> bool {
    let left = left.to_ascii_lowercase();
    let right = right.to_ascii_lowercase();
    left == right
        || left
            .strip_prefix(right.as_str())
            .is_some_and(|tail| tail.starts_with('/'))
        || right
            .strip_prefix(left.as_str())
            .is_some_and(|tail| tail.starts_with('/'))
}

fn npm_excludes(root: &Path, archive: &Path, bytes: &[u8]) -> Result<(), String> {
    let package: Value = serde_json::from_slice(bytes).map_err(|_| refusal())?;
    if package.get("packageManager").and_then(Value::as_str) != Some("npm@11.12.1") {
        return Err(refusal());
    }
    if [
        "prepack",
        "prepare",
        "postpack",
        "prepublish",
        "prepublishOnly",
        "publish",
        "postpublish",
    ]
    .iter()
    .any(|hook| {
        package
            .get("scripts")
            .and_then(|scripts| scripts.get(*hook))
            .is_some()
    }) {
        return Err(refusal());
    }
    if package.get("bundleDependencies").is_some()
        || package.get("bundledDependencies").is_some()
        || package.get("workspaces").is_some()
        || package.get("publishConfig").is_some()
    {
        return Err(refusal());
    }
    let archive = archive
        .strip_prefix(root)
        .map_err(|_| refusal())?
        .to_str()
        .ok_or_else(refusal)?;
    let archive = if cfg!(windows) {
        archive.replace('\\', "/")
    } else {
        archive.to_owned()
    };
    let archive = literal(&archive)?;
    let files = package["files"]
        .as_array()
        .filter(|files| files.len() <= 256)
        .ok_or_else(refusal)?;
    let mut included = Vec::new();
    for entry in files {
        included.push(literal(entry.as_str().ok_or_else(refusal)?)?);
    }
    if let Some(main) = package.get("main") {
        included.push(literal(main.as_str().ok_or_else(refusal)?)?);
    }
    // The pinned npm-packlist also requires browser entry points even though
    // the summary npm files documentation lists only main/bin.
    if let Some(browser) = package.get("browser") {
        included.push(literal(browser.as_str().ok_or_else(refusal)?)?);
    }
    if package.get("man").is_some() || package.pointer("/directories/man").is_some() {
        return Err(refusal());
    }
    if let Some(bin) = package.get("bin") {
        match bin {
            Value::String(path) => included.push(literal(path)?),
            Value::Object(paths) if paths.len() <= 128 => {
                for path in paths.values() {
                    included.push(literal(path.as_str().ok_or_else(refusal)?)?);
                }
            }
            _ => return Err(refusal()),
        }
    }
    if let Some(path) = package.pointer("/directories/bin") {
        included.push(literal(path.as_str().ok_or_else(refusal)?)?);
    }
    for included in included {
        if intersects(&included, &archive) {
            return Err(refusal());
        }
        let path = root.join(included);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(refusal());
                }
                let canonical = path.canonicalize().map_err(|_| refusal())?;
                if !canonical.starts_with(root)
                    || airk_core::source_policy::retained_agent_edit_path(
                        &canonical.to_string_lossy(),
                    )
                {
                    return Err(refusal());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(refusal()),
        }
    }
    Ok(())
}

fn manifests(parent: &Path, root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    let archive = parent.join(".airk/agent-edits");
    let mut result = Vec::new();
    for (depth, directory) in parent.ancestors().enumerate() {
        if depth > 64 || !directory.starts_with(root) {
            return Err(refusal());
        }
        for unsupported in [
            "Cargo.toml",
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "MANIFEST.in",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
        ] {
            if directory
                .join(unsupported)
                .try_exists()
                .map_err(|_| refusal())?
            {
                return Err(refusal());
            }
        }
        let path = directory.join("package.json");
        match super::io::read_config(&path) {
            Ok(Some(bytes)) if bytes.len() <= 128 * 1024 => {
                npm_excludes(directory, &archive, bytes.as_bytes())?;
                result.push((path, bytes.into_bytes()));
            }
            Ok(None) => (),
            _ => return Err(refusal()),
        }
        if directory == root {
            break;
        }
    }
    if result.is_empty() {
        return Err(refusal());
    }
    Ok(result)
}

impl Eligibility {
    pub fn review(parent: &Path) -> Result<Self, String> {
        let parent = parent.canonicalize().map_err(|_| refusal())?;
        let output = git(&parent, &["rev-parse", "--show-toplevel"], None)?;
        if output.trim().lines().count() != 1 {
            return Err(refusal());
        }
        let root = Path::new(output.trim())
            .canonicalize()
            .map_err(|_| refusal())?;
        if !parent.starts_with(&root) {
            return Err(refusal());
        }
        let transaction = crate::journal::mint().map_err(|_| refusal())?;
        let manifests = manifests(&parent, &root)?;
        let reviewed = Self {
            parent,
            root,
            transaction,
            manifests,
        };
        reviewed.revalidate()?;
        Ok(reviewed)
    }

    pub fn revalidate(&self) -> Result<(), String> {
        if manifests(&self.parent, &self.root)? != self.manifests {
            return Err(refusal());
        }
        let relative = self
            .parent
            .strip_prefix(&self.root)
            .map_err(|_| refusal())?
            .to_str()
            .ok_or_else(refusal)?;
        if !cfg!(windows) && relative.contains('\\') {
            return Err(refusal());
        }
        let relative = if cfg!(windows) {
            relative.replace('\\', "/")
        } else {
            relative.to_owned()
        };
        let base = if relative.is_empty() {
            ".airk/agent-edits".into()
        } else {
            format!("{relative}/.airk/agent-edits")
        };
        let tracked = git(
            &self.root,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--",
                &format!(":(literal){base}"),
            ],
            None,
        )?;
        if !tracked.is_empty() {
            return Err(refusal());
        }
        let paths = BTreeSet::from([
            format!("{base}/v1/owner.json"),
            format!("{base}/v1/{}/receipt.json", self.transaction),
            format!("{base}/v1/{}/prepared", self.transaction),
            format!("{base}/v1/{}/before", self.transaction),
        ]);
        let mut input = Vec::new();
        for path in &paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        let output = git(
            &self.root,
            &["check-ignore", "--no-index", "-z", "--stdin"],
            Some(input),
        )?;
        let observed: BTreeSet<_> = output.split_terminator('\0').map(str::to_owned).collect();
        if !output.ends_with('\0') || observed != paths {
            return Err(refusal());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
