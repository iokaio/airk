// SPDX-License-Identifier: Apache-2.0
//! Versioned native adapter capabilities and deterministic default selection.
use super::profile::Selection;
use airk_core::extraction::{AdapterCapabilities, AdapterDescriptor, ExtractionProfile};
use std::collections::BTreeMap;

fn objective_c_prefix(source: &[u8]) -> bool {
    // Selection examines at most 4 KiB and ignores comments/string contents.
    // This is a strong filename discriminator, not Objective-C extraction.
    let source = &source[..source.len().min(4096)];
    let mut visible = source.to_vec();
    let mut i = 0;
    while i < source.len() {
        let start = i;
        if source[i..].starts_with(b"/*") {
            i += 2;
            while i < source.len() && !source[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(source.len());
        } else if source[i..].starts_with(b"//") {
            while i < source.len() && source[i] != b'\n' {
                i += 1;
            }
        } else if matches!(source[i], b'\'' | b'"') {
            let quote = source[i];
            i += 1;
            while i < source.len() {
                if source[i] == b'\\' {
                    i = (i + 2).min(source.len());
                } else if source[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else {
            i += 1;
            continue;
        }
        for byte in &mut visible[start..i] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    visible.split(|byte| *byte == b'\n').any(|line| {
        let line = line.trim_ascii_start();
        [
            b"@interface ".as_slice(),
            b"@implementation ",
            b"@protocol ",
            b"#import ",
        ]
        .iter()
        .any(|marker| line.starts_with(marker))
    })
}

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
            ("dart", "tree-sitter-dart/0.2.0", "dart", true, true),
            ("r", "tree-sitter-r/1.3.0", "r", true, true),
            ("ocaml", "tree-sitter-ocaml/0.26.0/ocaml", "ml", true, true),
            (
                "ocaml-interface",
                "tree-sitter-ocaml/0.26.0/interface",
                "mli",
                true,
                true,
            ),
            ("haskell", "tree-sitter-haskell/0.24.1", "hs", true, true),
            (
                "haskell-literate",
                "tree-sitter-haskell/0.24.1",
                "lhs",
                true,
                true,
            ),
            ("erlang", "tree-sitter-erlang/0.21.0", "erl hrl", true, true),
            (
                "fsharp",
                "tree-sitter-fsharp/0.3.12/fsharp",
                "fs fsx",
                true,
                true,
            ),
            (
                "fsharp-signature",
                "tree-sitter-fsharp/0.3.12/signature",
                "fsi",
                true,
                true,
            ),
            ("nix", "tree-sitter-nix/0.3.0", "nix", true, true),
            ("zig", "tree-sitter-zig/1.1.2", "zig", true, true),
            ("julia", "tree-sitter-julia/0.23.1", "jl", true, true),
            (
                "clojure",
                "tree-sitter-clojure/0.1.0",
                "clj cljs cljc",
                true,
                true,
            ),
            ("edn", "tree-sitter-clojure/0.1.0", "edn", false, false),
            ("perl", "ts-parser-perl/2.0.0", "", true, true),
            (
                "groovy",
                "dekobon-tree-sitter-groovy/0.3.0",
                "groovy",
                true,
                true,
            ),
            (
                "gradle-groovy",
                "dekobon-tree-sitter-groovy/0.3.0",
                "gradle",
                true,
                true,
            ),
            (
                "rmd",
                "pulldown-cmark/0.10+tree-sitter-r/1.3.0",
                "rmd",
                true,
                true,
            ),
            ("scala", "tree-sitter-scala/0.25.1", "scala sc", true, true),
            ("elixir", "tree-sitter-elixir/0.3.5", "ex exs", true, true),
            ("objc", "tree-sitter-objc/3.0.2", "", true, true),
            (
                "objcpp",
                "tree-sitter-objc/3.0.2+tree-sitter-cpp/0.23.4",
                "mm",
                true,
                true,
            ),
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
                "sql-postgresql",
                "tree-sitter-sequel/0.3.11",
                "",
                false,
                false,
            ),
            ("sql-mysql", "tree-sitter-sequel/0.3.11", "", false, false),
            ("starlark", "tree-sitter-python/0.25.0", "bzl", true, false),
            (
                "gradle-kotlin",
                "tree-sitter-kotlin-ng/1.1.0",
                "",
                true,
                true,
            ),
            ("hcl", "tree-sitter-hcl/1.1.0", "hcl tf tfvars", true, false),
            (
                "dockerfile",
                "tree-sitter-dockerfile-updated/0.2.0",
                "dockerfile",
                true,
                false,
            ),
            ("proto", "tree-sitter-proto/0.4.0", "proto", true, false),
            (
                "graphql",
                "tree-sitter-graphql/0.1.0",
                "graphql gql",
                false,
                false,
            ),
            ("cmake", "tree-sitter-cmake/0.7.1", "cmake", false, false),
            ("make", "tree-sitter-make/1.1.1", "mk", false, false),
            (
                "json",
                "tree-sitter-json/0.24.8",
                "json jsonc",
                false,
                false,
            ),
            ("yaml", "tree-sitter-yaml/0.7.2", "yaml yml", false, false),
            ("toml", "tree-sitter-toml-ng/0.7.0", "toml", false, false),
            ("html", "tree-sitter-html/0.23.2", "html htm", false, false),
            ("css", "tree-sitter-css/0.25.0", "css", false, false),
            (
                "scss",
                "cortexkit-tree-sitter-scss/1.0.1",
                "scss",
                true,
                false,
            ),
            ("vue", "tree-sitter-html/0.23.2", "vue", false, false),
            ("svelte", "tree-sitter-html/0.23.2", "svelte", false, false),
            ("astro", "tree-sitter-html/0.23.2", "astro", true, false),
            (
                "mdx",
                "tree-sitter-typescript/0.23.2/tsx",
                "mdx",
                true,
                false,
            ),
            (
                "razor",
                "tree-sitter-razor/0.1.0",
                "razor cshtml",
                false,
                false,
            ),
            ("notebook", "tree-sitter-json/0.24.8", "ipynb", false, false),
        ] {
            let structured = matches!(
                id,
                "clojure"
                    | "edn"
                    | "elixir"
                    | "json"
                    | "yaml"
                    | "toml"
                    | "html"
                    | "css"
                    | "cmake"
                    | "make"
                    | "starlark"
                    | "scss"
                    | "vue"
                    | "svelte"
                    | "astro"
                    | "mdx"
                    | "razor"
                    | "notebook"
            );
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
            ("markdown", "md markdown"),
            ("csv", "csv"),
            ("tsv", "tsv"),
            ("xml", "xml xsd xsl"),
            ("msbuild", "csproj fsproj vbproj props targets sln slnx"),
            ("github-actions", ""),
            ("compose", ""),
            ("kubernetes", ""),
            ("openapi", ""),
            ("gitlab-ci", ""),
            ("project-manifest", ""),
            ("instruction-markdown", ""),
            ("svg", "svg"),
            ("ini", "ini cfg conf properties"),
            ("pdf", "pdf"),
            ("docx", "docx"),
            ("xlsx", "xlsx"),
            ("pptx", "pptx"),
            ("text", ""),
        ] {
            registry.add_baseline(
                id,
                matches!(id, "github-actions" | "compose").then_some("tree-sitter-yaml/0.7.2"),
                suffixes,
                AdapterCapabilities {
                    lexical: true,
                    structural_facts: true,
                    native_locations: true,
                    ..Default::default()
                },
            );
        }
        for id in [
            "html",
            "vue",
            "svelte",
            "astro",
            "mdx",
            "notebook",
            "razor",
            "rmd",
            "haskell-literate",
        ] {
            let descriptor = registry
                .entries
                .get_mut(id)
                .expect("registered composite host");
            descriptor.capabilities.embedded_regions = true;
            descriptor.capabilities.definitions = true;
            descriptor.capabilities.imports = true;
            descriptor.capabilities.call_candidates = true;
            descriptor.capabilities.continuity = true;
            descriptor.limitations = vec!["Bounded mapped child syntax and original host locations; unsupported/dynamic markers remain partial. No framework compilation, execution or resolved references.".into()];
        }
        registry.entries.get_mut("objcpp").unwrap().limitations = vec![
            "Always partial: clean Objective-C islands and disjoint C++ syntax only; no compiler, preprocessor, runtime dispatch or edit-continuity qualification.".into()
        ];
        registry.entries.get_mut("elixir").unwrap().limitations = vec![
            "Literal structural macro-head and written call evidence; quoted macro bodies excluded. No macro execution, runtime resolution or native declaration continuity.".into()
        ];
        registry
    }

    fn add_baseline(
        &mut self,
        id: &str,
        grammar: Option<&str>,
        suffixes: &str,
        mut capabilities: AdapterCapabilities,
    ) {
        capabilities.continuity = capabilities.definitions
            || matches!(
                id,
                "markdown"
                    | "instruction-markdown"
                    | "text"
                    | "csv"
                    | "pdf"
                    | "docx"
                    | "xlsx"
                    | "pptx"
            );
        let limitations = if capabilities.continuity {
            vec!["Conservative native continuity within established artifact/owner identity; unsupported dimensions and partial extraction remain unknown. No resolved references or embedded-host extraction qualification.".into()]
        } else {
            vec!["Existing bounded extraction only; no resolved references, embedded-host extraction or general continuity qualification.".into()]
        };
        self.entries.insert(
            id.into(),
            AdapterDescriptor {
                profile: ExtractionProfile {
                    contract_version: if crate::composite::is_host(id) { 2 } else { 1 },
                    adapter_id: id.into(),
                    adapter_version: if matches!(
                        id,
                        "html" | "scss" | "vue" | "svelte" | "astro" | "mdx" | "razor" | "notebook"
                    ) {
                        "composite-facts/1"
                    } else if matches!(
                        id,
                        "hcl"
                            | "dockerfile"
                            | "proto"
                            | "graphql"
                            | "cmake"
                            | "make"
                            | "msbuild"
                            | "github-actions"
                            | "compose"
                    ) {
                        "repository-facts/1"
                    } else if matches!(
                        id,
                        "starlark"
                            | "gradle-kotlin"
                            | "kubernetes"
                            | "openapi"
                            | "gitlab-ci"
                            | "project-manifest"
                            | "instruction-markdown"
                    ) {
                        "project-facts/1"
                    } else if matches!(
                        id,
                        "r" | "perl"
                            | "groovy"
                            | "gradle-groovy"
                            | "rmd"
                            | "nix"
                            | "zig"
                            | "julia"
                            | "clojure"
                            | "edn"
                            | "ocaml"
                            | "ocaml-interface"
                            | "haskell"
                            | "haskell-literate"
                            | "erlang"
                            | "fsharp"
                            | "fsharp-signature"
                    ) {
                        "general-b-facts/1"
                    } else if matches!(id, "dart" | "scala" | "elixir" | "objc" | "objcpp") {
                        "general-facts/1"
                    } else if matches!(id, "php" | "ruby" | "sql" | "sql-postgresql" | "sql-mysql")
                    {
                        "repair-facts/1"
                    } else {
                        "native-facts/7"
                    }
                    .into(),
                    language: id.into(),
                    dialect: match id {
                        "sql-postgresql" => Some("postgresql".into()),
                        "sql-mysql" => Some("mysql".into()),
                        _ => None,
                    },
                    grammar_identity: grammar.map(|g| format!("tree-sitter/0.25.10:{g}")),
                    mapping_version: 1,
                    chunk_version: 2,
                    options: BTreeMap::new(),
                    configuration_inputs: BTreeMap::new(),
                },
                capabilities,
                limitations,
            },
        );
        for suffix in suffixes.split_whitespace() {
            self.suffixes.insert(suffix.into(), id.into());
        }
    }

    pub fn get(&self, id: &str) -> Option<&AdapterDescriptor> {
        self.entries.get(id)
    }

    pub fn effective_profile(&self, path: &str, id: &str) -> anyhow::Result<ExtractionProfile> {
        let mut profile = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("unknown adapter"))?
            .profile
            .clone();
        if matches!(
            id,
            "kubernetes" | "openapi" | "gitlab-ci" | "project-manifest"
        ) {
            profile.options.insert(
                "schema_flavor".into(),
                crate::project_schemas::flavor(path, id),
            );
            let grammar = if path.ends_with(".toml") {
                "tree-sitter-toml-ng/0.7.0"
            } else if path.ends_with(".json") || path.ends_with(".jsonc") {
                "tree-sitter-json/0.24.8"
            } else {
                "tree-sitter-yaml/0.7.2"
            };
            profile.grammar_identity = Some(format!("tree-sitter/0.25.10:{grammar}"));
        } else if id == "instruction-markdown" {
            profile.options.insert(
                "instruction_flavor".into(),
                crate::instruction_documents::flavor(path).into(),
            );
        }
        if crate::composite::is_host(id) {
            let children: &[&str] = match id {
                "notebook" => &[
                    "markdown",
                    "python",
                    "javascript",
                    "typescript",
                    "rust",
                    "bash",
                    "c",
                    "cpp",
                ],
                "razor" => &["csharp"],
                "mdx" => &["markdown", "tsx"],
                "haskell-literate" => &["markdown", "haskell"],
                "rmd" => &[
                    "markdown",
                    "r",
                    "python",
                    "javascript",
                    "typescript",
                    "bash",
                ],
                _ => &["javascript", "typescript", "css", "scss"],
            };
            for child in children {
                profile.options.insert(
                    format!("child_profile:{child}"),
                    self.entries[*child].profile.identity()?.to_string(),
                );
            }
        }
        Ok(profile)
    }

    pub(crate) fn validate_effective_profile(
        &self,
        path: &str,
        profile: &ExtractionProfile,
    ) -> anyhow::Result<()> {
        profile.identity()?;
        let mut checked = profile.clone();
        if checked.options.contains_key("header_context") {
            anyhow::ensure!(
                matches!(checked.adapter_id.as_str(), "c" | "cpp")
                    && path.to_ascii_lowercase().ends_with(".h")
                    && checked.options.get("header_context").map(String::as_str)
                        == Some(crate::header_context::CONTEXT_VERSION)
                    && checked.configuration_inputs.len() == 1,
                "invalid captured header profile"
            );
            super::profile::validate_relative_path(
                checked.configuration_inputs.keys().next().unwrap(),
            )?;
            checked.options.remove("header_context");
            checked.configuration_inputs.clear();
        }
        anyhow::ensure!(
            checked == self.effective_profile(path, &profile.adapter_id)?,
            "unsupported effective extraction profile"
        );
        Ok(())
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
        if path
            .rsplit_once('.')
            .is_some_and(|(_, suffix)| matches!(suffix, "pl" | "pm" | "t"))
            && source[..source.len().min(256)]
                .split(|byte| *byte == b'\n')
                .next()
                .is_some_and(|line| {
                    line.starts_with(b"#!")
                        && line
                            .split(|byte| byte.is_ascii_whitespace() || *byte == b'/')
                            .any(|word| word == b"perl")
                })
        {
            selected.profile = self.effective_profile(path, "perl")?;
            selected.basis = "bounded_perl_shebang".into();
            return Ok(selected);
        }
        if path.to_ascii_lowercase().ends_with(".m") && objective_c_prefix(source) {
            selected.profile = self.effective_profile(path, "objc")?;
            selected.basis = "bounded_objective_c_discriminator".into();
            return Ok(selected);
        }
        if selected.basis == "known_basename" {
            return Ok(selected);
        }
        if let Some((_, adapter)) = self
            .compound_suffixes
            .iter()
            .filter(|(suffix, _)| path.ends_with(suffix.as_str()))
            .max_by_key(|(suffix, _)| suffix.len())
        {
            selected.profile = self.effective_profile(path, adapter)?;
            selected.basis = "compound_suffix".into();
            return Ok(selected);
        }
        if matches!(
            selected.profile.adapter_id.as_str(),
            "yaml" | "json" | "toml"
        ) && let Some(id) = crate::project_schemas::detect(path, source)
        {
            selected.profile = self.effective_profile(path, id)?;
            selected.basis = "bounded_schema_discriminator".into();
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
        let workflow = path.strip_prefix(".github/workflows/").is_some_and(|tail| {
            !tail.contains('/') && matches!(lower_suffix.as_str(), "yaml" | "yml")
        });
        let (id, basis) = if crate::instruction_documents::recognized(path) {
            ("instruction-markdown", "known_basename")
        } else if matches!(basename, "BUILD" | "BUILD.bazel") {
            ("starlark", "known_basename")
        } else if basename.ends_with(".gradle.kts") {
            ("gradle-kotlin", "compound_suffix")
        } else if workflow {
            ("github-actions", "known_basename")
        } else if matches!(
            basename,
            "compose.yaml" | "compose.yml" | "docker-compose.yml" | "docker-compose.yaml"
        ) {
            ("compose", "known_basename")
        } else if basename == "CMakeLists.txt" {
            ("cmake", "known_basename")
        } else if basename == "Makefile" {
            ("make", "known_basename")
        } else if (basename.eq_ignore_ascii_case("Dockerfile")
            || basename.starts_with("Dockerfile."))
            && !matches!(
                lower_suffix.as_str(),
                "pdf"
                    | "docx"
                    | "xlsx"
                    | "pptx"
                    | "md"
                    | "markdown"
                    | "mdx"
                    | "csv"
                    | "tsv"
                    | "xml"
                    | "svg"
                    | "xsd"
                    | "xsl"
                    | "csproj"
                    | "props"
                    | "targets"
            )
        {
            ("dockerfile", "known_basename")
        } else if let Some(id) = self.suffixes.get(&lower_suffix) {
            (id.as_str(), "suffix")
        } else {
            ("text", "utf8_fallback")
        };
        let ambiguity = match suffix {
            "h" | "H" => vec!["Header defaults to C; C++ context is not inferred".into()],
            "sql" | "SQL" => vec![
                "SQL dialect unselected; written syntax does not establish dialect semantics"
                    .into(),
            ],
            "m" | "pl" | "pm" | "t" | "cls" | "v" | "s" | "S" => vec![format!(
                "Ambiguous suffix .{suffix}; no qualified default language"
            )],
            _ => vec![],
        };
        Selection {
            profile: self
                .effective_profile(path, id)
                .expect("registered default adapter"),
            basis: basis.into(),
            ambiguity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_conventions_are_scoped_and_do_not_capture_generic_files() {
        let registry = AdapterRegistry::baseline();
        for (path, adapter) in [
            (".github/workflows/check.yaml", "github-actions"),
            (".github/workflows/check.yml", "github-actions"),
            ("nested/.github/workflows/check.yml", "yaml"),
            (".github/workflows/nested/check.yml", "yaml"),
            ("workflows/check.yml", "yaml"),
            ("compose.yaml", "compose"),
            ("nested/docker-compose.yml", "compose"),
            ("compose.other.yaml", "yaml"),
            ("CMakeLists.txt", "cmake"),
            ("nested/Makefile", "make"),
            ("x.mk", "make"),
            ("x.fsproj", "msbuild"),
            ("x.slnx", "msbuild"),
            ("x.xml", "xml"),
            ("x.tfvars", "hcl"),
            ("Dockerfile.dev", "dockerfile"),
            ("x.proto", "proto"),
            ("x.gql", "graphql"),
        ] {
            assert_eq!(
                registry.select_default(path).profile.adapter_id,
                adapter,
                "{path}"
            );
        }
    }
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
    fn admitted_profiles_preserve_ambiguous_fallbacks() {
        let r = AdapterRegistry::baseline();
        for (path, language) in [
            ("x.vue", "vue"),
            ("x.scss", "scss"),
            ("build.gradle.kts", "gradle-kotlin"),
            ("x.mdx", "mdx"),
            ("Dockerfile.dev", "dockerfile"),
        ] {
            assert_eq!(r.select_default(path).profile.language, language);
        }
        assert_eq!(r.select_default("x.S").profile.language, "text");
        assert!(!r.select_default("x.S").ambiguity.is_empty());
    }

    #[test]
    fn r_and_gradle_are_selected_but_ambiguous_perl_requires_a_shebang() -> anyhow::Result<()> {
        let registry = AdapterRegistry::baseline();
        for (path, id) in [
            ("x.R", "r"),
            ("x.r", "r"),
            ("x.Rmd", "rmd"),
            ("build.gradle", "gradle-groovy"),
            ("x.groovy", "groovy"),
        ] {
            assert_eq!(registry.select_default(path).profile.adapter_id, id);
        }
        for path in ["x.pl", "x.pm", "x.t"] {
            assert_eq!(
                registry
                    .select_with_prefix(path, b"# example perl\n")?
                    .profile
                    .adapter_id,
                "text"
            );
            assert_eq!(
                registry
                    .select_with_prefix(path, b"#!/usr/bin/env perl\npackage Archive;\n")?
                    .profile
                    .adapter_id,
                "perl"
            );
            assert_eq!(
                registry
                    .select_with_prefix(path, b"#!/usr/bin/perl-like\n")?
                    .profile
                    .adapter_id,
                "text"
            );
        }
        Ok(())
    }

    #[test]
    fn objective_c_selection_requires_visible_bounded_markers() -> anyhow::Result<()> {
        let registry = AdapterRegistry::baseline();
        for source in [
            b"/*\n@interface Hidden\n*/\n".as_slice(),
            b"// @interface Hidden\n",
            b"x = \"\n@interface Hidden\n\";",
            b"function x = sample(y)\nx = y;\nend",
        ] {
            assert_eq!(
                registry
                    .select_with_prefix("sample.m", source)?
                    .profile
                    .adapter_id,
                "text"
            );
        }
        assert_eq!(
            registry
                .select_with_prefix("sample.m", b"/* note */\n@interface Visible\n@end")?
                .profile
                .adapter_id,
            "objc"
        );
        let mut late = vec![b' '; 4096];
        late.extend_from_slice(b"\n@interface Late");
        assert_eq!(
            registry
                .select_with_prefix("sample.m", &late)?
                .profile
                .adapter_id,
            "text"
        );
        Ok(())
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
