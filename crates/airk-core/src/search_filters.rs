// SPDX-License-Identifier: Apache-2.0
//! Optional evidence selectors over saved extraction metadata, never path guesses.
use crate::Artifact;
use serde::{Deserialize, Serialize};

/// A deliberately conservative classification of the indexed adapter/language.
/// Unknown includes mixed host documents and unrecognized future adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Code,
    Document,
    Configuration,
    Unknown,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_kind: Option<EvidenceKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_path_prefix: Option<String>,
}

impl SearchFilters {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.language
                .as_ref()
                .is_none_or(|language| { !language.is_empty() && language.len() <= 128 }),
            "language must contain 1..128 bytes"
        );
        anyhow::ensure!(
            self.exclude_path_prefix
                .as_ref()
                .is_none_or(|prefix| { !prefix.is_empty() && prefix.len() <= 4096 }),
            "exclude_path_prefix must contain 1..4096 bytes"
        );
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.language.is_none()
            && self.evidence_kind.is_none()
            && self.exclude_path_prefix.is_none()
    }

    /// Exact indexed language labels and literal relative prefixes are intentional.
    pub fn matches(&self, artifact: &Artifact) -> bool {
        self.language
            .as_ref()
            .is_none_or(|language| language == &artifact.language)
            && self
                .evidence_kind
                .is_none_or(|kind| kind == evidence_kind(artifact))
            && self
                .exclude_path_prefix
                .as_ref()
                .is_none_or(|prefix| !artifact.path.starts_with(prefix))
    }
}

/// Classification does not load retained facts or infer content from a suffix.
/// A saved profile is authoritative when present; legacy rows use their indexed
/// language. Generic text, XML and mixed code/document hosts stay unknown.
pub fn evidence_kind(artifact: &Artifact) -> EvidenceKind {
    let adapter = artifact
        .retained
        .as_ref()
        .and_then(|retained| retained.profile.as_ref())
        .map_or(artifact.language.as_str(), |profile| {
            profile.adapter_id.as_str()
        });
    match adapter {
        "rust" | "python" | "javascript" | "typescript" | "tsx" | "go" | "java" | "dart" | "r"
        | "cuda" | "wgsl" | "glsl" | "assembly" | "assembly-x86-att" | "assembly-x86-intel"
        | "assembly-aarch64" | "fortran-free" | "fortran-fixed" | "cobol-free" | "cobol-fixed"
        | "delphi" | "apex" | "solidity" | "vhdl" | "verilog" | "systemverilog" | "ocaml"
        | "ocaml-interface" | "haskell" | "erlang" | "fsharp" | "fsharp-signature" | "nix"
        | "zig" | "julia" | "clojure" | "perl" | "groovy" | "scala" | "elixir" | "objc"
        | "objcpp" | "c" | "cpp" | "csharp" | "ruby" | "php" | "bash" | "powershell" | "lua"
        | "swift" | "kotlin" | "sql" | "sql-postgresql" | "sql-mysql" | "plsql" | "tsql"
        | "vbnet" | "abap" | "css" | "scss" => EvidenceKind::Code,
        "markdown"
        | "instruction-markdown"
        | "csv"
        | "tsv"
        | "pdf"
        | "docx"
        | "xlsx"
        | "pptx"
        | "ocr" => EvidenceKind::Document,
        "json" | "jsonc" | "yaml" | "toml" | "ini" | "edn" | "msbuild" | "github-actions"
        | "compose" | "kubernetes" | "openapi" | "gitlab-ci" | "project-manifest" | "hcl"
        | "dockerfile" | "cmake" | "make" | "starlark" | "gradle-groovy" | "gradle-kotlin"
        | "proto" | "graphql" => EvidenceKind::Configuration,
        _ => EvidenceKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(path: &str, language: &str) -> Artifact {
        Artifact {
            path: path.into(),
            content_hash: String::new(),
            bytes: 0,
            chunks: vec![],
            language: language.into(),
            extraction_method: "fixture".into(),
            parse_status: "ok".into(),
            facts: vec![],
            retained: None,
        }
    }

    #[test]
    fn filters_use_indexed_metadata_and_literal_prefixes() -> anyhow::Result<()> {
        let rust = artifact("docs/sample.md", "rust");
        let filters = SearchFilters {
            language: Some("rust".into()),
            evidence_kind: Some(EvidenceKind::Code),
            exclude_path_prefix: Some("docs/generated/".into()),
        };
        filters.validate()?;
        assert!(filters.matches(&rust));
        assert!(!filters.matches(&artifact("src/sample.rs", "markdown")));
        assert!(!filters.matches(&artifact("docs/generated/a", "rust")));
        assert!(filters.matches(&artifact("docs/generated-other/a", "rust")));
        assert_eq!(
            evidence_kind(&artifact("data.rs", "json")),
            EvidenceKind::Configuration
        );
        Ok(())
    }

    #[test]
    fn unknown_and_mixed_metadata_is_not_guessed_from_a_filename() {
        for language in ["text", "xml", "notebook", "mdx", "vue", "future-adapter"] {
            assert_eq!(
                evidence_kind(&artifact("code.rs", language)),
                EvidenceKind::Unknown
            );
        }
        assert!(SearchFilters::default().is_empty());
        assert!(
            SearchFilters {
                language: Some(String::new()),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            SearchFilters {
                exclude_path_prefix: Some(String::new()),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
