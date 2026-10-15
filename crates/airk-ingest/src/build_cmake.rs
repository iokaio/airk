// SPDX-License-Identifier: Apache-2.0
//! Written build structure; never configures a project or evaluates expressions.
use crate::build_facts::Facts;
use anyhow::{Result, ensure};
use tree_sitter::Parser;

pub fn analyze(path: &str, text: &str) -> Result<crate::syntax::Analysis> {
    let mut output = Facts::new(path, text, "cmake")?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_cmake::LANGUAGE.into())?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("CMake parser returned no tree"))?;
    let mut pending = vec![tree.root_node()];
    let mut count = 0;
    while let Some(node) = pending.pop() {
        count += 1;
        ensure!(count <= 500_000, "CMake exceeds node limit");
        if !node.has_error() && !node.is_missing() {
            if matches!(
                node.kind(),
                "normal_command" | "function_command" | "macro_command"
            ) {
                let mut cursor = node.walk();
                let children: Vec<_> = node.named_children(&mut cursor).collect();
                let command = children
                    .first()
                    .and_then(|n| n.utf8_text(text.as_bytes()).ok())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if let Some(args) = children.iter().find(|n| n.kind() == "argument_list") {
                    let mut cursor = args.walk();
                    let args: Vec<_> = args.named_children(&mut cursor).collect();
                    let first_kind = match command.as_str() {
                        "add_executable" | "add_library" => Some("build_target"),
                        "target_sources"
                        | "target_link_libraries"
                        | "target_include_directories"
                        | "add_dependencies" => Some("target_reference"),
                        "set" | "option" | "unset" => Some("build_variable"),
                        "function" | "macro" => Some("build_function"),
                        _ => None,
                    };
                    if let (Some(kind), Some(arg)) = (first_kind, args.first()) {
                        output.node(*arg, kind)?;
                    }
                    let rest_kind = match command.as_str() {
                        "add_executable" | "add_library" | "target_sources" => {
                            Some("source_argument")
                        }
                        "target_link_libraries" | "add_dependencies" => Some("dependency_argument"),
                        "target_include_directories" => Some("include_directory"),
                        "include" | "add_subdirectory" => Some("include_argument"),
                        "find_package" => Some("package_argument"),
                        "set" | "option" => Some("variable_value"),
                        _ => None,
                    };
                    if let Some(kind) = rest_kind {
                        for arg in args.iter().skip(usize::from(first_kind.is_some())) {
                            output.node(*arg, kind)?;
                        }
                    }
                }
            } else if node.kind() == "variable_ref" {
                output.node(node, "variable_expression")?;
            }
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        pending.extend(children.into_iter().rev());
    }
    Ok(output.finish(tree.root_node().has_error()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_commands_and_expression_spans() -> Result<()> {
        let text = "# add_library(fake ghost.cc)\r\nADD_LIBRARY(beacon STATIC src/beacon.cc)\r\ntarget_link_libraries(beacon PRIVATE ${LIBS} $<CONFIG:Debug>)\nset(NOTE [=[add_library(fake nope)]=])\n";
        let result = analyze("CMakeLists.txt", text)?;
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "build_target" && f.name == "beacon" && f.start_line == 2)
        );
        assert!(
            !result
                .facts
                .iter()
                .any(|f| f.kind == "build_target" && f.name == "fake")
        );
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "dependency_argument" && f.name == "${LIBS}")
        );
        for fact in result.facts {
            let span = fact.code.unwrap().span.unwrap();
            assert_eq!(text[span.start_byte..span.end_byte].trim(), fact.name);
        }
        Ok(())
    }
    #[test]
    fn malformed_is_partial_and_names_are_bounded() -> Result<()> {
        assert_eq!(
            analyze("x.cmake", "add_library(broken\n")?.status,
            "partial-syntax"
        );
        let original = "x".repeat(4097);
        let source = format!("set({original} value)");
        let result = analyze("x.cmake", &source)?;
        let fact = result
            .facts
            .iter()
            .find(|f| f.kind == "build_variable")
            .unwrap();
        assert_eq!(fact.name.len(), 4096);
        let code = fact.code.as_ref().unwrap();
        assert!(code.details_truncated);
        let span = code.span.as_ref().unwrap();
        assert_eq!(&source[span.start_byte..span.end_byte], original);
        Ok(())
    }
}
