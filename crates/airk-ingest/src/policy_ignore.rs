// SPDX-License-Identifier: Apache-2.0
//! Ignore rules captured through the same protected source reader as documents.
//! The ignore crate compiles/matches supplied lines, but opens no policy inputs.
use crate::native_name_boundary::NativeNameBoundaryUnavailable;
use airk_core::source_policy::SourcePolicy;
use anyhow::{Context, Result, ensure};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) const MAX_FILE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_CONTROL_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_DIRECTORIES: usize = 32768;

fn refused(reason: &'static str) -> anyhow::Error {
    NativeNameBoundaryUnavailable::new(reason).into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    None,
    Ignore,
    Whitelist,
}

pub(crate) struct ProtectedIgnore {
    root: PathBuf,
    policy: Option<SourcePolicy>,
    charged: usize,
    directories: BTreeMap<PathBuf, [Gitignore; 4]>,
    failure: Option<&'static str>,
    global: Gitignore,
    root_blocked: bool,
    controls: Option<std::sync::Arc<crate::control_snapshot::ControlSnapshot>>,
}

impl ProtectedIgnore {
    pub(crate) fn new(root: &Path, policy: &SourcePolicy) -> Result<Self> {
        let mut selected = Self::local(root, policy)?;
        if !selected.root_blocked {
            selected.global = selected.global_rules()?;
        }
        Ok(selected)
    }

    pub(crate) fn tracked(
        root: &Path,
        policy: Option<&SourcePolicy>,
        controls: std::sync::Arc<crate::control_snapshot::ControlSnapshot>,
    ) -> Result<Self> {
        let mut selected = Self::local_with_controls(root, policy, Some(controls))?;
        if !selected.root_blocked {
            selected.global = selected.global_rules()?;
        }
        Ok(selected)
    }
    fn local(root: &Path, policy: &SourcePolicy) -> Result<Self> {
        Self::local_with_controls(root, Some(policy), None)
    }
    fn local_with_controls(
        root: &Path,
        policy: Option<&SourcePolicy>,
        controls: Option<std::sync::Arc<crate::control_snapshot::ControlSnapshot>>,
    ) -> Result<Self> {
        let root = root.to_path_buf();
        let mut selected = Self {
            global: Gitignore::empty(),
            root,
            policy: policy.cloned(),
            charged: 0,
            directories: BTreeMap::new(),
            failure: None,
            root_blocked: false,
            controls,
        };
        if let Some(controls) = &selected.controls {
            controls.record_environment(&selected.root)?;
        }
        let root_text = selected
            .root
            .to_str()
            .ok_or_else(|| refused("ignore_control_locator"))?;
        selected.root_blocked = policy
            .map(|policy| crate::permanent_policy::allows_checked(policy, root_text, "", true))
            .transpose()?
            .is_some_and(|allowed| !allowed);
        if selected.root_blocked {
            return Ok(selected);
        }
        if let Some(policy) = policy {
            crate::permanent_policy::qualify_discovery_directory(policy, root_text, "")?;
        } else {
            crate::native_name_boundary::qualify(&selected.root, "", true)?;
        }
        // The explicit walk root has its own controls even when no child is
        // eligible. Ancestor controls retain their existing precedence.
        let ancestors = selected
            .root
            .ancestors()
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        ensure!(ancestors.len() <= 256, refused("ignore_control_depth"));
        for parent in ancestors.into_iter().rev() {
            selected.directory(&parent)?;
        }
        Ok(selected)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn capture(&mut self, path: &Path) -> Result<Option<String>> {
        let path = std::path::absolute(path).map_err(|_| refused("ignore_control_locator"))?;
        if self.controls.is_some() {
            crate::control_snapshot::qualify_locator(&path, false)?;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(controls) = &self.controls {
                    controls.absent_qualified(&path)?;
                }
                return Ok(None);
            }
            Err(error) => {
                return Err(refused("ignore_control_metadata")).with_context(|| {
                    format!(
                        "ignore control metadata {}: {error} (native {:?})",
                        path.display(),
                        error.raw_os_error()
                    )
                });
            }
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            refused("aliased_ignore_control")
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                refused("aliased_ignore_control")
            );
        }
        ensure!(
            self.policy
                .as_ref()
                .map(
                    |policy| crate::permanent_policy::allows_absolute_control_input_checked(
                        policy, &path
                    )
                )
                .transpose()?
                .unwrap_or(true),
            refused("excluded_ignore_control")
        );
        let length =
            usize::try_from(metadata.len()).map_err(|_| refused("ignore_control_budget"))?;
        let allowance = length
            .checked_add(1)
            .ok_or_else(|| refused("ignore_control_budget"))?;
        ensure!(
            length <= MAX_FILE_BYTES && allowance <= MAX_CONTROL_BYTES.saturating_sub(self.charged),
            refused("ignore_control_budget")
        );
        // A failed capture still consumes its allowance. The reader never follows
        // an input/ancestor link and verifies held identity and pathname afterward.
        self.charged += allowance;
        let bytes = crate::source_read::read(
            path.parent()
                .ok_or_else(|| refused("ignore_control_locator"))?,
            &path,
            MAX_FILE_BYTES,
            allowance,
        )
        .map_err(|_| refused("ignore_control_unavailable"))?;
        ensure!(
            bytes.len() <= MAX_FILE_BYTES,
            refused("ignore_control_budget")
        );
        if let Some(controls) = &self.controls {
            controls.file(&path, &bytes)?;
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| refused("ignore_control_encoding"))
    }

    fn compile(&mut self, base: &Path, path: &Path) -> Result<Gitignore> {
        let mut builder = GitignoreBuilder::new(base);
        if let Some(text) = self.capture(path)? {
            for (number, line) in text.lines().enumerate() {
                let line = if number == 0 {
                    line.strip_prefix('\u{feff}').unwrap_or(line)
                } else {
                    line
                };
                builder
                    .add_line(Some(path.to_path_buf()), line)
                    .map_err(|_| refused("ignore_control_pattern"))?;
            }
        }
        builder
            .build()
            .map_err(|_| refused("ignore_control_pattern"))
    }

    fn directory(&mut self, path: &Path) -> Result<()> {
        if let Some(reason) = self.failure {
            return Err(refused(reason));
        }
        let result = self.load_directory(path);
        if let Err(error) = &result {
            // A partial directory snapshot cannot be retried by another source
            // reference. Only a fresh matcher may observe repaired controls.
            self.failure = Some(
                error
                    .downcast_ref::<NativeNameBoundaryUnavailable>()
                    .map_or(
                        "ignore_control_unavailable",
                        NativeNameBoundaryUnavailable::reason,
                    ),
            );
        }
        result
    }

    fn load_directory(&mut self, path: &Path) -> Result<()> {
        if self.directories.contains_key(path) {
            return Ok(());
        }
        ensure!(
            self.directories.len() < MAX_DIRECTORIES,
            refused("ignore_control_budget")
        );
        // Worktree/commondir indirection needs a separately qualified protected
        // resolver. Never let the dependency follow an unreviewed control path.
        let git = path.join(".git");
        if self.controls.is_some() {
            crate::control_snapshot::qualify_locator(&git, true)?;
        }
        match fs::symlink_metadata(&git) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_dir() && !metadata.file_type().is_symlink(),
                    refused("ignore_control_git_reference")
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        metadata.file_attributes() & 0x400 == 0,
                        refused("ignore_control_git_reference")
                    );
                }
                if let Some(controls) = &self.controls {
                    controls.directory_qualified(&git)?;
                }
                ensure!(
                    !git.join("commondir").try_exists().map_err(|error| refused(
                        "ignore_control_metadata"
                    )
                    .context(format!(
                        "ignore commondir metadata {}: {error} (native {:?})",
                        git.join("commondir").display(),
                        error.raw_os_error()
                    )))?,
                    refused("ignore_control_git_reference")
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(controls) = &self.controls {
                    controls.absent_qualified(&git)?;
                }
            }
            Err(error) => {
                return Err(refused("ignore_control_metadata")).with_context(|| {
                    format!(
                        "ignore directory metadata {}: {error} (native {:?})",
                        git.display(),
                        error.raw_os_error()
                    )
                });
            }
        }
        if let Some(controls) = self.controls.clone() {
            let commondir = git.join("commondir");
            crate::control_snapshot::qualify_locator(&commondir, false)?;
            ensure!(
                matches!(fs::symlink_metadata(&commondir),Err(error) if error.kind()==std::io::ErrorKind::NotFound),
                refused("ignore_control_git_reference")
            );
            controls.absent_qualified(&commondir)?;
        }
        let rules = [
            self.compile(path, &path.join(".airkignore"))?,
            self.compile(path, &path.join(".ignore"))?,
            self.compile(path, &path.join(".gitignore"))?,
            self.compile(path, &git.join("info").join("exclude"))?,
        ];
        self.directories.insert(path.to_path_buf(), rules);
        Ok(())
    }

    pub(crate) fn decision(&mut self, relative: &Path, is_dir: bool) -> Result<Decision> {
        if self.root_blocked {
            return Ok(Decision::Ignore);
        }
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
            refused("ignore_control_locator")
        );
        let path = self.root.join(relative);
        let parents = path
            .parent()
            .ok_or_else(|| refused("ignore_control_locator"))?
            .ancestors()
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        ensure!(parents.len() <= 256, refused("ignore_control_depth"));
        for parent in parents.iter().rev() {
            self.directory(parent)?;
        }
        // AIRK custom rules outrank .ignore, which outranks Git rules. Within a
        // family the nearest directory wins, including explicit negation.
        for family in 0..4 {
            for parent in &parents {
                match self.directories[parent][family].matched(&path, is_dir) {
                    ignore::Match::Ignore(_) => return Ok(Decision::Ignore),
                    ignore::Match::Whitelist(_) => return Ok(Decision::Whitelist),
                    ignore::Match::None => (),
                }
            }
        }
        Ok(match self.global.matched(&path, is_dir) {
            ignore::Match::Ignore(_) => Decision::Ignore,
            ignore::Match::Whitelist(_) => Decision::Whitelist,
            ignore::Match::None => Decision::None,
        })
    }

    fn global_rules(&mut self) -> Result<Gitignore> {
        // WalkBuilder anchors global rules to its current working directory,
        // independently of each selected source root. If CWD is unavailable,
        // the ordinary adapter skips global controls entirely.
        let Ok(base) = std::env::current_dir() else {
            return Ok(Gitignore::empty());
        };
        let home = home();
        let xdg = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|p| p.join(".config")));
        let mut candidates = Vec::new();
        if let Some(path) = std::env::var_os("GIT_CONFIG_GLOBAL").filter(|v| !v.is_empty()) {
            candidates.push(PathBuf::from(path));
        }
        if let Some(home) = &home {
            candidates.push(home.join(".gitconfig"));
        }
        if let Some(xdg) = &xdg {
            candidates.push(xdg.join("git").join("config"));
        }
        candidates.push(
            std::env::var_os("GIT_CONFIG_SYSTEM")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/etc/gitconfig")),
        );
        let fallback = xdg.map(|xdg| xdg.join("git").join("ignore"));
        self.global_rules_from(&base, &candidates, fallback.as_deref(), home.as_deref())
    }

    fn global_rules_from(
        &mut self,
        base: &Path,
        candidates: &[PathBuf],
        fallback: Option<&Path>,
        home: Option<&Path>,
    ) -> Result<Gitignore> {
        for config in candidates {
            if let Some(text) = self.capture(config)?
                && let Some(path) = excludes_path(&text, home)?
            {
                return self.compile(base, &path);
            }
        }
        match fallback {
            Some(path) => self.compile(base, path),
            None => Ok(Gitignore::empty()),
        }
    }
}

#[allow(deprecated)]
fn home() -> Option<PathBuf> {
    std::env::home_dir()
}

/// Qualify the dependency's ordinary single-token excludes-file convention.
/// Complex/ambiguous values refuse instead of changing ignore semantics silently.
fn excludes_path(text: &str, home: Option<&Path>) -> Result<Option<PathBuf>> {
    for line in text.lines() {
        let Some((key, raw)) = line.trim().split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("excludesfile") {
            continue;
        }
        let raw = raw.trim();
        let value = match raw.strip_prefix('"') {
            Some(value) => value
                .strip_suffix('"')
                .ok_or_else(|| refused("ignore_control_config"))?
                .trim(),
            None => raw,
        };
        ensure!(
            !value.is_empty()
                && value.len() <= 4096
                && !value
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control() || c == '"'),
            refused("ignore_control_config")
        );
        let value = if value.contains('~') {
            let home = home
                .and_then(Path::to_str)
                .ok_or_else(|| refused("ignore_control_config"))?;
            value.replace('~', home)
        } else {
            value.to_owned()
        };
        return Ok(Some(PathBuf::from(value)));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::{Incarnation, Sha256Digest},
        index_catalog::{CatalogRevision, DenyId, IndexId, PathScope, SourceRootId},
        source_policy::{DenyReason, DenyRule, POLICY_SCHEMA, SourceRoot},
    };

    fn fixture() -> Result<(tempfile::TempDir, SourcePolicy)> {
        let owned = tempfile::tempdir()?;
        let root = owned.path().canonicalize()?;
        let id = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
        let policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: IndexId::from_identity(Incarnation::from_bytes([2; 16])),
            owner_binding: Sha256Digest::from_bytes([3; 32]),
            namespace: "collections-1.4".into(),
            collection: "fixture".into(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot {
                id,
                canonical_path: root.to_str().unwrap().into(),
                aliases: vec![],
                owner: true,
            }],
            positive_includes: None,
            denies: vec![DenyRule {
                source_root_id: id,
                kind: PathScope::Subtree,
                path: "private".into(),
                rule_id: DenyId::from_identity(Incarnation::from_bytes([4; 16])),
                reason: DenyReason::Trim,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: None,
            }],
        };
        Ok((owned, policy))
    }

    #[cfg(windows)]
    #[test]
    fn native_verbatim_ignore_controls_use_native_components_and_preserve_source_bytes()
    -> Result<()> {
        let (_owned, policy) = fixture()?;
        let root = Path::new(&policy.roots[0].canonical_path);
        ensure!(
            root.to_string_lossy().starts_with(r"\\?\"),
            "fixture must exercise actual canonical Windows verbatim naming"
        );
        let info = root.join(".git").join("info");
        fs::create_dir_all(&info)?;
        let control = info.join("exclude");
        let rules = b"excluded.md\n";
        fs::write(&control, rules)?;
        let source = root.join("excluded.md");
        let bytes = b"original read-only source evidence\n";
        fs::write(&source, bytes)?;
        let mut selected = ProtectedIgnore::local(root, &policy)?;
        assert!(matches!(
            selected.decision(Path::new("excluded.md"), false)?,
            Decision::Ignore
        ));
        assert!(matches!(
            selected.decision(Path::new("eligible.md"), false)?,
            Decision::None
        ));
        assert_eq!(fs::read(&source)?, bytes);
        assert_eq!(fs::read(&control)?, rules);
        Ok(())
    }

    #[test]
    fn protected_patterns_preserve_native_family_and_nested_negation_precedence() -> Result<()> {
        let (_owned, policy) = fixture()?;
        let root = Path::new(&policy.roots[0].canonical_path);
        fs::create_dir(root.join("nested"))?;
        fs::write(root.join(".gitignore"), "*.secret\nnested/drop.txt\n")?;
        fs::write(root.join(".ignore"), "*.tmp\n")?;
        fs::write(root.join(".airkignore"), "!permit.secret\n")?;
        fs::write(root.join("nested/.gitignore"), "local.txt\n")?;
        fs::write(root.join("nested/.airkignore"), "!keep.tmp\n")?;
        let mut protected = ProtectedIgnore::local(root, &policy)?;
        let mut ordinary = crate::source_scope::builder(root);
        ordinary.git_global(false);
        let mut ordinary = ordinary.build_matchers().remove(0);
        for (path, expected) in [
            ("a.secret", false),
            ("permit.secret", true),
            ("nested/drop.txt", false),
            ("nested/local.txt", false),
            ("nested/no.tmp", false),
            ("nested/keep.tmp", true),
            ("good.txt", true),
        ] {
            let (baseline, error) = ordinary.matched_with_errors(path, false);
            assert!(error.is_none());
            assert_eq!(!baseline.is_ignore(), expected, "baseline {path}");
            assert_eq!(
                protected.decision(Path::new(path), false)? != Decision::Ignore,
                expected,
                "protected {path}"
            );
        }
        Ok(())
    }

    #[test]
    fn configured_global_excludes_cannot_capture_a_permanently_denied_control() -> Result<()> {
        let (_owned, policy) = fixture()?;
        let root = Path::new(&policy.roots[0].canonical_path);
        fs::create_dir(root.join("private"))?;
        let hidden = root.join("private/global.ignore");
        fs::write(&hidden, "public.txt\n")?;
        let config = root.join("global.config");
        fs::write(
            &config,
            format!(
                "[core]\nexcludesFile = \"{}\"\n",
                hidden.to_string_lossy().replace('\\', "/")
            ),
        )?;
        let mut selected = ProtectedIgnore::local(root, &policy)?;
        let error = selected
            .global_rules_from(root, &[config], None, None)
            .expect_err("denied global control must refuse");
        assert!(format!("{error:#}").contains("excluded_ignore_control"));
        Ok(())
    }

    #[test]
    fn global_patterns_keep_the_ordinary_cwd_base_for_configured_and_default_controls() -> Result<()>
    {
        let (owned, mut policy) = fixture()?;
        let base = owned.path().canonicalize()?.join("host");
        let root = base.join("project");
        fs::create_dir_all(&root)?;
        policy.roots[0].canonical_path = root.to_str().unwrap().into();
        let control = owned.path().canonicalize()?.join("global.ignore");
        let config = owned.path().canonicalize()?.join("global.config");
        fs::write(
            &config,
            format!(
                "[core]\nexcludesFile=\"{}\"\n",
                control.to_string_lossy().replace('\\', "/")
            ),
        )?;
        for (pattern, relative, directory, expected) in [
            ("/generated/", "generated", true, false),
            ("/project/generated/", "generated", true, true),
            ("project/build.txt", "build.txt", false, true),
            ("*.tmp", "scratch.tmp", false, true),
        ] {
            fs::write(&control, format!("{pattern}\n"))?;
            // This is the dependency's native matcher/base used by
            // WalkBuilder::build_ignore, without mutating process CWD or env.
            let mut baseline = GitignoreBuilder::new(&base);
            assert!(baseline.add(&control).is_none());
            let baseline = baseline.build()?;
            assert_eq!(
                baseline.matched(root.join(relative), directory).is_ignore(),
                expected,
                "ordinary {pattern}"
            );
            for configured in [false, true] {
                let mut protected = ProtectedIgnore::local(&root, &policy)?;
                protected.global = if configured {
                    protected.global_rules_from(&base, std::slice::from_ref(&config), None, None)?
                } else {
                    protected.global_rules_from(&base, &[], Some(&control), None)?
                };
                assert_eq!(
                    protected.decision(Path::new(relative), directory)? == Decision::Ignore,
                    expected,
                    "protected {pattern}, configured={configured}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn nested_control_failure_is_latched_until_a_fresh_snapshot() -> Result<()> {
        let (_owned, policy) = fixture()?;
        let root = Path::new(&policy.roots[0].canonical_path);
        let nested = root.join("nested");
        fs::create_dir(&nested)?;
        fs::write(nested.join(".airkignore"), "# capture before failure\n")?;
        fs::write(nested.join(".ignore"), [0xff])?;
        let mut protected = ProtectedIgnore::local(root, &policy)?;
        let before = protected.charged;
        let error = protected
            .decision(Path::new("nested/a.txt"), false)
            .expect_err("invalid nested control must refuse");
        assert_eq!(
            error
                .downcast_ref::<NativeNameBoundaryUnavailable>()
                .unwrap()
                .reason(),
            "ignore_control_encoding"
        );
        let after = protected.charged;
        assert!(
            after > before,
            "the first attempt must capture nested controls"
        );
        fs::write(nested.join(".ignore"), "b.txt\n")?;
        for relative in ["nested/a.txt", "nested/b.txt", "public.txt"] {
            let error = protected
                .decision(Path::new(relative), false)
                .expect_err("a failed snapshot cannot recover in place");
            assert_eq!(
                error
                    .downcast_ref::<NativeNameBoundaryUnavailable>()
                    .unwrap()
                    .reason(),
                "ignore_control_encoding"
            );
            assert_eq!(protected.charged, after, "controls were captured again");
        }
        let mut fresh = ProtectedIgnore::local(root, &policy)?;
        assert_eq!(
            fresh.decision(Path::new("nested/a.txt"), false)?,
            Decision::None
        );
        assert_eq!(
            fresh.decision(Path::new("nested/b.txt"), false)?,
            Decision::Ignore
        );
        Ok(())
    }

    #[test]
    fn control_budget_refuses_before_loading_oversized_rules() -> Result<()> {
        let (_owned, policy) = fixture()?;
        let root = Path::new(&policy.roots[0].canonical_path);
        fs::write(root.join(".airkignore"), vec![b'a'; MAX_FILE_BYTES + 1])?;
        let error = ProtectedIgnore::local(root, &policy)
            .err()
            .expect("oversized control must refuse");
        assert!(format!("{error:#}").contains("ignore_control_budget"));
        Ok(())
    }

    #[test]
    fn positive_partition_controls_do_not_expand_document_or_asset_eligibility() -> Result<()> {
        use airk_core::index_catalog::ScopeAtom;
        let (_owned, mut policy) = fixture()?;
        let root = PathBuf::from(&policy.roots[0].canonical_path);
        fs::create_dir_all(root.join("docs/nested"))?;
        fs::write(root.join(".airkignore"), "*.secret\n")?;
        fs::write(root.join("docs/.ignore"), "omit.md\n")?;
        policy.positive_includes = Some(vec![ScopeAtom {
            source_root_id: policy.roots[0].id,
            kind: PathScope::Subtree,
            path: "docs".into(),
        }]);
        let mut protected = ProtectedIgnore::local(&root, &policy)?;
        assert!(protected.charged > 0);
        assert_eq!(
            protected.decision(Path::new("docs/item.secret"), false)?,
            Decision::Ignore
        );
        assert_eq!(
            protected.decision(Path::new("docs/omit.md"), false)?,
            Decision::Ignore
        );
        assert_eq!(
            protected.decision(Path::new("docs/nested/keep.md"), false)?,
            Decision::None
        );
        assert!(!crate::permanent_policy::allows_checked(
            &policy,
            root.to_str().unwrap(),
            ".airkignore",
            false
        )?);
        assert!(!crate::permanent_policy::allows_absolute_input_checked(
            &policy,
            &root.join(".airkignore")
        )?);
        assert!(!crate::permanent_policy::allows_absolute_input_checked(
            &policy,
            &root.join("unselected/model.bin")
        )?);
        policy.denies[0].path = ".airkignore".into();
        policy.denies[0].kind = PathScope::Exact;
        let error = ProtectedIgnore::local(&root, &policy)
            .err()
            .expect("deny must also reject positive partition controls");
        assert!(format!("{error:#}").contains("excluded_ignore_control"));
        assert_eq!(fs::read_to_string(root.join(".airkignore"))?, "*.secret\n");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn positive_partition_control_alias_remains_unavailable() -> Result<()> {
        use airk_core::index_catalog::ScopeAtom;
        let (_owned, mut policy) = fixture()?;
        let root = PathBuf::from(&policy.roots[0].canonical_path);
        fs::create_dir(root.join("docs"))?;
        fs::write(root.join("unselected-rules"), "keep.md\n")?;
        std::os::unix::fs::symlink("unselected-rules", root.join(".airkignore"))?;
        policy.positive_includes = Some(vec![ScopeAtom {
            source_root_id: policy.roots[0].id,
            kind: PathScope::Exact,
            path: "docs/keep.md".into(),
        }]);
        let error = ProtectedIgnore::local(&root, &policy)
            .err()
            .expect("control dependencies must never follow a link");
        assert!(format!("{error:#}").contains("aliased_ignore_control"));
        assert_eq!(
            fs::read_to_string(root.join("unselected-rules"))?,
            "keep.md\n"
        );
        Ok(())
    }
}
