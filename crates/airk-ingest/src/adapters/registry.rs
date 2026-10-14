// SPDX-License-Identifier: Apache-2.0
//! Descriptions of existing adapters only. No scanner dispatch is changed.
use super::profile::Selection;
use airk_core::extraction::{AdapterCapabilities, AdapterDescriptor, ExtractionProfile};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct AdapterRegistry {
    entries: BTreeMap<String, AdapterDescriptor>,
    suffixes: BTreeMap<String, String>,
    compound_suffixes: BTreeMap<String, String>,
    discriminators: Vec<(Vec<u8>, String)>,
}

impl AdapterRegistry {
    pub fn baseline() -> Self {
        let mut registry = Self {
            entries: BTreeMap::new(),
            suffixes: BTreeMap::new(),
            compound_suffixes: BTreeMap::new(),
            discriminators: vec![],
        };
        for (id, grammar, suffixes, imports, calls) in [
            ("rust", "tree-sitter-rust/0.24.2", "rs", true, true),
            ("python", "tree-sitter-python/0.25.0", "py pyi", true, true),
            (
                "javascript",
                "tree-sitter-javascript/0.25.0",
                "js jsx mjs cjs",
                true,
                true,
            ),
            (
                "typescript",
                "tree-sitter-typescript/0.23.2/typescript",
                "ts mts cts",
                true,
                true,
            ),
            (
                "tsx",
                "tree-sitter-typescript/0.23.2/tsx",
                "tsx",
                true,
                true,
            ),
            ("go", "tree-sitter-go/0.25.0", "go", true, true),
            ("java", "tree-sitter-java/0.23.5", "java", true, true),
            ("c", "tree-sitter-c/0.24.2", "c h", true, true),
            (
                "cpp",
                "tree-sitter-cpp/0.23.4",
                "cpp cc cxx hpp hh hxx",
                true,
                true,
            ),
            ("csharp", "tree-sitter-c-sharp/0.23.5", "cs", true, true),
            ("ruby", "tree-sitter-ruby/0.23.1", "rb", false, true),
            ("php", "tree-sitter-php/0.25.1", "php", true, true),
            (
                "bash",
                "tree-sitter-bash/0.25.1",
                "sh bash zsh",
                false,
                true,
            ),
            (
                "powershell",
                "tree-sitter-powershell/0.26.4",
                "ps1 psm1 psd1",
                true,
                true,
            ),
            ("lua", "tree-sitter-lua/0.5.0", "lua", false, true),
            ("swift", "tree-sitter-swift/0.7.4", "swift", true, true),
            (
                "kotlin",
                "tree-sitter-kotlin-ng/1.1.0",
                "kt kts",
                true,
                true,
            ),
            ("sql", "tree-sitter-sequel/0.3.11", "sql", false, false),
            (
                "json",
                "tree-sitter-json/0.24.8",
                "json jsonc",
                false,
                false,
            ),
            ("yaml", "tree-sitter-yaml/0.7.2", "yaml yml", false, false),
            ("toml", "tree-sitter-toml-ng/0.7.0", "toml", false, false),
            (
                "html",
                "tree-sitter-html/0.23.2",
                "html htm vue svelte",
                false,
                false,
            ),
            ("css", "tree-sitter-css/0.25.0", "css scss", false, false),
        ] {
            let structured = matches!(id, "json" | "yaml" | "toml" | "html" | "css");
            registry.add_baseline(
                id,
                Some(grammar),
                suffixes,
                AdapterCapabilities {
                    lexical: true,
                    definitions: !structured,
                    imports,
                    call_candidates: calls,
                    structural_facts: structured,
                    native_locations: true,
                    ..Default::default()
                },
            );
        }
        for (id, suffixes) in [
            ("markdown", "md markdown mdx"),
            ("csv", "csv"),
            ("tsv", "tsv"),
            ("xml", "xml xsd xsl csproj props targets"),
            ("svg", "svg"),
            ("ini", "ini cfg conf properties"),
            ("dockerfile", "dockerfile"),
            ("pdf", "pdf"),
            ("docx", "docx"),
            ("xlsx", "xlsx"),
            ("pptx", "pptx"),
            ("text", ""),
        ] {
            registry.add_baseline(
                id,
                None,
                suffixes,
                AdapterCapabilities {
                    lexical: true,
                    structural_facts: true,
                    native_locations: true,
                    ..Default::default()
                },
            );
        }
        registry
    }

    fn add_baseline(
        &mut self,
        id: &str,
        grammar: Option<&str>,
        suffixes: &str,
        capabilities: AdapterCapabilities,
    ) {
        self.entries.insert(id.into(), AdapterDescriptor {
            profile: ExtractionProfile { contract_version: 1, adapter_id: id.into(), adapter_version: "native-facts/7".into(), language: id.into(), dialect: None, grammar_identity: grammar.map(|g| format!("tree-sitter/0.25.10:{g}")), mapping_version: 1, chunk_version: 1, options: BTreeMap::new(), configuration_inputs: BTreeMap::new() },
            capabilities,
            limitations: vec!["Existing bounded extraction only; no resolved references, embedded mapping or continuity qualification".into()],
        });
        for suffix in suffixes.split_whitespace() {
            self.suffixes.insert(suffix.into(), id.into());
        }
    }

    pub fn get(&self, id: &str) -> Option<&AdapterDescriptor> {
        self.entries.get(id)
    }
    pub fn descriptors(&self) -> impl Iterator<Item = &AdapterDescriptor> {
        self.entries.values()
    }

    /// Explicit static registration; never fetches schemas or executes code.
    pub fn register_compound_suffix(&mut self, suffix: &str, adapter: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.entries.contains_key(adapter)
                && suffix.starts_with('.')
                && suffix.len() <= 256
                && !suffix.contains(['/', '\\', '\0', ':']),
            "invalid compound registration"
        );
        anyhow::ensure!(
            self.compound_suffixes.len() < 256 && !self.compound_suffixes.contains_key(suffix),
            "duplicate or excessive compound registrations"
        );
        self.compound_suffixes.insert(suffix.into(), adapter.into());
        Ok(())
    }

    /// Case-sensitive literal prefix, bounded and used only after suffix rules.
    pub fn register_discriminator(&mut self, prefix: &[u8], adapter: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.entries.contains_key(adapter)
                && !prefix.is_empty()
                && prefix.len() <= 4096
                && self.discriminators.len() < 256,
            "invalid content discriminator"
        );
        anyhow::ensure!(
            !self.discriminators.iter().any(|(p, _)| p == prefix),
            "duplicate content discriminator"
        );
        self.discriminators.push((prefix.to_vec(), adapter.into()));
        Ok(())
    }

    pub(super) fn select_with_prefix(
        &self,
        path: &str,
        source: &[u8],
    ) -> anyhow::Result<Selection> {
        let mut selected = self.select_default(path);
        if selected.basis == "known_basename" {
            return Ok(selected);
        }
        if let Some((_, adapter)) = self
            .compound_suffixes
            .iter()
            .filter(|(suffix, _)| path.ends_with(suffix.as_str()))
            .max_by_key(|(suffix, _)| suffix.len())
        {
            selected.profile = self.entries[adapter].profile.clone();
            selected.basis = "compound_suffix".into();
            return Ok(selected);
        }
        if selected.basis != "utf8_fallback" {
            return Ok(selected);
        }
        let matches: Vec<_> = self
            .discriminators
            .iter()
            .filter(|(prefix, _)| source.starts_with(prefix))
            .collect();
        anyhow::ensure!(matches.len() <= 1, "ambiguous content discriminators");
        if let Some((_, adapter)) = matches.first() {
            selected.profile = self.entries[adapter].profile.clone();
            selected.basis = "bounded_content_prefix".into();
        }
        Ok(selected)
    }

    pub(super) fn select_default(&self, path: &str) -> Selection {
        let basename = path.rsplit('/').next().unwrap_or(path);
        let suffix = basename.rsplit_once('.').map_or("", |(_, s)| s);
        let lower_suffix = suffix.to_ascii_lowercase();
        let (id, basis) =
            if basename.eq_ignore_ascii_case("Dockerfile") || basename.starts_with("Dockerfile.") {
                ("dockerfile", "known_basename")
            } else if let Some(id) = self.suffixes.get(&lower_suffix) {
                (id.as_str(), "suffix")
            } else {
                ("text", "utf8_fallback")
            };
        let ambiguity = match suffix {
            "h" | "H" => vec!["Header defaults to C; C++ context is not inferred".into()],
            "sql" | "SQL" => vec!["SQL dialect unselected; only existing CREATE subset".into()],
            "m" | "pl" | "cls" | "v" | "s" | "S" => vec![format!(
                "Ambiguous suffix .{suffix}; no qualified default language"
            )],
            _ => vec![],
        };
        Selection {
            profile: self.entries[id].profile.clone(),
            basis: basis.into(),
            ambiguity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_additions_do_not_change_existing_profile_identity() -> anyhow::Result<()> {
        let mut registry = AdapterRegistry::baseline();
        let old = registry.select_default("src/a.rs").profile.identity()?;
        registry.add_baseline("test-only", None, "fixture", AdapterCapabilities::default());
        assert_eq!(old, registry.select_default("src/a.rs").profile.identity()?);
        registry
            .entries
            .get_mut("rust")
            .unwrap()
            .profile
            .options
            .insert("mode".into(), "changed".into());
        assert_ne!(old, registry.select_default("src/a.rs").profile.identity()?);
        Ok(())
    }
    #[test]
    fn partial_profiles_are_not_new_language_support() {
        let r = AdapterRegistry::baseline();
        for (path, language) in [
            ("x.vue", "html"),
            ("x.scss", "css"),
            ("build.gradle.kts", "kotlin"),
            ("x.mdx", "markdown"),
            ("x.fsproj", "text"),
            ("Dockerfile.dev", "dockerfile"),
        ] {
            assert_eq!(r.select_default(path).profile.language, language);
        }
        assert_eq!(r.select_default("x.S").profile.language, "text");
        assert!(!r.select_default("x.S").ambiguity.is_empty());
    }

    #[test]
    fn compounds_precede_suffixes_and_content_conflicts_fail() -> anyhow::Result<()> {
        let mut registry = AdapterRegistry::baseline();
        registry.register_compound_suffix(".special.rs", "text")?;
        assert_eq!(
            registry
                .select_with_prefix("a.special.rs", b"")?
                .profile
                .language,
            "text"
        );
        registry.register_discriminator(b"#!example", "bash")?;
        assert_eq!(
            registry
                .select_with_prefix("a.unknown", b"#!example\n")?
                .profile
                .language,
            "bash"
        );
        assert_eq!(
            registry
                .select_with_prefix("a.rs", b"#!example\n")?
                .profile
                .language,
            "rust"
        );
        registry.register_discriminator(b"#!", "python")?;
        assert!(
            registry
                .select_with_prefix("a.unknown", b"#!example\n")
                .is_err()
        );
        Ok(())
    }
}
