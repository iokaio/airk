// SPDX-License-Identifier: Apache-2.0
//! Written GPU/assembly syntax without compilation, execution or ISA inference.
mod assembly;
mod cuda;
mod glsl;
mod wgsl;
use crate::{build_facts::Facts, syntax::Analysis};
use airk_core::{
    CodeSpan, EvidenceBasis,
    location::{ByteSpan, HostBudget},
};
use anyhow::{Result, ensure};
use tree_sitter::{Language, Node, Parser};

struct Cue<'tree, 'source> {
    node: Node<'tree>,
    kind: &'static str,
    name: &'source str,
    declaration: bool,
    body: Option<Node<'tree>>,
}
fn cue<'a, 'b>(
    node: Node<'a>,
    kind: &'static str,
    name: &'b str,
    declaration: bool,
) -> Cue<'a, 'b> {
    Cue {
        node,
        kind,
        name,
        declaration,
        body: node.child_by_field_name("body"),
    }
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}
fn named<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}
fn field<'a>(node: Node<'_>, source: &'a str, key: &str) -> Option<&'a str> {
    node.child_by_field_name(key).map(|node| text(node, source))
}

pub(crate) fn analyze(path: &str, source: &str, adapter: &str) -> Result<Analysis> {
    let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
    budget.charge(1, 0, source.len(), 0)?;
    analyze_budgeted(path, source, adapter, &mut budget)
}
/// Caller admits source/map bytes once; siblings share this work/fact/hash budget.
pub(crate) fn analyze_budgeted(
    path: &str,
    source: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    ensure!(
        source.len() <= crate::MAX_STRUCTURED_BYTES,
        "GPU/assembly source byte limit"
    );
    let grammar: Language = match adapter {
        "cuda" => tree_sitter_cuda::LANGUAGE.into(),
        "wgsl" => tree_sitter_wgsl::LANGUAGE.into(),
        "glsl" => tree_sitter_glsl::LANGUAGE_GLSL.into(),
        "assembly" | "assembly-x86-att" | "assembly-x86-intel" | "assembly-aarch64" => {
            tree_sitter_asm::LANGUAGE.into()
        }
        _ => anyhow::bail!("unsupported GPU/assembly adapter"),
    };
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&grammar)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("GPU/assembly parser returned no tree"))?;
    let mut output = Facts::new(path, source, adapter)?;
    let mut pending = vec![(tree.root_node(), None)];
    let mut partial = tree.root_node().has_error()
        || adapter == "assembly"
        || (adapter.starts_with("assembly") && path.ends_with(".S"));
    while let Some((node, owner)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let mut next_owner = owner;
        let kind = node.kind();
        if kind.starts_with("preproc")
            || (adapter == "wgsl" && kind == "import_path")
            || kind.contains("compiler_directive")
            || kind.contains("macro")
            || kind.contains("generate")
            || kind.starts_with("yul_")
        {
            partial = true;
        }
        // Do not discover declarations inside comments, strings or macro bodies.
        if matches!(
            kind,
            "comment"
                | "line_comment"
                | "block_comment"
                | "string"
                | "string_literal"
                | "double_quoted_string"
                | "text_macro_definition"
                | "text_macro_usage"
                | "preproc_arg"
                | "char_literal"
        ) {
            continue;
        }
        let broken_tail = node.next_named_sibling().is_some_and(|next| {
            next.is_error() && source[node.end_byte()..next.start_byte()].trim().is_empty()
        });
        if !node.has_error() && !node.is_missing() && !broken_tail {
            let selected = match adapter {
                "cuda" => cuda::select(node, source),
                "wgsl" => wgsl::select(node, source),
                "glsl" => glsl::select(node, source),
                _ => assembly::select(node, source, adapter),
            };
            for selected in selected {
                if selected.name.trim().is_empty() {
                    continue;
                }
                budget.charge(0, 1, 0, 0)?;
                let index = output.facts.len();
                output.push(
                    selected.node.start_byte(),
                    selected.node.end_byte(),
                    if selected.declaration {
                        "definition"
                    } else {
                        selected.kind
                    },
                    selected.node.kind(),
                    selected.name,
                )?;
                let owner_id =
                    owner.map(|i: usize| output.facts[i].code.as_ref().unwrap().symbol_id.clone());
                let mut scope = owner.map_or_else(Vec::new, |i| {
                    output.facts[i].code.as_ref().unwrap().scope.clone()
                });
                let truncated =
                    owner.is_some_and(|i| output.facts[i].code.as_ref().unwrap().details_truncated);
                if let Some(i) = owner {
                    scope.push(output.facts[i].name.clone());
                }
                ensure!(scope.len() <= 128, "GPU/assembly owner depth limit");
                let signature = selected
                    .body
                    .filter(|body| body.start_byte() > selected.node.start_byte())
                    .map(|body| ByteSpan {
                        start: selected.node.start_byte(),
                        end: body.start_byte(),
                    });
                let fingerprints = if selected.declaration {
                    let mut values = crate::syntax::declaration_fingerprints(
                        source,
                        selected.name,
                        &scope.iter().map(String::as_str).collect::<Vec<_>>(),
                        signature,
                        selected.body,
                        None,
                        budget,
                    )?;
                    if truncated {
                        values.qualified_name = None;
                    }
                    Some(values)
                } else {
                    None
                };
                let code = output.facts[index].code.as_mut().unwrap();
                if selected.declaration {
                    code.modifiers.push(selected.kind.into());
                }
                code.owner_id = owner_id;
                code.scope = scope;
                code.details_truncated |= truncated;
                code.fingerprints = fingerprints;
                code.signature_span = signature.map(|span| CodeSpan {
                    start_byte: span.start,
                    end_byte: span.end,
                });
                code.basis = EvidenceBasis::Syntax;
                code.producer = format!("airk-tree-sitter/{adapter}");
                code.producer_version = "specialist-gpu-facts/1".into();
                code.coverage="Selected written GPU/assembly syntax only; no compilation, execution, graphics validation, preprocessor expansion or resolved bindings. Assembly ISA interpretation requires an explicit profile; unsupported body/documentation dimensions remain unknown".into();
                if matches!(
                    selected.kind,
                    "call"
                        | "kernel_launch"
                        | "branch"
                        | "import"
                        | "inheritance"
                        | "instantiation"
                        | "include"
                        | "use"
                        | "library"
                ) {
                    code.target = Some(selected.name.chars().take(1024).collect());
                }
                if selected.declaration && selected.node == node {
                    next_owner = Some(index);
                }
            }
        }
        for i in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "GPU/assembly pending node limit");
            if let Some(child) = node.named_child(i) {
                pending.push((child, next_owner));
            }
        }
    }
    ensure!(
        serde_json::to_vec(&output.facts)?.len() <= 16_777_216,
        "GPU/assembly evidence byte limit"
    );
    let mut result = output.finish(partial);
    if !partial {
        result.status = "syntax".into();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glsl_sibling_declarators_roundtrip_payload_binding() -> Result<()> {
        use airk_core::binding::{ArtifactBinding, bind_payload};
        use sha2::{Digest, Sha256};
        let path = "original.vert";
        let source = "// Original 🌱\r\nuniform float first, second;\r\nlayout(std140) uniform Ledger { vec4 amount; } ledger;\r\nvoid main() { float local_first, local_second; }\r\n";
        let extracted = crate::documents::extract(path, source.as_bytes())?;
        let source_hash =
            airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(source.as_bytes()).into());
        let artifact = airk_core::Artifact {
            path: path.into(),
            content_hash: source_hash.to_string(),
            bytes: source.len(),
            chunks: extracted.chunks,
            language: extracted.language,
            extraction_method: extracted.method,
            parse_status: extracted.parse_status,
            facts: extracted.facts,
            retained: None,
        };
        let profile = crate::adapters::legacy::legacy_profile(&artifact)?;
        let payload =
            crate::adapters::legacy::convert_native_legacy(&artifact, source.as_bytes(), &profile)?;
        let bound = bind_payload(
            &payload,
            &profile,
            ArtifactBinding {
                path,
                source_hash,
                source_bytes: source.len(),
            },
        )?;
        assert_eq!(
            serde_json::to_value(&bound)?,
            serde_json::to_value(&artifact)?
        );
        for name in ["first", "second"] {
            assert!(
                artifact
                    .facts
                    .iter()
                    .find(|f| f.name == name)
                    .unwrap()
                    .code
                    .as_ref()
                    .unwrap()
                    .owner_id
                    .is_none()
            );
        }
        for name in ["local_first", "local_second"] {
            let fact = artifact.facts.iter().find(|f| f.name == name).unwrap();
            let owner = fact.code.as_ref().unwrap().owner_id.as_ref().unwrap();
            assert!(
                artifact
                    .facts
                    .iter()
                    .any(|f| f.name == "main" && &f.code.as_ref().unwrap().symbol_id == owner)
            );
        }
        Ok(())
    }
    #[test]
    fn original_gpu_and_assembly_rows_preserve_crlf_unicode_spans() -> Result<()> {
        for (adapter, source, names, kinds) in [
            (
                "cuda",
                include_str!("../tests/fixtures/specialist-gpu/original.cu"),
                vec!["record", "submit", "receipts", "Ledger", "bounded"],
                vec!["kernel", "kernel_launch", "launch_bounds", "type"],
            ),
            (
                "wgsl",
                include_str!("../tests/fixtures/specialist-gpu/original.wgsl"),
                vec!["Receipt", "record", "receipt"],
                vec!["stage", "binding"],
            ),
            (
                "glsl",
                include_str!("../tests/fixtures/specialist-gpu/original.vert"),
                vec!["main", "position", "receipt", "Ledger", "uniform"],
                vec![
                    "binding",
                    "interface_block",
                    "storage_qualifier",
                    "directive",
                ],
            ),
            (
                "assembly-x86-att",
                include_str!("../tests/fixtures/specialist-gpu/original.s"),
                vec!["record"],
                vec!["section", "call", "branch"],
            ),
        ] {
            let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
            let result = analyze("original", &source, adapter)?;
            for name in names {
                assert!(
                    result.facts.iter().any(|f| f.name == name),
                    "{adapter} missing {name}: {:?}",
                    result.facts
                );
            }
            for kind in kinds {
                assert!(
                    result.facts.iter().any(|f| f.kind == kind
                        || f.code.as_ref().is_some_and(|code| code
                            .modifiers
                            .iter()
                            .any(|modifier| modifier == kind))),
                    "{adapter} missing {kind}: {:?}",
                    result.facts
                );
            }
            for fact in result.facts {
                let span = fact.code.as_ref().unwrap().span.as_ref().unwrap();
                assert!(source.get(span.start_byte..span.end_byte).is_some());
                assert_eq!(
                    fact.start_line,
                    source[..span.start_byte]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1
                );
            }
        }
        Ok(())
    }
    #[test]
    fn generic_isa_and_preprocessor_remain_explicitly_uninterpreted() -> Result<()> {
        let asm = analyze("written.s", "record:\ncall submit\n", "assembly")?;
        assert_eq!(asm.status, "partial-syntax");
        assert!(
            !asm.facts
                .iter()
                .any(|f| f.kind == "call" || f.kind == "branch")
        );
        for (adapter, source) in [
            (
                "cuda",
                "// __global__ void Phantom() {}\n#define DECL void Phantom() {}\nvoid real() {}\n",
            ),
            (
                "glsl",
                "// void Phantom() {}\n#define DECL void Phantom() {}\nvoid real() {}\n",
            ),
            (
                "wgsl",
                "// fn Phantom() {}\n#ifdef WRITTEN\nfn real() {}\n#endif\n",
            ),
        ] {
            let result = analyze("original", source, adapter)?;
            assert_eq!(result.status, "partial-syntax");
            assert!(!result.facts.iter().any(|f| f.name == "Phantom"));
        }
        Ok(())
    }
    #[test]
    fn malformed_input_and_shared_fact_limits_fail_closed() -> Result<()> {
        let declarations = analyze("original.glsl", "uniform float first, second;\n", "glsl")?;
        for name in ["first", "second"] {
            assert!(
                declarations
                    .facts
                    .iter()
                    .any(|fact| fact.name == name && fact.kind == "definition")
            );
        }
        let ids = declarations
            .facts
            .iter()
            .filter(|fact| fact.kind == "definition")
            .map(|fact| fact.code.as_ref().unwrap().symbol_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), 2);
        assert_eq!(
            analyze("bad.wgsl", "fn broken( {", "wgsl")?.status,
            "partial-syntax"
        );
        let mut budget = HostBudget::new(5000, 1, 0)?;
        analyze_budgeted("one.cu", "void one() {}", "cuda", &mut budget)?;
        assert!(analyze_budgeted("two.cu", "void two() {}", "cuda", &mut budget).is_err());
        Ok(())
    }
}
