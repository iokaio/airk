# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
import hashlib
import json

root = Path(__file__).resolve().parents[3]
out = Path(__file__).resolve().parent
changes = {}
path = Path("crates/airk-cli/tests/mcp.rs")
before = (root / path).read_text(encoding="utf-8")
start = before.index("fn assert_query_scope_metadata_tool(")
end = before.index("\nimpl Session {", start)
helper = '''fn assert_query_scope_metadata_tool(tools: &[Value], name: &str, transport_project: bool) {
    let tool = tools
        .iter()
        .find(|tool| tool["name"] == name)
        .expect("query scope metadata tool");
    let canonical_catalog = if transport_project {
        airk_mcp::unified::tools()
    } else {
        airk_mcp::local::tools()
    };
    let canonical = canonical_catalog
        .iter()
        .find(|tool| tool.name == name)
        .expect("canonical query scope metadata tool");
    let canonical = serde_json::to_value(canonical).unwrap();
    assert_eq!(tool["inputSchema"], canonical["inputSchema"]);
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    let mut expected_schema = airk_core::combined_reads::query_scope::schema();
    expected_schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    if transport_project {
        let project = &tool["inputSchema"]["properties"]["project"];
        assert_eq!(project["type"], "string");
        assert_eq!(project["pattern"], "^[0-9a-f]{64}$");
        assert!(
            !tool["inputSchema"]["required"]
                .as_array()
                .is_some_and(|fields| fields.iter().any(|field| field == "project")),
            "the configured transport binding must remain optional"
        );
        expected_schema["properties"]["project"] = project.clone();
    } else {
        assert!(tool["inputSchema"]["properties"].get("project").is_none());
    }
    // The optional configured transport binding is the complete public delta.
    assert_eq!(tool["inputSchema"], expected_schema);
    for field in ["project_ids", "index_ids", "verification", "response_mode"] {
        assert!(
            tool["inputSchema"]["properties"].get(field).is_none(),
            "discovery must not advertise {field}"
        );
    }
}
'''
after = before[:start] + helper + before[end:]
after = after.replace('assert_query_scope_metadata_tool(tools, "query_scope");', 'assert_query_scope_metadata_tool(tools, "query_scope", true);')
after = after.replace('assert_query_scope_metadata_tool(catalog["tools"].as_array().unwrap(), "airk_query_scope");', 'assert_query_scope_metadata_tool(\n        catalog["tools"].as_array().unwrap(),\n        "airk_query_scope",\n        false,\n    );')
assert after.count('assert_query_scope_metadata_tool(tools, "query_scope", true);') == 2
changes[path] = (before, after)

path = Path("crates/airk-cli/tests/service_runtime.rs")
before = (root / path).read_text(encoding="utf-8")
old = '''        ensure!(
            scope["annotations"]["readOnlyHint"] == true
                && scope["inputSchema"]["additionalProperties"] == false
                && scope["inputSchema"]["properties"]
                    == airk_core::combined_reads::query_scope::schema()["properties"],
            "bridge query scope discovery diverged from its closed read-only metadata contract"
        );'''
new = '''        let canonical_catalog = if unified {
            airk_mcp::unified::tools()
        } else {
            airk_mcp::local::tools()
        };
        let canonical = canonical_catalog
            .iter()
            .find(|tool| tool.name == scope_name)
            .context("canonical query scope metadata tool missing")?;
        let canonical = serde_json::to_value(canonical)?;
        ensure!(
            scope["annotations"]["readOnlyHint"] == true
                && scope["inputSchema"]["additionalProperties"] == false
                && scope["inputSchema"] == canonical["inputSchema"],
            "bridge query scope discovery diverged from its closed read-only metadata contract"
        );
        let mut expected_schema = airk_core::combined_reads::query_scope::schema();
        expected_schema["$schema"] =
            serde_json::json!("https://json-schema.org/draft/2020-12/schema");
        if unified {
            let project = &scope["inputSchema"]["properties"]["project"];
            ensure!(
                project["type"] == "string"
                    && project["pattern"] == "^[0-9a-f]{64}$"
                    && !scope["inputSchema"]["required"]
                        .as_array()
                        .is_some_and(|fields| fields.iter().any(|field| field == "project")),
                "public query scope must expose only an optional configured transport binding"
            );
            expected_schema["properties"]["project"] = project.clone();
        }
        ensure!(
            scope["inputSchema"] == expected_schema,
            "query scope must preserve the complete native schema plus only its public transport binding"
        );
        for field in ["project_ids", "index_ids", "verification", "response_mode"] {
            ensure!(
                scope["inputSchema"]["properties"].get(field).is_none(),
                "query scope must not advertise {field}"
            );
        }'''
assert before.count(old) == 1
after = before.replace(old, new)
changes[path] = (before, after)

patch = []
inputs = []
for path, (before, after) in changes.items():
    proposed = out / "proposed" / path
    proposed.parent.mkdir(parents=True, exist_ok=True)
    proposed.write_text(after, encoding="utf-8", newline="\n")
    patch.extend(difflib.unified_diff(before.splitlines(True), after.splitlines(True), fromfile="a/" + path.as_posix(), tofile="b/" + path.as_posix()))
    inputs.append({"path": path.as_posix(), "sha256": hashlib.sha256((root / path).read_bytes()).hexdigest()})
(out / "catalog-tests.patch").write_text("".join(patch), encoding="utf-8", newline="\n")
(out / "catalog-tests.inputs.json").write_text(json.dumps(inputs, indent=2) + "\n", encoding="utf-8", newline="\n")
print(json.dumps({"patch": str(out / "catalog-tests.patch"), "preimages": inputs}, indent=2))
