# SPDX-License-Identifier: Apache-2.0
"""Derive the terminal service-registry schema fixtures of change-log entry 68
part 3 from the retained airk-state/1 captures.

The terminal catalog layout is every retained predecessor layout without the
tables reviewed-grant removal drops, together with the indexes, triggers and
automatic indexes SQLite drops with them. This derivation reads only the
retained 1.8.16 captures in fixtures/schema/airk-state-1 and removes those
objects by their table name; it shares no code with the Rust upgrade step it
checks.

    python scripts/dev/derive_terminal_registry_schema.py          # write
    python scripts/dev/derive_terminal_registry_schema.py --check  # compare
"""
import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures"
RETAINED = FIXTURES / "schema" / "airk-state-1"
TERMINAL = ROOT / "fixtures" / "schema" / "airk-state-1-terminal"
# Reviewed by hand against the local trust plan, decision 3, and the user's
# 23 November 2026 record-cleanup decisions.
REMOVED_TABLES = {
    "index_access_grants",
    "index_access_memberships",
    "index_access_scopes",
    "index_adopted_authorities",
    "index_adopted_configuration_claims",
    "index_context_imports",
    "legacy_index_projects",
}
# terminal name -> retained predecessor captures that must all reduce to it.
LAYOUTS = {
    "service-1.4.terminal-authority0": ["service-1.4.authority0"],
    "service-1.4.terminal-authority1": ["service-1.4.authority1"],
    "service-1.4.terminal-policy1": ["service-1.4.policy1"],
    "service-1.4.terminal-policy2": ["service-1.4.policy2"],
    "service-1.4.terminal-policy3": ["service-1.4.policy3"],
    "service-1.4.terminal-authority1-schema13": [
        "service-1.4.authority1-armed",
        "service-1.4.adopted-policy0",
    ],
    "service-1.4.terminal-policy1-schema13": [
        "service-1.4.policy1-armed",
        "service-1.4.adopted-policy1",
    ],
    "service-1.4.terminal-policy2-schema13": [
        "service-1.4.policy2-armed",
        "service-1.4.adopted-policy2",
    ],
    "service-1.4.terminal-policy3-schema13": [
        "service-1.4.policy3-armed",
        "service-1.4.adopted-policy3",
    ],
    # The U105 description layer over policy 3 (fixtures/split/agents).
    "service-1.4.terminal-policy3-descriptions": [
        "../../split/agents/registry-policy3-descriptions",
    ],
}


def load(name, kind):
    return json.loads((RETAINED / f"{name}.{kind}.json").read_text(encoding="utf-8"))


def reduce(objects):
    return [item for item in objects if item["tbl_name"] not in REMOVED_TABLES]


def render(value):
    return json.dumps(value, indent=2, ensure_ascii=True) + "\n"


def derived():
    files = {}
    for terminal, sources in LAYOUTS.items():
        objects = [reduce(load(source, "objects")) for source in sources]
        metas = [load(source, "meta") for source in sources]
        for other, meta in zip(objects[1:], metas[1:]):
            if other != objects[0] or meta != metas[0]:
                sys.exit(f"{terminal}: {sources} do not reduce to one layout")
        files[f"{terminal}.objects.json"] = render(objects[0])
        files[f"{terminal}.meta.json"] = render(metas[0])
    files["manifest.json"] = render({
        "schema": "airk.terminal-registry-schema/1",
        "state_contract": "airk-state/1",
        "derived_by": "python scripts/dev/derive_terminal_registry_schema.py",
        "removed_tables": sorted(REMOVED_TABLES),
        "layouts": {name: sorted(sources) for name, sources in LAYOUTS.items()},
    })
    return files


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    files = derived()
    if args.check:
        stale = [name for name, text in files.items()
                 if not (TERMINAL / name).is_file()
                 or (TERMINAL / name).read_text(encoding="utf-8") != text]
        if stale:
            sys.exit("terminal registry fixtures differ: " + ", ".join(stale))
        print(f"{len(files)} terminal registry fixtures current")
        return
    TERMINAL.mkdir(exist_ok=True)
    for name, text in files.items():
        (TERMINAL / name).write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {len(files)} files to {TERMINAL.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
