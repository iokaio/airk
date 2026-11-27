# SPDX-License-Identifier: Apache-2.0
"""The harness's one mapping between AIRK tool spellings.

Three spellings name one operation: native (``airk_search``), operation
(``search``, also the pre-1.9 public MCP name) and public (``search_passages``
from 1.9, completion plan decision 12, 26 November 2026). The table is not kept
here: it is read from ``fixtures/split/catalog/public_tool_names.json``, which
the Rust owner ``airk-core::tool_names`` emits and checks.

Native names work against every fixture image, old or new, so the harness calls
tools by native name. ``public(..., names="legacy")`` gives the pre-1.9 public
names, for a script that must address an older image's MCP catalog by its own
names; ``names="current"`` (the default) gives the 1.9 names.
"""
from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TABLE = ROOT / "fixtures" / "split" / "catalog" / "public_tool_names.json"
NAMES = ("current", "legacy")


@lru_cache(maxsize=1)
def renamed() -> tuple[tuple[str, str, str], ...]:
    """``(native, operation, public)`` for every renamed public tool."""
    table = json.loads(TABLE.read_text(encoding="utf-8"))
    if table.get("schema") != "airk.public-tool-names/1":
        raise ValueError(f"unexpected public tool name table schema in {TABLE}")
    return tuple((row["native"], row["operation"], row["public"]) for row in table["renamed"])


def native(name: str) -> str:
    """The native name of any spelling: native, operation, legacy public or 1.9 public."""
    if name.startswith("airk_"):
        return name
    for native_name, _operation, public_name in renamed():
        if name == public_name:
            return native_name
    return "airk_" + name


def operation(name: str) -> str:
    """The operation name (``search``) of any spelling."""
    return native(name)[len("airk_"):]


def public(name: str, names: str = "current") -> str:
    """The public MCP name of any spelling, as a 1.9 (``current``) or pre-1.9 (``legacy``) image lists it."""
    if names not in NAMES:
        raise ValueError(f"names must be one of {NAMES}, not {names!r}")
    op = operation(name)
    if names == "current":
        for _native, old, public_name in renamed():
            if op == old:
                return public_name
    return op
