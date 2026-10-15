// SPDX-License-Identifier: Apache-2.0
//! Native grammar profiles. Syntax facts never imply cross-file name resolution.
use airk_core::continuity::{CodeFingerprints, MAX_CONTINUITY_HASH_BYTES};
use airk_core::identity::framed_digest;
use airk_core::location::HostBudget;
use airk_core::{CodeEvidence, CodeSpan, DocumentLocation, EvidenceBasis, Fact};
use std::path::Path;
use tree_sitter::{Language, Node, Parser};

pub struct Analysis {
    pub language: String,
    pub status: String,
    pub facts: Vec<Fact>,
}

struct Profile {
    language: &'static str,
    grammar: Language,
    definitions: &'static [&'static str],
    imports: &'static [&'static str],
    keys: &'static [&'static str],
}

pub fn analyze(path: &str, text: &str) -> anyhow::Result<Analysis> {
    analyze_profile(path, text, profile(path))
}

/// Dispatch an already selected adapter without reinterpreting its filename.
pub(crate) fn analyze_selected(
    path: &str,
    text: &str,
    adapter_id: &str,
) -> anyhow::Result<Analysis> {
    match adapter_id {
        "ocaml" | "ocaml-interface" | "haskell" | "erlang" | "fsharp" | "fsharp-signature" => {
            return crate::languages_b_functional::analyze(path, text, adapter_id);
        }
        "nix" | "zig" | "julia" | "clojure" | "edn" => {
            return crate::languages_b_data::analyze(path, text, adapter_id);
        }
        "r" | "perl" | "groovy" | "gradle-groovy" => {
            return crate::languages_b_r_perl_groovy::analyze(path, text, adapter_id);
        }
        "dart" | "scala" | "elixir" | "objc" | "objcpp" => {
            return crate::languages_a::analyze(path, text, adapter_id);
        }
        "scss" => return crate::scss::analyze(path, text),
        "cmake" => return crate::build_cmake::analyze(path, text),
        "make" => return crate::build_make::analyze(path, text),
        "msbuild" => return crate::msbuild::analyze(path, text),
        "github-actions" => return crate::actions::analyze(path, text),
        "compose" => return crate::compose::analyze(path, text),
        "starlark" | "gradle-kotlin" => {
            return crate::project_build::analyze(path, text, adapter_id);
        }
        "kubernetes" | "openapi" | "gitlab-ci" | "project-manifest" => {
            return crate::project_schemas::analyze_selected(path, text, adapter_id);
        }
        _ => {}
    }
    analyze_profile(path, text, profile_id(adapter_id))
}

/// Shared child extraction. Caller admits text and map bytes once; this charges
/// parse attempts, visited syntax/fingerprint nodes, emitted facts and hash inputs.
/// Specialized adapters without a shared walk are rejected, never reset silently.
pub(crate) fn analyze_selected_budgeted(
    path: &str,
    text: &str,
    adapter_id: &str,
    budget: &mut HostBudget,
) -> anyhow::Result<Analysis> {
    anyhow::ensure!(
        text.len() <= crate::MAX_FILE_BYTES,
        "child source exceeds native ceiling"
    );
    if adapter_id == "scss" {
        return crate::scss::analyze_budgeted(path, text, budget);
    }
    if matches!(
        adapter_id,
        "ocaml" | "ocaml-interface" | "haskell" | "erlang" | "fsharp" | "fsharp-signature"
    ) {
        return crate::languages_b_functional::analyze_budgeted(path, text, adapter_id, budget);
    }
    if matches!(adapter_id, "nix" | "zig" | "julia" | "clojure" | "edn") {
        return crate::languages_b_data::analyze_budgeted(path, text, adapter_id, budget);
    }
    if matches!(adapter_id, "r" | "perl" | "groovy" | "gradle-groovy") {
        return crate::languages_b_r_perl_groovy::analyze_budgeted(path, text, adapter_id, budget);
    }
    anyhow::ensure!(
        !matches!(
            adapter_id,
            "php"
                | "ruby"
                | "sql"
                | "sql-postgresql"
                | "sql-mysql"
                | "hcl"
                | "dockerfile"
                | "proto"
                | "graphql"
        ),
        "adapter has no shared-budget child walk"
    );
    let selected = profile_id(adapter_id)
        .ok_or_else(|| anyhow::anyhow!("adapter has no shared-budget child walk"))?;
    analyze_profile_budgeted(path, text, Some(selected), budget, true)
}

fn analyze_profile(path: &str, text: &str, selected: Option<Profile>) -> anyhow::Result<Analysis> {
    let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
    analyze_profile_budgeted(path, text, selected, &mut budget, false)
}

fn analyze_profile_budgeted(
    path: &str,
    text: &str,
    selected: Option<Profile>,
    budget: &mut HostBudget,
    shared: bool,
) -> anyhow::Result<Analysis> {
    let Some(profile) = selected else {
        return Ok(Analysis {
            language: "text".into(),
            status: "text-only".into(),
            facts: Vec::new(),
        });
    };
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&profile.grammar)?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("parser returned no tree"))?;
    use crate::repository_syntax::RepositoryFormat;
    let repository_format = match profile.language {
        "hcl" => Some(RepositoryFormat::Hcl),
        "dockerfile" => Some(RepositoryFormat::Dockerfile),
        "proto" => Some(RepositoryFormat::Proto),
        "graphql" => Some(RepositoryFormat::Graphql),
        _ => None,
    };
    if let Some(format) = repository_format {
        return crate::repository_binding::analyze(
            path,
            text,
            tree.root_node(),
            format,
            profile.language,
        );
    }
    let mut facts = Vec::new();
    let mut pending = vec![tree.root_node()];
    let mut visited = 0;
    let mut fingerprint_bytes = 0usize;
    while let Some(node) = pending.pop() {
        visited += 1;
        budget.charge(1, 0, 0, 0)?;
        anyhow::ensure!(visited <= 500_000, "file exceeds parser node limit");
        // Error-containing declarations do not become authoritative syntax facts.
        if !node.has_error() && !node.is_missing() {
            let kind = if profile.definitions.contains(&node.kind()) {
                Some("definition")
            } else if profile.imports.contains(&node.kind()) {
                Some("import")
            } else if profile.keys.contains(&node.kind()) {
                Some("key")
            } else if call_kind(profile.language, node.kind()) {
                Some("call")
            } else if matches!(
                node.kind(),
                "catch_clause" | "except_clause" | "rescue" | "catch_block"
            ) {
                Some("exception_handler")
            } else if matches!(
                node.kind(),
                "base_list"
                    | "superclass"
                    | "super_interfaces"
                    | "class_heritage"
                    | "delegation_specifiers"
            ) {
                Some("inherits")
            } else {
                None
            };
            if let Some(kind) = kind {
                let name = if kind == "call" {
                    call_target(node, text)
                } else if matches!(kind, "import" | "inherits" | "exception_handler") {
                    node.utf8_text(text.as_bytes()).ok().map(str::to_owned)
                } else {
                    fact_name(node, text)
                };
                if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
                    let start = node.start_position().row + 1;
                    let end = node.end_position().row + usize::from(node.end_position().column > 0);
                    budget.charge(0, 1, 0, 0)?;
                    facts.push(Fact {
                        code: if kind == "key" {
                            None
                        } else {
                            Some(code_evidence(
                                path,
                                node,
                                text,
                                &profile,
                                kind,
                                name.trim(),
                                EvidenceBudget {
                                    fingerprint_bytes: &mut fingerprint_bytes,
                                    host: shared.then_some(&mut *budget),
                                },
                            )?)
                        },
                        name: bounded_name(name.trim()),
                        kind: kind.into(),
                        syntax_kind: node.kind().into(),
                        start_line: start,
                        end_line: end.max(start),
                        document: None,
                        location: DocumentLocation::Lines {
                            start,
                            end: end.max(start),
                        },
                    });
                    if kind == "call"
                        && let Some(rule) = framework_call(profile.language, name.trim())
                    {
                        let mut cue = facts.last().expect("call fact").clone();
                        cue.kind = rule.into();
                        let evidence = cue.code.as_mut().expect("code call");
                        evidence.basis = EvidenceBasis::StructuralRule;
                        evidence.producer = "airk-framework-cues".into();
                        evidence.producer_version = "1".into();
                        evidence.coverage="Matched a written call pattern; aliases, framework identity and runtime behavior unverified".into();
                        budget.charge(0, 1, 0, 0)?;
                        facts.push(cue);
                    }
                    if kind == "definition" {
                        let definition = facts.last().expect("definition").clone();
                        for rule in annotation_rules(
                            profile.language,
                            definition
                                .code
                                .as_ref()
                                .map_or(&[], |c| c.modifiers.as_slice()),
                        ) {
                            let mut cue = definition.clone();
                            cue.kind = rule.into();
                            let evidence = cue.code.as_mut().expect("definition evidence");
                            evidence.basis = EvidenceBasis::StructuralRule;
                            evidence.producer = "airk-framework-cues".into();
                            evidence.producer_version = "1".into();
                            evidence.coverage="Recognized written attribute/annotation; framework binding, security and runtime behavior unverified".into();
                            budget.charge(0, 1, 0, 0)?;
                            facts.push(cue);
                        }
                    }
                    anyhow::ensure!(facts.len() <= 10_000, "file exceeds 10000 syntax facts");
                }
            }
        }
        let mut cursor = node.walk();
        // Iterative walk avoids Rust stack growth on deeply nested input.
        for child in node.named_children(&mut cursor) {
            anyhow::ensure!(pending.len() < 500_000, "parser pending limit exceeded");
            pending.push(child);
        }
    }
    facts.sort_by(|a, b| (a.start_line, &a.kind, &a.name).cmp(&(b.start_line, &b.kind, &b.name)));
    let analysis = Analysis {
        language: profile.language.into(),
        status: if tree.root_node().has_error() {
            "partial-syntax"
        } else {
            "syntax"
        }
        .into(),
        facts,
    };
    if matches!(
        profile.language,
        "php" | "ruby" | "sql" | "sql-postgresql" | "sql-mysql"
    ) {
        crate::syntax_repairs::enrich(analysis, path, text, tree.root_node(), profile.language)
    } else {
        Ok(analysis)
    }
}

fn call_kind(language: &str, kind: &str) -> bool {
    let kinds: &[&str] = match language {
        "rust" | "javascript" | "typescript" | "tsx" | "go" | "c" | "cpp" | "swift" | "kotlin" => {
            &["call_expression"]
        }
        "lua" => &["function_call"],
        "python" => &["call"],
        "java" => &["method_invocation", "object_creation_expression"],
        "csharp" => &["invocation_expression", "object_creation_expression"],
        "ruby" => &["call"],
        "php" => &[
            "function_call_expression",
            "member_call_expression",
            "scoped_call_expression",
        ],
        "bash" => &["command"],
        "powershell" => &["command", "invokation_expression"],
        _ => &[],
    };
    kinds.contains(&kind)
}

fn call_target(node: Node<'_>, text: &str) -> Option<String> {
    for field in [
        "function",
        "expression",
        "name",
        "type",
        "method",
        "command_name",
    ] {
        if let Some(child) = node.child_by_field_name(field) {
            let name = child.utf8_text(text.as_bytes()).ok()?;
            // Java/PHP receiver and method name are separate fields.
            if matches!(field, "name" | "method")
                && let Some(receiver) = node
                    .child_by_field_name("object")
                    .or_else(|| node.child_by_field_name("receiver"))
            {
                return Some(format!(
                    "{}.{}",
                    receiver.utf8_text(text.as_bytes()).ok()?,
                    name
                ));
            }
            return Some(name.into());
        }
    }
    if node.kind() == "call_expression" {
        let first = node.named_child(0)?;
        if matches!(
            first.kind(),
            "simple_identifier" | "identifier" | "navigation_expression"
        ) {
            return first.utf8_text(text.as_bytes()).ok().map(str::to_owned);
        }
    }
    None
}

fn symbol_id(path: &str, node: Node<'_>) -> String {
    format!(
        "syntax:{path}:{}:{}:{}",
        node.start_byte(),
        node.end_byte(),
        node.kind()
    )
}

struct EvidenceBudget<'a> {
    fingerprint_bytes: &'a mut usize,
    host: Option<&'a mut HostBudget>,
}

fn code_evidence(
    path: &str,
    node: Node<'_>,
    text: &str,
    profile: &Profile,
    kind: &str,
    name: &str,
    work: EvidenceBudget<'_>,
) -> anyhow::Result<CodeEvidence> {
    let EvidenceBudget {
        fingerprint_bytes,
        host: mut budget,
    } = work;
    let mut omitted = name.len() > 1024;
    let mut scopes = Vec::new();
    let mut full_scopes = Vec::new();
    let mut owner_id = None;
    let mut parent = node.parent();
    let mut count = 0;
    while let Some(ancestor) = parent {
        if let Some(budget) = budget.as_deref_mut() {
            budget.charge(1, 0, 0, 0)?;
        }
        count += 1;
        if count > 128 {
            break;
        }
        if profile.definitions.contains(&ancestor.kind()) || ancestor.kind() == "impl_item" {
            let scope_name = fact_name(ancestor, text).or_else(|| {
                ancestor
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(text.as_bytes()).ok())
                    .map(str::to_owned)
            });
            if let Some(scope_name) = scope_name {
                omitted |= scope_name.len() > 1024;
                if owner_id.is_none() {
                    owner_id = Some(symbol_id(path, ancestor));
                }
                scopes.push(bounded_name(&scope_name));
                full_scopes.push(scope_name);
            }
        }
        parent = ancestor.parent();
    }
    scopes.reverse();
    full_scopes.reverse();
    let mut modifiers = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(budget) = budget.as_deref_mut() {
            budget.charge(1, 0, 0, 0)?;
        }
        if matches!(
            child.kind(),
            "visibility_modifier"
                | "modifier"
                | "modifiers"
                | "attribute_list"
                | "attribute_item"
                | "annotation"
                | "marker_annotation"
        ) && let Ok(s) = child.utf8_text(text.as_bytes())
        {
            omitted |= s.len() > 1024;
            modifiers.push(bounded_name(s));
        }
    }
    let mut previous = node.prev_named_sibling();
    let mut documentation = None;
    let mut full_documentation = Vec::new();
    let mut documentation_complete = true;
    let mut external_modifiers = Vec::new();
    let mut modifiers_complete = true;
    let mut next_documentation_row = node.start_position().row;
    for _ in 0..16 {
        let Some(sibling) = previous else {
            break;
        };
        if let Some(budget) = budget.as_deref_mut() {
            budget.charge(1, 0, 0, 0)?;
        }
        if sibling.end_position().row + 2 < next_documentation_row {
            break;
        }
        let s = sibling.utf8_text(text.as_bytes()).unwrap_or("");
        omitted |= s.len() > 1024;
        if sibling.kind().contains("comment") {
            documentation = Some(bounded_name(s));
            full_documentation.push(s);
        } else if sibling.kind() == "attribute_item" {
            modifiers.push(bounded_name(s));
            external_modifiers.push(s);
        } else {
            break;
        }
        next_documentation_row = sibling.start_position().row;
        previous = sibling.prev_named_sibling();
    }
    if previous.is_some_and(|s| {
        s.kind().contains("comment") && s.end_position().row + 2 >= next_documentation_row
    }) {
        documentation_complete = false;
    }
    if previous.is_some_and(|s| {
        s.kind() == "attribute_item" && s.end_position().row + 2 >= next_documentation_row
    }) {
        modifiers_complete = false;
    }
    external_modifiers.reverse();
    full_documentation.reverse();
    if let Some(parent) = node.parent().filter(|p| p.kind() == "decorated_definition") {
        let mut cursor = parent.walk();
        for (index, decorator) in parent
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "decorator")
            .take(17)
            .enumerate()
        {
            if index == 16 {
                modifiers_complete = false;
            }
            if modifiers.len() >= 32 {
                omitted = true;
                modifiers_complete = false;
                break;
            }
            omitted |= decorator
                .utf8_text(text.as_bytes())
                .is_ok_and(|s| s.len() > 1024);
            modifiers.push(bounded_name(
                decorator.utf8_text(text.as_bytes()).unwrap_or(""),
            ));
            external_modifiers.push(decorator.utf8_text(text.as_bytes()).unwrap_or(""));
        }
    }
    let body = node.child_by_field_name("body");
    let signature = if kind == "definition" {
        Some(
            text[node.start_byte()..body.map_or(node.end_byte(), |n| n.start_byte())]
                .trim()
                .to_owned(),
        )
    } else {
        None
    };
    let get_field = |names: &[&str]| -> Option<String> {
        names.iter().find_map(|field| {
            node.child_by_field_name(field)
                .and_then(|n| n.utf8_text(text.as_bytes()).ok())
                .map(str::to_owned)
        })
    };
    let parameters = get_field(&["parameters", "parameter_list"]).or_else(|| {
        node.child_by_field_name("declarator")
            .and_then(|n| n.child_by_field_name("parameters"))
            .and_then(|n| n.utf8_text(text.as_bytes()).ok())
            .map(str::to_owned)
    });
    let return_type = get_field(&["return_type", "result", "type"]);
    let qualified_display = if scopes.is_empty() {
        name.into()
    } else {
        format!("{}::{name}", scopes.join("::"))
    };
    let details_truncated = omitted
        || qualified_display.len() > 1024
        || count > 128
        || signature.as_ref().is_some_and(|s| s.len() > 1024)
        || parameters.as_ref().is_some_and(|s| s.len() > 1024)
        || return_type.as_ref().is_some_and(|s| s.len() > 1024);
    let qualified_name = bounded_name(&qualified_display);
    // Hash complete original bytes before any display truncation. Literal and
    // whitespace changes remain observable; no token-erasing normalization.
    let fingerprints = (kind == "definition")
        .then(|| -> anyhow::Result<CodeFingerprints> {
            let signature_bytes = body.map(|body| &text[node.start_byte()..body.start_byte()]);
            let body_bytes = body.and_then(|body| body.utf8_text(text.as_bytes()).ok());
            let needed = name
                .len()
                .saturating_mul(2)
                .saturating_add(full_scopes.iter().map(String::len).sum::<usize>())
                .saturating_add(signature_bytes.map_or(0, str::len))
                .saturating_add(body_bytes.map_or(0, str::len))
                .saturating_add(external_modifiers.iter().map(|s| s.len()).sum::<usize>())
                .saturating_add(full_documentation.iter().map(|s| s.len()).sum::<usize>());
            if let Some(budget) = budget.as_deref_mut() {
                budget.charge_fingerprints(needed)?;
            }
            *fingerprint_bytes = fingerprint_bytes.saturating_add(needed);
            let admitted = *fingerprint_bytes <= MAX_CONTINUITY_HASH_BYTES;
            let hash = |domain: &[u8], value: &str| framed_digest(domain, &[value.as_bytes()]);
            let mut qualified: Vec<&[u8]> = full_scopes.iter().map(|s| s.as_bytes()).collect();
            qualified.push(name.as_bytes());
            let body_informative = if admitted {
                if let Some(body) = body {
                    informative_body_with_budget(body, text, budget)?
                } else {
                    false
                }
            } else {
                false
            };
            Ok(CodeFingerprints {
                version: 1,
                name: admitted.then(|| hash(b"code-name/1", name)),
                qualified_name: (admitted && count <= 128)
                    .then(|| framed_digest(b"code-qualified-name/1", &qualified)),
                signature: signature_bytes
                    .filter(|_| admitted && modifiers_complete)
                    .map(|s| {
                        let mut fields: Vec<&[u8]> =
                            external_modifiers.iter().map(|s| s.as_bytes()).collect();
                        fields.push(s.as_bytes());
                        framed_digest(b"code-signature/1", &fields)
                    }),
                body: body_bytes
                    .filter(|_| admitted)
                    .map(|s| hash(b"code-body/1", s)),
                body_informative,
                documentation: (admitted && documentation_complete && profile.language != "python")
                    .then(|| {
                        let fields: Vec<&[u8]> =
                            full_documentation.iter().map(|s| s.as_bytes()).collect();
                        framed_digest(b"code-documentation/1", &fields)
                    }),
            })
        })
        .transpose()?;
    Ok(CodeEvidence {
        fingerprints,
        span: Some(CodeSpan {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        }),
        signature_span: (kind == "definition").then(|| CodeSpan {
            start_byte: node.start_byte(),
            end_byte: body.map_or(node.end_byte(), |n| n.start_byte()),
        }),
        symbol_id: symbol_id(path, node),
        qualified_name,
        scope: scopes,
        owner_id,
        signature: signature.map(|s| bounded_name(&s)),
        parameters: parameters.map(|s| bounded_name(&s)),
        return_type: return_type.map(|s| bounded_name(&s)),
        modifiers,
        documentation,
        target: matches!(kind, "call" | "inherits" | "import").then(|| bounded_name(name)),
        basis: EvidenceBasis::Syntax,
        producer: format!("airk-tree-sitter/{}", profile.language),
        producer_version: "native-facts/4".into(),
        coverage:
            "Selected grammar nodes; written syntax only, no inferred types or target resolution"
                .into(),
        details_truncated,
    })
}

pub(crate) fn informative_body(body: Node<'_>, text: &str) -> bool {
    informative_body_with_budget(body, text, None).unwrap_or(false)
}

/// Complete original declaration inputs for adapters with dedicated node mappings.
/// `None` means unknown, including documentation; `Some(&[])` means known absence.
pub(crate) fn declaration_fingerprints(
    source: &str,
    name: &str,
    qualified_parts: &[&str],
    signature: Option<airk_core::location::ByteSpan>,
    body: Option<Node<'_>>,
    documentation: Option<&[airk_core::location::ByteSpan]>,
    budget: &mut HostBudget,
) -> anyhow::Result<CodeFingerprints> {
    let read = |span: airk_core::location::ByteSpan| -> anyhow::Result<&str> {
        span.validate(source)?;
        Ok(&source[span.start..span.end])
    };
    let signature = signature.map(read).transpose()?;
    let body_text = body
        .map(|node| {
            read(airk_core::location::ByteSpan {
                start: node.start_byte(),
                end: node.end_byte(),
            })
        })
        .transpose()?;
    let docs = documentation
        .map(|spans| {
            spans
                .iter()
                .copied()
                .map(read)
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .transpose()?;
    let needed = qualified_parts
        .iter()
        .fold(name.len().saturating_mul(2), |sum, part| {
            sum.saturating_add(part.len())
        })
        .saturating_add(signature.map_or(0, str::len))
        .saturating_add(body_text.map_or(0, str::len))
        .saturating_add(
            docs.as_ref()
                .map_or(0, |docs| docs.iter().map(|doc| doc.len()).sum()),
        );
    budget.charge_fingerprints(needed)?;
    let mut qualified: Vec<&[u8]> = qualified_parts.iter().map(|part| part.as_bytes()).collect();
    qualified.push(name.as_bytes());
    Ok(CodeFingerprints {
        version: 1,
        name: Some(framed_digest(b"code-name/1", &[name.as_bytes()])),
        qualified_name: Some(framed_digest(b"code-qualified-name/1", &qualified)),
        signature: signature.map(|value| framed_digest(b"code-signature/1", &[value.as_bytes()])),
        body: body_text.map(|value| framed_digest(b"code-body/1", &[value.as_bytes()])),
        body_informative: if let Some(body) = body {
            informative_body_with_budget(body, source, Some(budget))?
        } else {
            false
        },
        documentation: docs.map(|docs| {
            framed_digest(
                b"code-documentation/1",
                &docs.iter().map(|doc| doc.as_bytes()).collect::<Vec<_>>(),
            )
        }),
    })
}

fn informative_body_with_budget(
    body: Node<'_>,
    text: &str,
    mut budget: Option<&mut HostBudget>,
) -> anyhow::Result<bool> {
    if body.end_byte() - body.start_byte() < 32 {
        return Ok(false);
    }
    let mut words = std::collections::BTreeSet::new();
    let mut pending = vec![body];
    let mut visited = 0usize;
    while let Some(node) = pending.pop() {
        visited += 1;
        if let Some(budget) = budget.as_deref_mut() {
            budget.charge(1, 0, 0, 0)?;
        }
        if visited > 4096 {
            return Ok(false);
        }
        if node.kind().contains("comment") {
            continue;
        }
        if node.named_child_count() > 0 {
            let mut cursor = node.walk();
            // The admission cap also bounds the traversal's pending storage.
            for child in node.named_children(&mut cursor).take(4097) {
                if pending.len() >= 4096 {
                    return Ok(false);
                }
                pending.push(child);
            }
            continue;
        }
        let Ok(token) = node.utf8_text(text.as_bytes()) else {
            return Ok(false);
        };
        for word in token.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if word.len() < 2
                || word.chars().all(|c| c.is_numeric())
                || matches!(
                    word,
                    "return"
                        | "let"
                        | "var"
                        | "const"
                        | "if"
                        | "else"
                        | "true"
                        | "false"
                        | "null"
                        | "nil"
                        | "None"
                        | "self"
                        | "this"
                        | "pass"
                        | "todo"
                        | "unimplemented"
                )
            {
                continue;
            }
            words.insert(word);
            if words.len() >= 4 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn framework_call(language: &str, target: &str) -> Option<&'static str> {
    match language {
        "javascript" | "typescript" | "tsx"
            if ["app.", "router."].iter().any(|p| target.starts_with(p))
                && ["get", "post", "put", "patch", "delete", "use"]
                    .iter()
                    .any(|m| target.ends_with(&format!(".{m}"))) =>
        {
            Some("route_registration")
        }
        "python"
            if [
                "app.route",
                "app.get",
                "app.post",
                "router.get",
                "router.post",
            ]
            .contains(&target) =>
        {
            Some("route_registration")
        }
        "javascript" | "typescript" | "tsx"
            if ["describe", "it", "test", "test.describe"].contains(&target) =>
        {
            Some("test_registration")
        }
        "javascript" | "typescript" | "tsx" | "python" | "rust" | "go"
            if [
                "db.query",
                "db.execute",
                "conn.execute",
                "connection.execute",
                "pool.query",
                "pool.execute",
            ]
            .contains(&target) =>
        {
            Some("database_call")
        }
        "python" if target == "Depends" => Some("dependency_injection"),
        _ => None,
    }
}

fn annotation_rules(language: &str, modifiers: &[String]) -> Vec<&'static str> {
    let rules: &[(&str, &str)] = match language {
        "rust" => &[
            ("#[test]", "test_declaration"),
            ("#[tokio::test]", "test_declaration"),
            ("#[get(", "route_annotation"),
            ("#[post(", "route_annotation"),
        ],
        "java" => &[
            ("@Test", "test_declaration"),
            ("@GetMapping", "route_annotation"),
            ("@PostMapping", "route_annotation"),
            ("@RequestMapping", "route_annotation"),
            ("@Inject", "dependency_injection"),
            ("@Autowired", "dependency_injection"),
        ],
        "csharp" => &[
            ("[Fact]", "test_declaration"),
            ("[Test]", "test_declaration"),
            ("[TestMethod]", "test_declaration"),
            ("[HttpGet", "route_annotation"),
            ("[HttpPost", "route_annotation"),
            ("[Route", "route_annotation"),
            ("[Inject]", "dependency_injection"),
        ],
        _ => &[],
    };
    let mut found = Vec::new();
    for (pattern, kind) in rules {
        if modifiers.iter().any(|m| {
            m.trim().strip_prefix(pattern).is_some_and(|suffix| {
                pattern.ends_with([']', '('])
                    || suffix.is_empty()
                    || suffix.starts_with(['(', ' ', ']'])
            })
        }) && !found.contains(kind)
        {
            found.push(*kind);
        }
    }
    found
}

fn bounded_name(name: &str) -> String {
    let mut end = name.len().min(1024);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}

fn fact_name(node: Node<'_>, text: &str) -> Option<String> {
    if node.kind() == "impl_item" {
        return node
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(text.as_bytes()).ok())
            .map(str::to_owned);
    }
    for field in ["name", "key", "declarator"] {
        if let Some(child) = node.child_by_field_name(field) {
            if field == "declarator" {
                return declarator_name(child, text);
            }
            return child.utf8_text(text.as_bytes()).ok().map(str::to_owned);
        }
    }
    let mut cursor = node.walk();
    let candidate = node.named_children(&mut cursor).find(|n| {
        matches!(
            n.kind(),
            "identifier"
                | "type_identifier"
                | "name"
                | "simple_identifier"
                | "function_name"
                | "string"
                | "bare_key"
                | "dotted_key"
                | "table_name"
                | "object_reference"
                | "tag_name"
                | "selectors"
                | "flow_node"
        )
    });
    candidate
        .and_then(|n| n.utf8_text(text.as_bytes()).ok())
        .map(str::to_owned)
}

fn declarator_name(mut node: Node<'_>, text: &str) -> Option<String> {
    for _ in 0..32 {
        if matches!(
            node.kind(),
            "identifier"
                | "field_identifier"
                | "qualified_identifier"
                | "operator_name"
                | "destructor_name"
        ) {
            return node.utf8_text(text.as_bytes()).ok().map(str::to_owned);
        }
        node = node
            .child_by_field_name("declarator")
            .or_else(|| node.child_by_field_name("name"))?;
    }
    None
}

fn profile(path: &str) -> Option<Profile> {
    let extension = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let id = match extension.as_str() {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "sh" | "bash" | "zsh" => "bash",
        "ps1" | "psm1" | "psd1" => "powershell",
        "lua" => "lua",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "html" | "htm" | "vue" | "svelte" => "html",
        "css" | "scss" => "css",
        "sql" => "sql",
        "hcl" | "tf" | "tfvars" => "hcl",
        "proto" => "proto",
        "graphql" | "gql" => "graphql",
        "dockerfile" => "dockerfile",
        _ => return None,
    };
    profile_id(id)
}

fn profile_id(adapter_id: &str) -> Option<Profile> {
    let (language, grammar, definitions, imports, keys): (
        &str,
        Language,
        &[&str],
        &[&str],
        &[&str],
    ) = match adapter_id {
        "hcl" => ("hcl", tree_sitter_hcl::LANGUAGE.into(), &[], &[], &[]),
        "dockerfile" => (
            "dockerfile",
            tree_sitter_dockerfile_updated::language(),
            &[],
            &[],
            &[],
        ),
        "proto" => ("proto", tree_sitter_proto::LANGUAGE.into(), &[], &[], &[]),
        "graphql" => (
            "graphql",
            tree_sitter_graphql::LANGUAGE.into(),
            &[],
            &[],
            &[],
        ),
        "rust" => (
            "rust",
            tree_sitter_rust::LANGUAGE.into(),
            &[
                "function_item",
                "impl_item",
                "struct_item",
                "enum_item",
                "trait_item",
                "type_item",
                "const_item",
                "static_item",
                "mod_item",
                "macro_definition",
            ],
            &["use_declaration", "extern_crate_declaration"],
            &[],
        ),
        "python" => (
            "python",
            tree_sitter_python::LANGUAGE.into(),
            &["function_definition", "class_definition"],
            &["import_statement", "import_from_statement"],
            &[],
        ),
        "javascript" => (
            "javascript",
            tree_sitter_javascript::LANGUAGE.into(),
            &[
                "function_declaration",
                "generator_function_declaration",
                "class_declaration",
                "method_definition",
                "variable_declarator",
            ],
            &["import_statement"],
            &[],
        ),
        "typescript" => (
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            &[
                "function_declaration",
                "function_signature",
                "class_declaration",
                "interface_declaration",
                "type_alias_declaration",
                "enum_declaration",
                "method_definition",
                "variable_declarator",
            ],
            &["import_statement", "import_alias"],
            &[],
        ),
        "tsx" => (
            "tsx",
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            &[
                "function_declaration",
                "class_declaration",
                "interface_declaration",
                "type_alias_declaration",
                "method_definition",
                "variable_declarator",
            ],
            &["import_statement"],
            &[],
        ),
        "go" => (
            "go",
            tree_sitter_go::LANGUAGE.into(),
            &["function_declaration", "method_declaration", "type_spec"],
            &["import_declaration"],
            &[],
        ),
        "java" => (
            "java",
            tree_sitter_java::LANGUAGE.into(),
            &[
                "class_declaration",
                "interface_declaration",
                "enum_declaration",
                "record_declaration",
                "method_declaration",
                "constructor_declaration",
            ],
            &["import_declaration", "package_declaration"],
            &[],
        ),
        "c" => (
            "c",
            tree_sitter_c::LANGUAGE.into(),
            &[
                "function_definition",
                "struct_specifier",
                "enum_specifier",
                "type_definition",
            ],
            &["preproc_include"],
            &[],
        ),
        "cpp" => (
            "cpp",
            tree_sitter_cpp::LANGUAGE.into(),
            &[
                "function_definition",
                "class_specifier",
                "struct_specifier",
                "enum_specifier",
                "namespace_definition",
                "type_definition",
            ],
            &["preproc_include", "using_declaration"],
            &[],
        ),
        "csharp" => (
            "csharp",
            tree_sitter_c_sharp::LANGUAGE.into(),
            &[
                "class_declaration",
                "struct_declaration",
                "interface_declaration",
                "enum_declaration",
                "record_declaration",
                "method_declaration",
                "constructor_declaration",
                "namespace_declaration",
            ],
            &["using_directive"],
            &[],
        ),
        "ruby" => (
            "ruby",
            tree_sitter_ruby::LANGUAGE.into(),
            &["class", "module", "method", "singleton_method"],
            &[],
            &[],
        ),
        "php" => (
            "php",
            tree_sitter_php::LANGUAGE_PHP.into(),
            &[
                "class_declaration",
                "interface_declaration",
                "trait_declaration",
                "function_definition",
                "method_declaration",
            ],
            &[
                "namespace_use_declaration",
                "include_expression",
                "require_expression",
            ],
            &[],
        ),
        "bash" => (
            "bash",
            tree_sitter_bash::LANGUAGE.into(),
            &["function_definition"],
            &[],
            &[],
        ),
        "powershell" => (
            "powershell",
            tree_sitter_powershell::LANGUAGE.into(),
            &["function_statement", "class_statement"],
            &["using_statement"],
            &[],
        ),
        "lua" => (
            "lua",
            tree_sitter_lua::LANGUAGE.into(),
            &["function_declaration"],
            &[],
            &[],
        ),
        "swift" => (
            "swift",
            tree_sitter_swift::LANGUAGE.into(),
            &[
                "function_declaration",
                "class_declaration",
                "protocol_declaration",
                "typealias_declaration",
            ],
            &["import_declaration"],
            &[],
        ),
        "kotlin" => (
            "kotlin",
            tree_sitter_kotlin_ng::LANGUAGE.into(),
            &[
                "function_declaration",
                "class_declaration",
                "object_declaration",
                "type_alias",
            ],
            &["import"],
            &[],
        ),
        "json" => (
            "json",
            tree_sitter_json::LANGUAGE.into(),
            &[],
            &[],
            &["pair"],
        ),
        "yaml" => (
            "yaml",
            tree_sitter_yaml::LANGUAGE.into(),
            &[],
            &[],
            &["block_mapping_pair", "flow_pair"],
        ),
        "toml" => (
            "toml",
            tree_sitter_toml_ng::LANGUAGE.into(),
            &[],
            &[],
            &["pair", "table", "table_array_element"],
        ),
        "html" => (
            "html",
            tree_sitter_html::LANGUAGE.into(),
            &[],
            &[],
            &["start_tag"],
        ),
        "css" => (
            "css",
            tree_sitter_css::LANGUAGE.into(),
            &[],
            &[],
            &["rule_set"],
        ),
        "sql" | "sql-postgresql" | "sql-mysql" => (
            match adapter_id {
                "sql-postgresql" => "sql-postgresql",
                "sql-mysql" => "sql-mysql",
                _ => "sql",
            },
            tree_sitter_sequel::LANGUAGE.into(),
            &[
                "create_table",
                "create_view",
                "create_function",
                "create_index",
            ],
            &[],
            &[],
        ),
        _ => return None,
    };
    Some(Profile {
        language,
        grammar,
        definitions,
        imports,
        keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_walks_share_facts_and_do_not_readmit_text() {
        let mut budget = HostBudget::new(1000, 1, 0).unwrap();
        let first =
            analyze_selected_budgeted("a.js", "function alpha() {}", "javascript", &mut budget)
                .unwrap();
        assert_eq!(first.facts.len(), 1);
        assert!(
            analyze_selected_budgeted("b.js", "function beta() {}", "javascript", &mut budget)
                .is_err()
        );
    }

    #[test]
    fn empty_and_malformed_child_walks_consume_shared_nodes() {
        let mut budget = HostBudget::new(3, 0, 0).unwrap();
        analyze_selected_budgeted("a.js", "", "javascript", &mut budget).unwrap();
        assert!(analyze_selected_budgeted("b.js", "", "javascript", &mut budget).is_err());
        let mut budget = HostBudget::new(2, 10, 0).unwrap();
        assert!(
            analyze_selected_budgeted("bad.js", "function ( {", "javascript", &mut budget).is_err()
        );
    }

    #[test]
    fn child_fingerprints_share_complete_input_allowance() {
        let mut budget = HostBudget::new(1000, 10, 0).unwrap();
        budget
            .charge_fingerprints(MAX_CONTINUITY_HASH_BYTES - 1)
            .unwrap();
        let error = analyze_selected_budgeted("a.rs", "fn alpha() {}", "rust", &mut budget)
            .err()
            .unwrap();
        assert!(error.to_string().contains("fingerprint budget"));
        assert!(budget.charge_fingerprints(1).is_err());
    }

    #[test]
    fn specialized_child_adapter_cannot_reset_shared_budget() {
        for adapter in ["php", "ruby", "sql", "hcl", "gradle-kotlin", "unknown"] {
            let mut budget = HostBudget::new(1000, 10, 0).unwrap();
            assert!(
                analyze_selected_budgeted("child", "", adapter, &mut budget)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("shared-budget child walk")
            );
        }
    }

    #[test]
    fn native_annotation_rules_preserve_evidence_and_do_not_guess_security() -> anyhow::Result<()> {
        for (path, text, kind) in [
            (
                "a.rs",
                "#[test]\nfn check() { target(); }",
                "test_declaration",
            ),
            (
                "a.java",
                "class A { @GetMapping(\"/x\") void run() {} }",
                "route_annotation",
            ),
            (
                "a.cs",
                "class A { [HttpGet(\"/x\")] void Run() {} }",
                "route_annotation",
            ),
        ] {
            let analysis = analyze(path, text)?;
            assert!(
                analysis.facts.iter().any(|f| f.kind == kind
                    && f.code
                        .as_ref()
                        .is_some_and(|c| c.basis == EvidenceBasis::StructuralRule)),
                "{path}: {:?}",
                analysis.facts
            );
        }
        assert!(annotation_rules("java", &["@GetMappingOther".into()]).is_empty());
        assert!(annotation_rules("csharp", &["[HttpGetter]".into()]).is_empty());
        Ok(())
    }

    #[test]
    fn call_profiles_extract_written_targets_across_common_languages() -> anyhow::Result<()> {
        let fixtures = [
            ("a.rs", "fn caller() { target(); }"),
            ("a.py", "def caller():\n    target()\n"),
            ("a.js", "function caller() { target(); }"),
            ("a.ts", "function caller(): void { target(); }"),
            ("a.go", "package p\nfunc caller() { target() }"),
            ("a.java", "class A { void caller() { target(); } }"),
            ("a.c", "void caller(void) { target(); }"),
            ("a.cpp", "void caller() { target(); }"),
            ("a.cs", "class A { void caller() { target(); } }"),
            ("a.rb", "def caller\n  target()\nend"),
            ("a.php", "<?php function caller() { target(); }"),
            ("a.sh", "caller() { target; }"),
            ("a.ps1", "function caller { target }"),
            ("a.lua", "function caller() target() end"),
            ("a.swift", "func caller() { target() }"),
            ("a.kt", "fun caller() { target() }"),
        ];
        let mut failures = Vec::new();
        for (path, text) in fixtures {
            let analysis = analyze(path, text)?;
            if !analysis
                .facts
                .iter()
                .any(|f| f.kind == "call" && f.name == "target")
            {
                let mut parser = Parser::new();
                parser.set_language(&profile(path).expect("profile").grammar)?;
                failures.push(format!(
                    "{path}: {}",
                    parser
                        .parse(text, None)
                        .expect("tree")
                        .root_node()
                        .to_sexp()
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    }

    #[test]
    fn qualified_scopes_signatures_calls_and_framework_patterns_remain_syntax() -> anyhow::Result<()>
    {
        let text = "class Auth { authenticate(token: string): boolean { db.query(token); return true; } }\nrouter.get('/login', handler);\n// db.execute()\nconst literal = 'router.post()';\n";
        let analysis = analyze("auth.ts", text)?;
        let definition = analysis
            .facts
            .iter()
            .find(|f| f.name == "authenticate" && f.kind == "definition")
            .expect("method");
        let code = definition.code.as_ref().expect("code evidence");
        assert_eq!(code.qualified_name, "Auth::authenticate");
        assert_eq!(code.parameters.as_deref(), Some("(token: string)"));
        assert!(
            code.return_type
                .as_ref()
                .expect("return type")
                .contains("boolean")
        );
        assert!(
            !code
                .signature
                .as_ref()
                .expect("signature")
                .contains("db.query")
        );
        let declaration = code.span.as_ref().expect("declaration span");
        assert_eq!(
            &text[declaration.start_byte..declaration.end_byte],
            "authenticate(token: string): boolean { db.query(token); return true; }"
        );
        let signature = code.signature_span.as_ref().expect("signature span");
        assert_eq!(
            text[signature.start_byte..signature.end_byte].trim(),
            code.signature.as_deref().expect("signature")
        );
        assert!(
            analysis
                .facts
                .iter()
                .any(|f| f.kind == "database_call" && f.name == "db.query")
        );
        assert!(
            analysis
                .facts
                .iter()
                .any(|f| f.kind == "route_registration" && f.name == "router.get")
        );
        assert!(
            !analysis
                .facts
                .iter()
                .any(|f| f.kind == "call" && (f.name == "db.execute" || f.name == "router.post"))
        );
        assert!(
            analysis
                .facts
                .iter()
                .filter_map(|f| f.code.as_ref())
                .all(|c| matches!(
                    c.basis,
                    EvidenceBasis::Syntax | EvidenceBasis::StructuralRule
                ))
        );
        let duplicate = analyze(
            "nested.py",
            "class A:\n    def same(self):\n        return foo()\nclass B:\n    def same(self):\n        return foo()\n",
        )?;
        let methods = duplicate
            .facts
            .iter()
            .filter(|f| f.name == "same")
            .map(|f| f.code.as_ref().expect("code"))
            .collect::<Vec<_>>();
        assert_eq!(methods.len(), 2);
        assert_ne!(methods[0].symbol_id, methods[1].symbol_id);
        assert_ne!(methods[0].qualified_name, methods[1].qualified_name);
        Ok(())
    }

    #[test]
    fn common_language_definitions_have_real_native_grammar_fixtures() -> anyhow::Result<()> {
        let fixtures = [
            (
                "a.rs",
                "use std::fmt;\nfn authenticate() {}\n",
                "authenticate",
            ),
            (
                "a.py",
                "import os\ndef authenticate():\n    return True\n",
                "authenticate",
            ),
            (
                "a.js",
                "import x from 'x';\nfunction authenticate() { return true; }\n",
                "authenticate",
            ),
            (
                "a.ts",
                "interface Token { value: string }\nfunction authenticate(): boolean { return true; }\n",
                "authenticate",
            ),
            (
                "a.tsx",
                "const authenticate = () => <div>ok</div>;\n",
                "authenticate",
            ),
            (
                "a.go",
                "package main\nfunc authenticate() bool { return true }\n",
                "authenticate",
            ),
            (
                "a.java",
                "class Auth { boolean authenticate() { return true; } }\n",
                "authenticate",
            ),
            (
                "a.c",
                "#include <stdio.h>\nint authenticate(void) { return 1; }\n",
                "authenticate",
            ),
            (
                "a.cpp",
                "namespace auth { class Token {}; int authenticate() { return 1; } }\n",
                "authenticate",
            ),
            (
                "a.cs",
                "using System;\nclass Auth { bool authenticate() { return true; } }\n",
                "authenticate",
            ),
            ("a.rb", "def authenticate\n  true\nend\n", "authenticate"),
            (
                "a.php",
                "<?php function authenticate() { return true; }\n",
                "authenticate",
            ),
            ("a.sh", "authenticate() { echo yes; }\n", "authenticate"),
            (
                "a.ps1",
                "function Authenticate { return $true }\n",
                "Authenticate",
            ),
            (
                "a.lua",
                "function authenticate() return true end\n",
                "authenticate",
            ),
            (
                "a.swift",
                "func authenticate() -> Bool { return true }\n",
                "authenticate",
            ),
            (
                "a.kt",
                "fun authenticate(): Boolean { return true }\n",
                "authenticate",
            ),
            ("a.sql", "CREATE TABLE tokens (id INTEGER);\n", "tokens"),
        ];
        let mut failures = Vec::new();
        for (path, text, name) in fixtures {
            let analysis = analyze(path, text)?;
            if !analysis
                .facts
                .iter()
                .any(|f| f.kind == "definition" && f.name == name)
            {
                let mut parser = Parser::new();
                parser.set_language(&profile(path).expect("profile").grammar)?;
                failures.push(format!(
                    "{path}: facts={:?}; tree={}",
                    analysis.facts,
                    parser
                        .parse(text, None)
                        .expect("tree")
                        .root_node()
                        .to_sexp()
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        Ok(())
    }

    #[test]
    fn structured_profiles_extract_keys_and_source_spans() -> anyhow::Result<()> {
        for (path, text, name) in [
            ("a.json", "{\"timeout\": 3}", "timeout"),
            ("a.yaml", "timeout: 3\n", "timeout"),
            ("a.toml", "timeout = 3\n", "timeout"),
            ("a.html", "<section id=\"main\">Hello</section>", "section"),
            ("a.css", ".main { color: red; }", ".main"),
        ] {
            let analysis = analyze(path, text)?;
            assert!(
                analysis
                    .facts
                    .iter()
                    .any(|f| f.kind == "key" && f.name.contains(name)),
                "{path}: {:?}",
                analysis.facts
            );
            assert!(
                analysis
                    .facts
                    .iter()
                    .all(|f| f.start_line > 0 && f.end_line >= f.start_line)
            );
        }
        Ok(())
    }

    #[test]
    fn comments_and_strings_do_not_become_definitions() -> anyhow::Result<()> {
        let result = analyze(
            "a.py",
            "# def fake():\ntext = 'def fake(): pass'\ndef real():\n    pass\n",
        )?;
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|f| f.kind == "definition")
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            vec!["real"]
        );
        let broken = analyze("a.rs", "fn broken( {\n")?;
        assert_eq!(broken.status, "partial-syntax");
        assert!(!broken.facts.iter().any(|f| f.name == "broken"));
        Ok(())
    }
}
