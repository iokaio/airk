// SPDX-License-Identifier: Apache-2.0
//! Deterministic early evidence ordering; every eligible path is still visited.

pub fn agent_path_priority(path: &str) -> u8 {
    let path = path.to_ascii_lowercase();
    let parts: Vec<_> = path.split('/').collect();
    let name = parts.last().copied().unwrap_or("");
    if parts.iter().any(|part| {
        matches!(
            *part,
            "vendor" | "vendored" | "generated" | "fixtures" | "testdata"
        )
    }) {
        return 5;
    }
    if name.starts_with("readme")
        || matches!(name, "agents.md" | "claude.md" | "contributing.md")
        || parts
            .iter()
            .any(|part| matches!(*part, "docs" | "doc" | "adr" | "adrs"))
    {
        return 0;
    }
    if matches!(
        name,
        "cargo.toml"
            | "package.json"
            | "pyproject.toml"
            | "go.mod"
            | "cmakelists.txt"
            | "makefile"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "dockerfile"
    ) {
        return 1;
    }
    if parts
        .iter()
        .any(|part| matches!(*part, "test" | "tests" | "__tests__"))
    {
        return 4;
    }
    if matches!(
        name,
        "main.rs" | "lib.rs" | "main.go" | "main.py" | "index.ts" | "index.js"
    ) {
        return 2;
    }
    3
}
