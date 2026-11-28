# SPDX-License-Identifier: Apache-2.0
"""Read-only patch and AST evidence for the checked Munarium repair experiment."""
import argparse
import ast
from collections import Counter
import difflib
import hashlib
import json
from pathlib import Path


def inventory(text):
    tree = ast.parse(text)
    tests = {}
    controls = Counter()
    transports = Counter()
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name.startswith("test_"):
            # Preserve expressions for the reviewer. Renaming is reported as removal plus addition.
            tests.setdefault(node.name, []).append({
                "assertions": [ast.unparse(n.test) for n in ast.walk(node) if isinstance(n, ast.Assert)],
                "raises": [ast.unparse(n) for n in ast.walk(node) if isinstance(n, ast.Call)
                           and ast.unparse(n.func).endswith(".raises")],
                "decorators": [ast.unparse(n) for n in node.decorator_list],
            })
        if isinstance(node, ast.Attribute) and node.attr in {"skip", "skipif", "xfail", "importorskip"}:
            controls[ast.unparse(node)] += 1
        if isinstance(node, ast.Call):
            func = ast.unparse(node.func).rsplit(".", 1)[-1]
            if func in {"SyncRestTransport", "AsyncRestTransport", "MockTransport"}:
                transports[func] += 1
    return {"tests": tests, "skip_controls": dict(controls), "transport_calls": dict(transports)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", required=True, type=Path)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--paths", nargs="+", required=True)
    parser.add_argument("--before", type=Path)
    args = parser.parse_args()
    inventory_by_path = {}
    file_digests = {}
    patch = []
    for path in sorted(set(args.paths)):
        before = args.snapshot / path
        after = args.work / path
        left = before.read_bytes() if before.is_file() else b""
        right = after.read_bytes() if after.is_file() else (b"" if args.before else left)
        file_digests[path] = hashlib.sha256(right).hexdigest() if after.is_file() or (not args.before and before.is_file()) else None
        try:
            inventory_by_path[path] = inventory(right.decode("utf-8-sig"))
        except (UnicodeError, SyntaxError) as exc:
            inventory_by_path[path] = {"parse_error": str(exc)}
        if args.before and left != right:
            diff = difflib.unified_diff(left.decode("utf-8").splitlines(keepends=True),
                                        right.decode("utf-8").splitlines(keepends=True),
                                        "a/" + path, "b/" + path)
            patch.extend(line if line.endswith("\n") else line + "\n" for line in diff)
    out = {"files": inventory_by_path, "file_sha256": file_digests}
    if args.before:
        initial = json.loads(args.before.read_text(encoding="utf-8"))
        flags = {}
        for path, final in inventory_by_path.items():
            original = initial["files"][path]
            early, late = original.get("tests", {}), final.get("tests", {})
            flags[path] = {"removed_tests": sorted(early.keys() - late.keys()),
                           "added_tests": sorted(late.keys() - early.keys()),
                           "changed_assertions_or_decorators": sorted(k for k in early.keys() & late.keys()
                                                                       if early[k] != late[k]),
                           "changed_skip_controls": original.get("skip_controls") != final.get("skip_controls"),
                           "changed_transport_calls": original.get("transport_calls") != final.get("transport_calls")}
        out.update(flags=flags, patch="".join(patch))
    print(json.dumps(out, ensure_ascii=True))


if __name__ == "__main__":
    main()