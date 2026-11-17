# SPDX-License-Identifier: Apache-2.0
"""Strict declared public-tool projection, never a native result normalizer.

Expected Values come from full native Values plus the explicitly reviewed
owning adapter and bundled UI definitions. Captured public Values never define
what is allowed. Unknown native fields, semantic strings and arrays stay exact.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re

REPO = Path(__file__).resolve().parents[2]
UI_NAMES = [
    ("draft_search", "airk_explorer_suggest_search"),
    ("draft_strategy", "airk_explorer_suggest_strategy"),
    ("draft_plan", "airk_explorer_suggest_plan"),
    ("list_templates", "airk_explorer_templates_list"),
    ("get_template", "airk_explorer_template_get"),
    ("validate_draft", "airk_explorer_validate_draft"),
    ("proposal_status", "airk_explorer_proposal_status"),
    ("get_handoff", "airk_explorer_handoff_get"),
]
PROJECT = {"type": "string", "pattern": "^[0-9a-f]{64}$", "description": "Configured source scope from projects. Required when this connection has no single default; never follows the Explorer dropdown."}
HELP_UI = {"query": {"type": "string", "maxLength": 256}, "topic_id": {"type": "string", "maxLength": 64},
           "detail": {"type": "string", "enum": ["summary", "steps", "full"]},
           "area": {"type": "string", "maxLength": 32}, "limit": {"type": "integer", "minimum": 1, "maximum": 20}}
RUST_STRING = r'"(?:[^"\\]|\\.)*"'


def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def pointer(path, key):
    return path + "/" + str(key).replace("~", "~0").replace("/", "~1")


def difference(left, right, path=""):
    if type(left) is not type(right):
        return {"pointer": path or "/", "kind": "type", "expected": left, "actual": right}
    if isinstance(left, dict):
        for key in sorted(set(left) | set(right)):
            child = pointer(path, key)
            if key not in left:
                return {"pointer": child, "kind": "unexpected_field", "actual": right[key]}
            if key not in right:
                return {"pointer": child, "kind": "missing_field", "expected": left[key]}
            found = difference(left[key], right[key], child)
            if found:
                return found
    elif isinstance(left, list):
        if len(left) != len(right):
            return {"pointer": path, "kind": "array_length", "expected": len(left), "actual": len(right)}
        for number, (expected, actual) in enumerate(zip(left, right)):
            found = difference(expected, actual, pointer(path, number))
            if found:
                return found
    elif left != right:
        return {"pointer": path or "/", "kind": "value", "expected": left, "actual": right}
    return None


def public_text(text):
    for public, old in UI_NAMES:
        text = text.replace(old, public)
    return text.replace("airk_explorer_ui_status", "status.explorer").replace("airk_explorer_help_search", "help").replace("airk_explorer_help_get", "help").replace("airk_", "")


def documentation_only(value, tool, rules, path="", mode="tool"):
    if isinstance(value, str):
        rewritten = public_text(value)
        if rewritten != value:
            assert mode == "documentation", f"{tool}{path}: alias rewrite would change a semantic non-documentation string"
            rules.append({"tool": tool, "pointer": path, "rule": "owning_public_text_documentation_alias", "native": value, "public": rewritten})
        return rewritten
    if isinstance(value, dict):
        result = {}
        for key, item in value.items():
            child_mode = "opaque"
            if mode in ("tool", "schema") and key in ("description", "title") and isinstance(item, str):
                child_mode = "documentation"
            elif mode == "tool":
                if key in ("inputSchema", "outputSchema"):
                    child_mode = "schema"
                elif key == "annotations":
                    child_mode = "annotations"
            elif mode == "annotations" and key == "title" and isinstance(item, str):
                child_mode = "documentation"
            elif mode == "schema":
                if key in ("properties", "patternProperties", "$defs", "definitions", "dependentSchemas") and isinstance(item, dict):
                    child_mode = "schema_map"
                elif key in ("items", "additionalItems", "additionalProperties", "unevaluatedProperties", "unevaluatedItems", "propertyNames", "contains", "not", "if", "then", "else", "contentSchema") and isinstance(item, dict):
                    child_mode = "schema"
                elif key in ("allOf", "anyOf", "oneOf", "prefixItems", "items") and isinstance(item, list):
                    child_mode = "schema_array"
            elif mode == "schema_map" and isinstance(item, dict):
                child_mode = "schema"
            result[key] = documentation_only(item, tool, rules, pointer(path, key), child_mode)
        return result
    if isinstance(value, list):
        return [documentation_only(item, tool, rules, pointer(path, n), "schema" if mode == "schema_array" else "opaque") for n, item in enumerate(value)]
    return copy.deepcopy(value)


def native_names(tools):
    assert isinstance(tools, list) and len(tools) <= 128
    names = []
    for tool in tools:
        name = tool["name"]
        assert isinstance(name, str) and name.startswith("airk_") and not name[5:].startswith("airk_"), "Native names require exactly one compatibility prefix"
        names.append(name[5:])
    assert len(set(names)) == len(names), "Duplicate canonical native tool identity"
    return names


def owning_contract(repo):
    source_path = repo / "crates/airk-mcp/src/unified.rs"
    ui_path = repo / "crates/airk-mcp/src/unified_explorer.json"
    source = source_path.read_text(encoding="utf-8")
    section = re.search(r"const UI_NAMES:.*?= &\[(.*?)\];", source, re.S)
    assert section
    pairs = re.findall(r'\(\s*("[^"]+")\s*,\s*("[^"]+")\s*\)', section[1])
    assert [(json.loads(a), json.loads(b)) for a, b in pairs] == UI_NAMES, "Unreviewed UI alias set"
    project = re.search(r"fn project_schema\(\) -> Value\s*\{\s*json!\((\{.*?\})\)\s*\}", source, re.S)
    assert project and json.loads(project[1]) == PROJECT, "Unreviewed project routing schema"
    # Fail closed if the reviewed five additions or their schemas change.
    help_fields = re.search(r"for \(key, property\) in \[(.*?)\]\s*\{\s*schema\[\"properties\"\]\[key\] = property;", source, re.S)
    assert help_fields
    parsed = []
    for found in re.finditer(r'\(\s*("[^"]+")\s*,\s*json!\(', help_fields[1]):
        remaining = help_fields[1][found.end():].lstrip()
        value, end = json.JSONDecoder().raw_decode(remaining)
        assert re.match(r'\)\s*,?\s*\)', remaining[end:].lstrip()), "Unsupported owning help tuple syntax"
        parsed.append((json.loads(found[1]), value))
    assert dict(parsed) == HELP_UI and len(parsed) == 5, "Unreviewed UI-only help additions"
    descriptions = {}
    for name in ("status", "help"):
        found = re.findall(rf"\b{name}\.description\s*=\s*Some\(\s*({RUST_STRING})\s*\.into\(\)\s*\)", source)
        assert len(found) == 1, f"Ambiguous owning {name} documentation override"
        descriptions[name] = json.loads(found[0])
    for name in ("suggest_view", "projects"):
        found = re.findall(rf'definition\("{name}",\s*({RUST_STRING})\s*,', source)
        assert len(found) == 1, f"Ambiguous owning {name} definition"
        descriptions[name] = json.loads(found[0])
    legacy = json.loads(ui_path.read_text(encoding="utf-8"))
    legacy_names = [tool["name"] for tool in legacy]
    assert len(legacy) <= 128 and len(set(legacy_names)) == len(legacy_names)
    return {"source_hashes": {str(source_path.relative_to(repo)): sha(source_path), str(ui_path.relative_to(repo)): sha(ui_path)},
            "descriptions": descriptions, "legacy": dict(zip(legacy_names, legacy))}


def add_project(tool, name, rules):
    schema = tool["inputSchema"]
    assert isinstance(schema, dict) and isinstance(schema.get("properties"), dict)
    assert "project" not in schema["properties"] and "project" not in schema.get("required", []), "Native/UI field collides with transport project selector"
    schema["properties"]["project"] = copy.deepcopy(PROJECT)
    rules.append({"tool": name, "pointer": "/inputSchema/properties/project", "rule": "optional_connection_project_routing", "value": PROJECT})
    return tool


def definition(name, description, schema, read_only):
    return {"name": name, "description": description, "inputSchema": schema,
            "annotations": {"readOnlyHint": read_only, "destructiveHint": False, "idempotentHint": True, "openWorldHint": False}}


def expected_tools(native_tools, contract, rules):
    names = native_names(native_tools)
    assert not set(names) & ({public for public, _ in UI_NAMES} | {"suggest_view", "projects"}), "UI/native tool identity collision"
    result = []
    for original, name in zip(native_tools, names):
        expected = copy.deepcopy(original)
        expected["name"] = name
        rules.append({"tool": name, "pointer": "/name", "rule": "exact_one_airk_prefix", "native": original["name"], "public": name})
        expected = documentation_only(expected, name, rules)
        if name == "help":
            properties = expected["inputSchema"]["properties"]
            assert "project" not in properties and "project" not in expected["inputSchema"].get("required", [])
            for key, value in HELP_UI.items():
                assert key not in properties and key not in expected["inputSchema"].get("required", []), "UI help addition collides with native input"
                properties[key] = copy.deepcopy(value)
                rules.append({"tool": name, "pointer": pointer("/inputSchema/properties", key), "rule": "ui_only_help_selector_not_native_read_equivalence", "value": value})
        else:
            add_project(expected, name, rules)
        if name in ("status", "help"):
            expected["description"] = contract["descriptions"][name]
            rules.append({"tool": name, "pointer": "/description", "rule": "exact_owning_adapter_description_override", "value": expected["description"]})
        result.append(expected)
    assert "help" in names and "status" in names, "Required native metadata tool missing"
    for public, old in UI_NAMES:
        original = copy.deepcopy(contract["legacy"][old])
        original["name"] = public
        expected = documentation_only(original, public, rules)
        if public not in ("list_templates", "get_template"):
            add_project(expected, public, rules)
        rules.append({"tool": public, "pointer": "/tools/" + public, "rule": "exact_bundled_ui_only_definition", "source_tool": old})
        result.append(expected)
    variants = []
    for kind in ("navigation", "review", "management"):
        schema = copy.deepcopy(contract["legacy"]["airk_explorer_suggest_" + kind]["inputSchema"])
        schema["title"] = "Draft for kind=" + kind
        rules.append({"tool": "suggest_view", "pointer": "/inputSchema/properties/draft/anyOf/" + str(len(variants)) + "/title", "rule": "exact_ui_variant_title", "value": schema["title"]})
        variants.append(documentation_only(schema, "suggest_view", rules, "/inputSchema/properties/draft/anyOf/" + str(len(variants)), "schema"))
    schema = {"type": "object", "additionalProperties": False, "required": ["kind", "draft"],
              "properties": {"kind": {"type": "string", "enum": ["navigation", "review", "management"]},
                             "draft": {"type": "object", "description": "Use the draft schema matching kind. The editor validates that exact kind before admitting a suggestion.", "anyOf": variants}}}
    result.append(add_project(definition("suggest_view", contract["descriptions"]["suggest_view"], schema, False), "suggest_view", rules))
    result.append(definition("projects", contract["descriptions"]["projects"], {"type": "object", "additionalProperties": False, "properties": {}}, True))
    for name in ("suggest_view", "projects"):
        rules.append({"tool": name, "pointer": "/tools/" + name, "rule": "exact_owning_ui_only_definition"})
    return result


def compare(native_result, mcp_result, repo=REPO):
    rules, failures = [], []
    report = {"schema": 1, "passed": False, "rules": rules, "differences": failures,
              "scope": "declared_native_input_and_full_tool_metadata_projection", "native_results_normalized": False,
              "ui_only_help_qualified_as_native_read": False, "ui_only_tools": [name for name, _ in UI_NAMES] + ["suggest_view", "projects"]}
    try:
        contract = owning_contract(repo)
        report["source_hashes"] = contract["source_hashes"]
        expected = expected_tools(native_result["tools"], contract, rules)
        actual = mcp_result["tools"]
        assert isinstance(actual, list) and len(actual) <= 128
        actual_names = [tool["name"] for tool in actual]
        assert len(set(actual_names)) == len(actual_names), "Duplicate public tool identity"
        expected_names = [tool["name"] for tool in expected]
        if actual_names != expected_names:
            failures.append({"pointer": "/tools", "kind": "exact_declared_tool_order_and_set", "expected": expected_names, "actual": actual_names})
        actual_map = dict(zip(actual_names, actual))
        for tool in expected:
            if tool["name"] not in actual_map:
                continue
            found = difference(tool, actual_map[tool["name"]])
            if found:
                failures.append({"tool": tool["name"], **found})
        report.update(passed=not failures, native_tool_count=len(native_result["tools"]), public_tool_count=len(actual))
    except (AssertionError, KeyError, TypeError, ValueError) as error:
        failures.append({"kind": "projection_contract_refused", "message": str(error)})
    return report


def get_path(value, path):
    for part in path:
        value = value[part]
    return value


def find_path(value, key, predicate, path=()):
    if isinstance(value, dict):
        for name, item in value.items():
            if name == key and predicate(item):
                return (*path, name)
            found = find_path(item, key, predicate, (*path, name))
            if found:
                return found
    elif isinstance(value, list):
        for n, item in enumerate(value):
            found = find_path(item, key, predicate, (*path, n))
            if found:
                return found
    return None


def tamper_tests(native, public):
    baseline = compare(native, public)
    assert baseline["passed"], "Actual captured baseline does not satisfy reviewed projection"
    names = native_names(native["tools"])
    by_name = {tool["name"]: n for n, tool in enumerate(public["tools"])}
    outcomes = []
    def reject(name, changed_native, changed_public, contract_refusal=False):
        observed = compare(changed_native, changed_public)
        assert observed["source_hashes"] == baseline["source_hashes"], "Owning projection source changed during tamper tests"
        assert not observed["passed"], f"Tampered projection accepted: {name}"
        assert contract_refusal or any(item["kind"] != "projection_contract_refused" for item in observed["differences"]), "Tamper test failed before checking its changed public value"
        outcomes.append({"id": name, "rejected": True, "differences": observed["differences"]})
    for field, predicate in (("maximum", lambda value: type(value) is int), ("enum", lambda value: isinstance(value, list) and bool(value)), ("required", lambda value: isinstance(value, list) and bool(value))):
        for original, name in zip(native["tools"], names):
            path = find_path(original["inputSchema"], field, predicate)
            if path:
                changed = copy.deepcopy(public)
                parent = get_path(changed["tools"][by_name[name]]["inputSchema"], path[:-1])
                prior = parent[path[-1]]
                parent[path[-1]] = prior + 1 if type(prior) is int else [*prior, "unexpected_projection_value"]
                reject("changed_native_" + field, native, changed)
                break
        else:
            raise AssertionError("Actual native catalog lacks tamper target: " + field)
    base = next(name for name in names if name != "help")
    for label, modify in [
        ("altered_project_schema", lambda tool: tool["inputSchema"]["properties"]["project"].update(pattern=".*")),
        ("required_project_selector", lambda tool: tool["inputSchema"].setdefault("required", []).append("project")),
        ("altered_tool_annotations", lambda tool: tool.setdefault("annotations", {}).update(readOnlyHint=False)),
    ]:
        changed = copy.deepcopy(public)
        modify(changed["tools"][by_name[base]])
        reject(label, native, changed)
    changed = copy.deepcopy(public)
    changed["tools"][by_name["help"]]["inputSchema"]["properties"]["limit"]["maximum"] = 21
    reject("altered_ui_help_extra", native, changed)
    changed = copy.deepcopy(public)
    changed["tools"].append({"name": "unexpected_tool", "inputSchema": {}})
    reject("unexpected_public_tool", native, changed)
    changed_native, changed_public = copy.deepcopy(native), copy.deepcopy(public)
    changed_native["tools"][names.index(base)]["inputSchema"]["future_schema_field"] = {"opaque": [True, 7, {"value": "retained"}]}
    changed_public["tools"][by_name[base]]["inputSchema"]["future_schema_field"] = {"opaque": [True, 7, {"value": "retained"}]}
    assert compare(changed_native, changed_public)["passed"], "Complete matching unknown native fields were dropped"
    changed_public["tools"][by_name[base]]["inputSchema"]["future_schema_field"]["opaque"][1] = 8
    reject("changed_unknown_native_field", changed_native, changed_public)
    changed_native = copy.deepcopy(native)
    changed_native["tools"][0]["inputSchema"]["semantic_default"] = "airk_search"
    reject("semantic_non_documentation_alias_rewrite", changed_native, public, contract_refusal=True)
    for field in ("default", "unknown_semantic_field"):
        changed_native = copy.deepcopy(native)
        changed_native["tools"][0]["inputSchema"][field] = {"title": "airk_search", "description": "airk_read_file"}
        reject("semantic_" + field + "_documentation_keys", changed_native, public, contract_refusal=True)
    return outcomes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--captured-report", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    captured = json.loads(args.captured_report.read_text(encoding="utf-8"))
    native, public = captured["cli_catalog"], captured["mcp_catalog"]
    result = compare(native, public)
    result.update(captured_report_sha256=sha(args.captured_report), comparator_sha256=sha(Path(__file__)),
                  qualification="offline_actual_captured_catalog_projection_only", cli_catalog=native, mcp_catalog=public)
    if args.self_test and result["passed"]:
        result["tamper_tests"] = tamper_tests(native, public)
    args.report.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": result["passed"], "differences": result["differences"], "tamper_tests": len(result.get("tamper_tests", []))}))
    if not result["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
