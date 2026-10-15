// SPDX-License-Identifier: Apache-2.0
//! Make rules and recipes are written evidence, never executable instructions.
use crate::build_facts::Facts;
use anyhow::{Result, ensure};
use tree_sitter::Parser;

pub fn analyze(path: &str, text: &str) -> Result<crate::syntax::Analysis> {
    let mut output = Facts::new(path, text, "make")?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_make::LANGUAGE.into())?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("Make parser returned no tree"))?;
    let mut pending = vec![tree.root_node()];
    let mut count = 0;
    let mut partial = tree.root_node().has_error();
    while let Some(node) = pending.pop() {
        count += 1;
        ensure!(count <= 500_000, "Make exceeds node limit");
        if !node.has_error() && !node.is_missing() {
            match node.kind() {
                "rule" => {
                    let mut cursor = node.walk();
                    for targets in node
                        .named_children(&mut cursor)
                        .filter(|n| n.kind() == "targets")
                    {
                        let mut cursor = targets.walk();
                        for target in targets
                            .named_children(&mut cursor)
                            .filter(|n| n.kind() != "comment")
                        {
                            output.node(target, "build_target")?;
                        }
                    }
                    for (field, kind) in [
                        ("target", "target_pattern"),
                        ("normal", "prerequisite"),
                        ("order_only", "order_only_prerequisite"),
                        ("prerequisite", "prerequisite"),
                    ] {
                        if let Some(value) = node.child_by_field_name(field) {
                            output.node(value, kind)?;
                        }
                    }
                }
                "variable_assignment"
                | "shell_assignment"
                | "RECIPEPREFIX_assignment"
                | "VPATH_assignment"
                | "define_directive" => {
                    if node.kind() == "RECIPEPREFIX_assignment" {
                        partial = true;
                        output.push(
                            node.start_byte(),
                            node.end_byte(),
                            "extraction_diagnostic",
                            "make_unsupported_prefix",
                            "Custom recipe prefixes are not interpreted by this grammar",
                        )?;
                    }
                    if let Some(name) = node.child_by_field_name("name") {
                        output.node(name, "build_variable")?;
                    }
                    if let Some(value) = node.child_by_field_name("value") {
                        output.node(value, "variable_value")?;
                    }
                }
                "include_directive" => {
                    if let Some(value) = node.child_by_field_name("filenames") {
                        output.node(value, "include_argument")?;
                    }
                }
                "recipe" => output.node(node, "recipe")?,
                "variable_reference"
                | "automatic_variable"
                | "substitution_reference"
                | "function_call"
                | "shell_function" => output.node(node, "variable_expression")?,
                _ => {}
            }
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        pending.extend(children.into_iter().rev());
    }
    Ok(output.finish(partial))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_targets_patterns_and_custom_prefix_limits() -> Result<()> {
        let result = analyze(
            "Makefile",
            "first second: %.o | output\n\t@echo unchanged\n",
        )?;
        for name in ["first", "second"] {
            assert!(
                result
                    .facts
                    .iter()
                    .any(|f| f.kind == "build_target" && f.name == name)
            );
        }
        let custom = analyze("Makefile", ".RECIPEPREFIX := >\nbeacon:\n>echo untouched\n")?;
        assert_eq!(custom.status, "partial-syntax");
        assert!(
            custom
                .facts
                .iter()
                .any(|f| f.kind == "extraction_diagnostic")
        );
        Ok(())
    }
    #[test]
    fn recipes_and_shell_expansions_remain_written() -> Result<()> {
        let text = "# ghost: phantom\nSOURCES := beacon.c\nGENERATED != echo untouched\nbeacon: $(SOURCES) | output\n\t@echo 'fake: dependency'\ninclude local.mk\n";
        let result = analyze("Makefile", text)?;
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "build_target" && f.name == "beacon")
        );
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "order_only_prerequisite" && f.name.contains("output"))
        );
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "recipe" && f.name.contains("fake:"))
        );
        assert!(
            !result
                .facts
                .iter()
                .any(|f| f.kind == "build_target" && f.name.contains("fake"))
        );
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "include_argument" && f.name == "local.mk")
        );
        Ok(())
    }
    #[test]
    fn bounded_display_preserves_complete_target_span() -> Result<()> {
        let original = "x".repeat(4097);
        let source = format!("{original}: source\n");
        let result = analyze("Makefile", &source)?;
        let fact = result
            .facts
            .iter()
            .find(|f| f.kind == "build_target")
            .unwrap();
        assert_eq!(fact.name.len(), 4096);
        let code = fact.code.as_ref().unwrap();
        assert!(code.details_truncated);
        let span = code.span.as_ref().unwrap();
        assert_eq!(&source[span.start_byte..span.end_byte], original);
        Ok(())
    }
}
