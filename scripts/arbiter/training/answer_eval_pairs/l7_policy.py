# SPDX-License-Identifier: Apache-2.0
"""Path-only admission for L7 development training inputs."""

from __future__ import annotations

import ast
import hashlib
import json
from pathlib import Path
import re
import unicodedata

PROTECTED_SCHEMA = "airk.private.answer-eval.protected-paths/1"
REPOS = frozenset(("bookbuilder", "munarium"))
MAX_INVENTORY_BYTES = 4 * 1024 * 1024
FIXTURE = Path(__file__).resolve().parents[3] / "arbiter-answer-eval" / "fixture.py"


def normalized_path(path: str) -> str:
    """The same repository-relative file across case or Unicode spellings."""
    return unicodedata.normalize("NFC", path).casefold()


def safe_relative_path(path: str) -> bool:
    return (isinstance(path, str) and bool(path)
            and not any(c in '\\:<>"|?*' or ord(c) < 32 or ord(c) == 127 for c in path)
            and all(part not in ("", ".", "..") and not part.endswith((" ", "."))
                    for part in path.split("/")))


def unique_object(items: list) -> dict:
    result = dict(items)
    if len(result) != len(items):
        raise ValueError("protected path inventory has a duplicate key")
    return result


def load_protected_paths(path: Path, expected_sha256: str) -> dict[str, frozenset[str]]:
    if not expected_sha256 or not re.fullmatch(r"[0-9a-f]{64}", expected_sha256):
        raise ValueError("protected path inventory needs an exact lowercase SHA-256")
    with Path(path).open("rb") as source:
        raw = source.read(MAX_INVENTORY_BYTES + 1)
    if len(raw) > MAX_INVENTORY_BYTES or hashlib.sha256(raw).hexdigest() != expected_sha256:
        raise ValueError("protected path inventory size or SHA-256 differs")
    data = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object)
    if (not isinstance(data, dict) or set(data) != {"schema", "paths"}
            or data["schema"] != PROTECTED_SCHEMA
            or not isinstance(data["paths"], dict) or set(data["paths"]) != REPOS):
        raise ValueError("protected path inventory must cover exactly Bookbuilder and Munarium")
    protected = {}
    for repo, paths in data["paths"].items():
        if not isinstance(paths, list) or not paths or not all(
            safe_relative_path(path) and path == unicodedata.normalize("NFC", path) for path in paths
        ):
            raise ValueError("protected path inventory needs normalized nonempty repository-relative file lists")
        keys = frozenset(normalized_path(path) for path in paths)
        if len(keys) != len(paths):
            raise ValueError("protected path inventory contains duplicate or aliased entries")
        protected[repo] = keys
    return protected


def bookbuilder_denies() -> tuple[tuple[str, str], ...]:
    """Read the maintained fixture's literal Bookbuilder-v2 policy without running it."""
    tree = ast.parse(FIXTURE.read_bytes())
    declarations, aliases = [], []
    alias = ast.parse('EXCLUSIONS["bookbuilder-v2"] = list(EXCLUSIONS["bookbuilder"])').body[0]
    for statement in tree.body:
        if not isinstance(statement, ast.Assign) or len(statement.targets) != 1:
            continue
        target = statement.targets[0]
        if isinstance(target, ast.Name) and target.id == "EXCLUSIONS":
            declarations.append(statement.value)
        if (isinstance(target, ast.Subscript) and isinstance(target.value, ast.Name)
                and target.value.id == "EXCLUSIONS" and isinstance(target.slice, ast.Constant)
                and target.slice.value == "bookbuilder-v2"):
            aliases.append(statement)
    if len(declarations) != 1 or [ast.dump(item) for item in aliases] != [ast.dump(alias)]:
        raise ValueError("Bookbuilder-v2 deny policy moved; review its owning fixture")
    declared = ast.literal_eval(declarations[0])
    policy = declared.get("bookbuilder") if isinstance(declared, dict) else None
    if not isinstance(policy, list) or not policy or not all(
        isinstance(row, tuple) and len(row) == 2 and row[0] in ("exact", "subtree")
        and safe_relative_path(row[1]) for row in policy
    ):
        raise ValueError("unsupported Bookbuilder-v2 deny policy")
    return tuple((kind, normalized_path(path)) for kind, path in policy)


def denied_by_fixture(path: str, rules: tuple[tuple[str, str], ...]) -> bool:
    key = normalized_path(path)
    return any(key == denied or (kind == "subtree" and key.startswith(denied + "/"))
               for kind, denied in rules)


def deny_policy_sha256(rules: tuple[tuple[str, str], ...]) -> str:
    return hashlib.sha256(json.dumps(rules, ensure_ascii=False, separators=(",", ":")).encode("utf-8")).hexdigest()
