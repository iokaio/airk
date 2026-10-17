// SPDX-License-Identifier: Apache-2.0
//! Bounded written language evidence. No compiler, macro, or runtime resolution.
mod dart;
mod elixir;
mod objc;
mod scala;

use crate::build_facts::Facts;
use airk_core::{EvidenceBasis, location::HostBudget};
use anyhow::{Result, ensure};
use tree_sitter::{Language, Node, Parser};

pub(crate) struct Cue {
    kind: &'static str,
    name: String,
    declaration: bool,
    structural: bool,
}

fn cue(kind: &'static str, name: &str, declaration: bool) -> Option<Cue> {
    (!name.trim().is_empty()).then(|| Cue {
        kind,
        name: name.trim().into(),
        declaration,
        structural: false,
    })
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}
fn named<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}
fn field<'a>(node: Node<'_>, source: &'a str, field: &str) -> Option<&'a str> {
    node.child_by_field_name(field)
        .map(|child| text(child, source))
}

pub(crate) fn analyze(path: &str, source: &str, adapter: &str) -> Result<crate::syntax::Analysis> {
    ensure!(
        source.len() <= crate::MAX_STRUCTURED_BYTES,
        "language source exceeds byte limit"
    );
    let grammar: Language = match adapter {
        "dart" => tree_sitter_dart::LANGUAGE.into(),
        "scala" => tree_sitter_scala::LANGUAGE.into(),
        "elixir" => tree_sitter_elixir::LANGUAGE.into(),
        "objc" | "objcpp" => tree_sitter_objc::LANGUAGE.into(),
        _ => anyhow::bail!("unknown languages A adapter"),
    };
    let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
    budget.charge(1, 0, source.len(), 0)?;
    let mut parser = Parser::new();
    parser.set_language(&grammar)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("language parser returned no tree"))?;
    let mut output = Facts::new(path, source, adapter)?;
    let mut islands = Vec::new();
    let mut pending = vec![(tree.root_node(), None)];
    let mut partial = tree.root_node().has_error() || adapter == "objcpp";
    while let Some((node, owner)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        if adapter == "elixir" && elixir::quoted(node, source) {
            partial = true;
            continue;
        }
        if adapter == "objcpp" && objc::island(node) {
            islands.push(node.byte_range());
        }
        // An Objective-C declaration with unsupported mixed syntax does not
        // lend authority to recovered fragments inside it.
        if matches!(adapter, "objc" | "objcpp") && objc::island(node) && node.has_error() {
            continue;
        }
        let mut next_owner = owner;
        let broken_tail = node.next_named_sibling().is_some_and(|next| {
            next.is_error() && source[node.end_byte()..next.start_byte()].trim().is_empty()
        });
        if !node.has_error() && !node.is_missing() && !broken_tail {
            let selected = match adapter {
                "dart" => dart::select(node, source),
                "scala" => scala::select(node, source),
                "elixir" => elixir::select(node, source),
                _ => objc::select(node, source),
            };
            // In dual mode the C++ pass owns calls outside admitted Objective-C
            // declarations; do not emit a second unowned C recovery fragment.
            if let Some(cue) = selected
                .filter(|cue| !(adapter == "objcpp" && cue.kind == "call" && owner.is_none()))
            {
                next_owner = emit(&mut output, node, cue, owner, adapter, source, &mut budget)?;
            }
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "language pending node limit");
            if let Some(child) = node.named_child(index) {
                pending.push((child, next_owner));
            }
        }
    }
    if adapter == "objcpp" {
        objc::cpp_pass(source, &mut output, &mut budget, islands)?;
    }
    ensure!(
        serde_json::to_vec(&output.facts)?.len() <= 16_777_216,
        "language evidence byte limit"
    );
    let mut result = output.finish(partial);
    if !partial {
        result.status = "syntax".into();
    }
    Ok(result)
}

fn emit(
    output: &mut Facts<'_>,
    node: Node<'_>,
    cue: Cue,
    owner: Option<usize>,
    adapter: &str,
    source: &str,
    budget: &mut HostBudget,
) -> Result<Option<usize>> {
    budget.charge(0, 1, 0, 0)?;
    let index = output.facts.len();
    output.push(
        node.start_byte(),
        node.end_byte(),
        cue.kind,
        node.kind(),
        &cue.name,
    )?;
    if output.facts.len() == index {
        return Ok(owner);
    }
    let owner_id = owner.map(|index| output.facts[index].code.as_ref().unwrap().symbol_id.clone());
    let mut scopes = owner.map_or_else(Vec::new, |index| {
        output.facts[index].code.as_ref().unwrap().scope.clone()
    });
    let inherited_truncation =
        owner.is_some_and(|index| output.facts[index].code.as_ref().unwrap().details_truncated);
    if let Some(owner) = owner {
        scopes.push(output.facts[owner].name.clone());
    }
    ensure!(scopes.len() <= 128, "language owner depth exceeds limit");
    let body = node
        .child_by_field_name("body")
        .or_else(|| named(node, "compound_statement"));
    let signature = body
        .map(|body| airk_core::location::ByteSpan {
            start: node.start_byte(),
            end: body.start_byte(),
        })
        .or_else(|| {
            matches!(
                node.kind(),
                "method_declaration" | "function_declaration" | "type_definition"
            )
            .then_some(airk_core::location::ByteSpan {
                start: node.start_byte(),
                end: node.end_byte(),
            })
        });
    let mut fingerprints = if cue.declaration && !cue.structural && cue.kind == "definition" {
        Some(crate::syntax::declaration_fingerprints(
            source,
            &cue.name,
            &scopes.iter().map(String::as_str).collect::<Vec<_>>(),
            signature,
            body,
            None,
            budget,
        )?)
    } else {
        None
    };
    if inherited_truncation && let Some(fingerprints) = &mut fingerprints {
        fingerprints.qualified_name = None;
    }
    let display_name = output.facts[index].name.clone();
    let code = output.facts[index].code.as_mut().unwrap();
    code.owner_id = owner_id;
    code.scope = scopes;
    code.details_truncated |= inherited_truncation;
    code.fingerprints = fingerprints;
    code.signature_span = signature.map(|span| airk_core::CodeSpan {
        start_byte: span.start,
        end_byte: span.end,
    });
    code.basis = if cue.structural {
        EvidenceBasis::StructuralRule
    } else {
        EvidenceBasis::Syntax
    };
    code.producer = if cue.structural {
        "airk-language-structure/elixir".into()
    } else {
        format!("airk-tree-sitter/{adapter}")
    };
    code.producer_version = "general-facts/1".into();
    code.coverage = if adapter == "objcpp" {
        "Partial independent Objective-C/C++ parses; only clean nonoverlapping written constructs; no mixed-language compiler interpretation or resolution"
    } else {
        "Selected original grammar constructs; no compiler inference, macro execution or resolved references; structural macro declaration continuity and documentation dimensions unsupported"
    }.into();
    if matches!(
        cue.kind,
        "call" | "message" | "import" | "use" | "require" | "alias"
    ) {
        code.target = Some(display_name);
    }
    Ok(if cue.declaration { Some(index) } else { owner })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DART: &str = include_str!("../tests/fixtures/languages-a/original.dart");
    const SCALA: &str = include_str!("../tests/fixtures/languages-a/original.scala");
    const ELIXIR: &str = include_str!("../tests/fixtures/languages-a/original.ex");
    const OBJC: &str = include_str!("../tests/fixtures/languages-a/original.m");
    const OBJCPP: &str = include_str!("../tests/fixtures/languages-a/original.mm");

    fn has(result: &crate::syntax::Analysis, kind: &str, name: &str) {
        assert!(
            result
                .facts
                .iter()
                .any(|fact| fact.kind == kind && fact.name.contains(name)),
            "missing {kind} {name}: {:?}",
            result
                .facts
                .iter()
                .map(|f| (&f.kind, &f.name, &f.syntax_kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn dart_libraries_parts_declarations_fields_and_calls_keep_original_spans() -> Result<()> {
        let source = DART.replace("\r\n", "\n").replace('\n', "\r\n");
        let result = analyze("original.dart", &source, "dart")?;
        assert_eq!(result.status, "syntax");
        for (kind, name) in [
            ("library", "archive"),
            ("import", "dart:math"),
            ("part", "archive_part"),
            ("definition", "Archive"),
            ("definition", "Named"),
            ("definition", "Display"),
            ("definition", "State"),
            ("definition", "render"),
            ("field", "title"),
            ("call", "print"),
        ] {
            has(&result, kind, name);
        }
        verify(&source, &result);
        Ok(())
    }

    #[test]
    fn scala_braced_and_indented_dialects_are_independently_qualified() -> Result<()> {
        let result = analyze("original.scala", SCALA, "scala")?;
        assert_eq!(result.status, "syntax");
        for (kind, name) in [
            ("package", "archive"),
            ("import", "ListBuffer"),
            ("definition", "Named"),
            ("definition", "Archive"),
            ("definition", "Main"),
            ("definition", "render"),
            ("field", "caption"),
            ("call", "println"),
        ] {
            has(&result, kind, name);
        }
        let modern = "enum State:\n  case Ready, Done\nobject Modern:\n  def render(value: Int): Int =\n    value + 1\n";
        let result = analyze("modern.sc", modern, "scala")?;
        assert_eq!(result.status, "syntax");
        has(&result, "definition", "State");
        has(&result, "definition", "render");
        verify(modern, &result);
        Ok(())
    }

    #[test]
    fn elixir_literal_macro_heads_guards_arity_and_quote_exclusion() -> Result<()> {
        let result = analyze("original.ex", ELIXIR, "elixir")?;
        for (kind, name) in [
            ("definition", "Archive"),
            ("definition", "render/1"),
            ("definition", "stamp/1"),
            ("alias", "String"),
            ("import", "Enum"),
            ("use", "GenServer"),
            ("require", "Logger"),
            ("call", "String.upcase"),
            ("guard", "is_binary"),
        ] {
            has(&result, kind, name);
        }
        assert!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "definition")
                .all(|fact| fact.code.as_ref().unwrap().basis == EvidenceBasis::StructuralRule)
        );
        let quoted = analyze(
            "quoted.ex",
            "quote do\n def phantom(value), do: value\nend\n# def fake(), do: 1\n\"def hidden(), do: 2\"\n",
            "elixir",
        )?;
        assert_eq!(quoted.status, "partial-syntax");
        assert!(!quoted.facts.iter().any(|fact| fact.kind == "definition"));
        Ok(())
    }

    #[test]
    fn objc_selectors_categories_properties_and_messages_are_written_evidence() -> Result<()> {
        let result = analyze("original.m", OBJC, "objc")?;
        assert_eq!(result.status, "syntax");
        for (kind, name) in [
            ("import", "Foundation"),
            ("definition", "Named"),
            ("definition", "Archive"),
            ("definition", "Archive(Display)"),
            ("definition", "render:with:"),
            ("property", "caption"),
            ("message", "self render:"),
        ] {
            has(&result, kind, name);
        }
        verify(OBJC, &result);
        Ok(())
    }

    #[test]
    fn objective_cpp_retains_disjoint_template_namespace_and_objc_islands_as_partial() -> Result<()>
    {
        let result = analyze("original.mm", OBJCPP, "objcpp")?;
        assert_eq!(result.status, "partial-syntax");
        has(&result, "definition", "archive");
        has(&result, "template", "typename");
        has(&result, "definition", "Archive");
        has(&result, "definition", "title");
        let unique: std::collections::BTreeSet<_> = result
            .facts
            .iter()
            .map(|f| {
                let c = f.code.as_ref().unwrap();
                (&c.symbol_id, &f.kind)
            })
            .collect();
        assert_eq!(unique.len(), result.facts.len());
        verify(OBJCPP, &result);
        Ok(())
    }

    #[test]
    fn comments_strings_errors_and_limits_never_fabricate_declarations() -> Result<()> {
        for (adapter, source) in [
            (
                "dart",
                "// class Phantom {}\nconst caption = 'class Hidden {}';\nclass Broken {",
            ),
            (
                "scala",
                "// class Phantom\nval caption = \"class Hidden\"\nclass Broken {",
            ),
            (
                "objc",
                "// @interface Phantom @end\nchar *caption = \"@interface Hidden @end\";\n@interface Broken",
            ),
        ] {
            let result = analyze("negative", source, adapter)?;
            assert_eq!(result.status, "partial-syntax");
            assert!(!result.facts.iter().any(|f| f.kind == "definition"
                && (f.name.contains("Phantom")
                    || f.name.contains("Hidden")
                    || f.name.contains("Broken"))));
        }
        assert!(
            analyze(
                "large.dart",
                &" ".repeat(crate::MAX_STRUCTURED_BYTES + 1),
                "dart"
            )
            .is_err()
        );
        assert!(analyze("many.scala", &"val value = 1\n".repeat(10_001), "scala").is_err());
        Ok(())
    }

    #[test]
    fn written_calls_and_messages_keep_exact_targets_without_literal_injection() -> Result<()> {
        for (adapter, source, kind, target) in [
            (
                "dart",
                "void render() { print('phantom()'); } // phantom()\n",
                "call",
                "print",
            ),
            (
                "scala",
                "object View { def render(): Unit = println(\"phantom()\") } // phantom()\n",
                "call",
                "println",
            ),
            (
                "elixir",
                "defmodule View do\n def render(value), do: String.upcase(value)\n # Phantom.run()\nend\n",
                "call",
                "String.upcase",
            ),
            (
                "objc",
                "@implementation View\n- (void)render { [self show:@\"phantom()\"]; }\n@end\n",
                "message",
                "[self show:@\"phantom()\"]",
            ),
            (
                "objcpp",
                "namespace view { int render(int value) { return transform(value); } }\n@interface View\n- (void)show;\n@end\n",
                "call",
                "transform",
            ),
        ] {
            let result = analyze("written", source, adapter)?;
            let fact = result
                .facts
                .iter()
                .find(|fact| fact.kind == kind && fact.name == target)
                .expect("selected original call/message");
            let code = fact.code.as_ref().unwrap();
            assert_eq!(code.target.as_deref(), Some(target));
            assert_eq!(code.basis, EvidenceBasis::Syntax);
            let span = code.span.as_ref().unwrap();
            assert!(source[span.start_byte..span.end_byte].contains(target));
            assert!(
                code.owner_id.is_some(),
                "written containment remains available"
            );
            assert!(!result.facts.iter().any(|fact| fact.kind == "call"
                && (fact.name == "phantom" || fact.name == "Phantom.run")));
        }
        let objc = analyze("written.m", OBJC, "objc")?;
        has(&objc, "call", "NSLog");
        Ok(())
    }

    fn verify(source: &str, result: &crate::syntax::Analysis) {
        for fact in &result.facts {
            let code = fact.code.as_ref().unwrap();
            let span = code.span.as_ref().unwrap();
            assert!(source.get(span.start_byte..span.end_byte).is_some());
            assert_eq!(
                fact.start_line,
                source[..span.start_byte]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1
            );
            if code.basis == EvidenceBasis::StructuralRule {
                assert!(code.fingerprints.is_none());
            }
            if let Some(owner) = &code.owner_id {
                assert!(
                    result
                        .facts
                        .iter()
                        .any(
                            |other| other.code.as_ref().is_some_and(|c| &c.symbol_id == owner
                                && c.span
                                    .as_ref()
                                    .is_some_and(|s| s.start_byte <= span.start_byte
                                        && s.end_byte >= span.end_byte))
                        )
                );
            }
        }
    }

    #[test]
    fn complete_native_fingerprints_survive_shifts_and_separate_body_signature_name() -> Result<()>
    {
        fn fingerprints(source: &str) -> airk_core::continuity::CodeFingerprints {
            analyze("x.dart", source, "dart")
                .unwrap()
                .facts
                .into_iter()
                .find(|f| f.kind == "definition")
                .unwrap()
                .code
                .unwrap()
                .fingerprints
                .unwrap()
        }
        let source = "String render(String value) { return value + 'café'; }";
        let original = fingerprints(source);
        let shifted = fingerprints(&format!("// documentation unknown\n\n{source}"));
        assert_eq!(
            serde_json::to_value(&original)?,
            serde_json::to_value(&shifted)?
        );
        assert!(original.documentation.is_none());
        let body = fingerprints(&source.replace("café", "coffee"));
        assert_eq!(original.signature, body.signature);
        assert_ne!(original.body, body.body);
        let signature = fingerprints(&source.replace("String value", "Object value"));
        assert_ne!(original.signature, signature.signature);
        assert_eq!(original.body, signature.body);
        let renamed = fingerprints(&source.replace("render", "display"));
        assert_ne!(original.name, renamed.name);
        assert_eq!(original.body, renamed.body);
        let long = format!("String render() {{ return '{}tail'; }}", "界".repeat(3000));
        assert_ne!(
            fingerprints(&long).body,
            fingerprints(&long.replace("tail", "edited")).body
        );
        Ok(())
    }
}
