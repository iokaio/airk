# SPDX-License-Identifier: Apache-2.0
"""Boundary gates for docs/simplify-plan.md principle 8 and Milestone S5.

Gated (exit 1 on a violation, with one precise message per finding):

- module honesty: every non-platform `#[path]` mount, every `include!` of
  `.rs` source and every orphan `.rs` file must have a reviewed entry in
  boundary-exceptions.json. Identity is line-independent: kind, declaring
  file and mounted value;
- crate edges: each workspace crate's normal (non-dev, non-build) `airk-*`
  dependencies must appear in the reviewed allowed-edge list;
- module cycles (ratchet): each top-level module cycle of a library crate
  must be a subset of exactly one recorded baseline cycle. A module joining
  a cycle, a new cycle or two baseline cycles merging fails; leaving a cycle
  passes.

Not applicable yet, reported as such: registration completeness (no
registry exists) and declared state ownership (no ownership declarations
exist). Sizes, visibility and vocabulary stay report-only in
scripts/dev/simplify_metrics.py.

The exceptions file is a reviewed expectation. It is never derived from the
tree at check time; `--write-baseline` regenerates it from the current tree
for a reviewed update, keeping existing reasons.

    python scripts/dev/boundary_check.py
    python scripts/dev/boundary_check.py --json
    python scripts/dev/boundary_check.py --write-baseline
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_EXCEPTIONS = Path(__file__).resolve().parent / "boundary-exceptions.json"
SCHEMA = "airk-boundary-exceptions/1"
REPORT_SCHEMA = "airk-boundary-check/1"

_SPEC = importlib.util.spec_from_file_location(
    "simplify_metrics", Path(__file__).resolve().parent / "simplify_metrics.py"
)
METRICS = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(METRICS)

AUTO_TARGET_PREFIXES = ("tests:", "benches:", "examples:")
EMBED_POOL_NAME = re.compile(r"(?:^|/)(?:embed_pool\w*|inference_owner\w*)\.rs$")
FEATURE_CFG = re.compile(r"\bfeature\s*=")

REASONS = {
    "integration-test-root-child": "Declared by an integration-test, bench or example root; at its canonical "
    "location beside the root it would become another Cargo target.",
    "platform-switch-child": "Declared by a file loaded through a kept platform-switch mount; its canonical "
    "location follows the switch, so it moves only with that switch.",
    "kept-mount-child": "Declared by a file that is itself loaded through a kept #[path] mount.",
    "multi-declaration": "The file is reached from more than one module declaration, so it has no single "
    "canonical location.",
    "parent-dir-mount": "A ../ mount that leaves its directory or Cargo target (shared test protocol or source).",
    "feature-switched-alternative": "A feature-switched alternative module; the canonical location is occupied "
    "by the other alternative.",
    "embed-pool-include": "Part of the embed-pool include! scheme; replacing it needs feature-build checks "
    "(docs/simplify-plan.md S4).",
}
UNCLASSIFIED = "unclassified"


class TrackingCrate(METRICS.Crate):
    """The metrics module walk, also recording which file declares each reached file."""

    def __init__(self, directory, root):
        self._stack = []
        self._mounts_seen = 0
        self._includes_seen = 0
        self.declarations = {}  # child path -> set of (declaring file, kind, line, value)
        self.mount_children = {}  # (file, line, value) -> resolved child path
        super().__init__(directory, root)

    def _visit(self, path, target, top, mod_rs_like, test_only):
        if self._stack:
            parent = METRICS.rel(self._stack[-1], self.root)
            # The walk appends a mount or include immediately before visiting its child.
            if len(self.path_mounts) > self._mounts_seen:
                self._mounts_seen = len(self.path_mounts)
                file, line, value, _ = self.path_mounts[-1]
                self.mount_children[(file, line, value)] = path
                declaration = (parent, "path", line, value)
            elif len(self.includes) > self._includes_seen:
                self._includes_seen = len(self.includes)
                file, line, value = self.includes[-1]
                declaration = (parent, "include", line, value)
            else:
                declaration = (parent, "mod", None, None)
            self.declarations.setdefault(path, set()).add(declaration)
        self._stack.append(path)
        try:
            super()._visit(path, target, top, mod_rs_like, test_only)
        finally:
            self._stack.pop()


def package_name(directory):
    try:
        data = tomllib.loads(METRICS.read_text(directory / "Cargo.toml"))
    except tomllib.TOMLDecodeError:
        return directory.name
    return data.get("package", {}).get("name", directory.name)


def discover(root):
    crates = []
    base = root / "crates"
    if base.is_dir():
        for directory in sorted(base.iterdir()):
            if (directory / "Cargo.toml").is_file():
                crate = TrackingCrate(directory, root)
                crate.walk()
                crate.package = package_name(directory)
                crates.append(crate)
    return crates


def attribute_cfgs(lines, mod_line):
    """`#[cfg(...)]` expressions in the attribute block above a 1-based `mod` line."""
    cfgs, index = [], mod_line - 2
    while index >= 0:
        stripped = lines[index].strip()
        if stripped.startswith("#"):
            match = METRICS.CFG_ATTR.search(stripped)
            if match:
                cfgs.append(match.group(1))
        elif not stripped.startswith("//"):
            break
        index -= 1
    return cfgs


def declared_twice(lines, mod_line):
    """True when the module declared on `mod_line` has another declaration in the same file."""
    match = METRICS.MOD_DECL.match(lines[mod_line - 1]) if 0 < mod_line <= len(lines) else None
    if not match:
        return False
    return sum(1 for text in lines if (other := METRICS.MOD_DECL.match(text)) and other.group(1) == match.group(1)) > 1


def honesty_items(root, crates):
    """Non-platform mounts, `.rs` includes and orphans, each with a computed reason class."""
    mounts = {}
    platform_children, mount_children = set(), {}
    root_files, declarations = set(), {}
    for crate in crates:
        for label, path in crate.targets:
            if label.startswith(AUTO_TARGET_PREFIXES):
                root_files.add(METRICS.rel(path, root))
        for child, found in crate.declarations.items():
            declarations.setdefault(METRICS.rel(child, root), set()).update(found)
        for file, line, value, platform in crate.path_mounts:
            child = crate.mount_children.get((file, line, value))
            child_rel = METRICS.rel(child, root) if child is not None and child.exists() else None
            if platform:
                if child_rel:
                    platform_children.add(child_rel)
                continue
            mounts.setdefault((file, value), (line, child_rel))
            if child_rel:
                mount_children[child_rel] = file
    # Files loaded through a platform mount, followed through further kept mounts.
    lineage, changed = set(platform_children), True
    while changed:
        changed = False
        for child, parent in mount_children.items():
            if parent in lineage and child not in lineage:
                lineage.add(child)
                changed = True

    items = []
    for (file, value), (line, child) in sorted(mounts.items()):
        lines = METRICS.read_text(root / file).splitlines()
        cfgs = attribute_cfgs(lines, line)
        if "../" in value.replace("\\", "/"):
            reason_class = "parent-dir-mount"
        elif any(FEATURE_CFG.search(cfg) for cfg in cfgs) and declared_twice(lines, line):
            reason_class = "feature-switched-alternative"
        elif file in lineage:
            reason_class = "platform-switch-child"
        elif child and len(declarations.get(child, ())) > 1:
            reason_class = "multi-declaration"
        elif file in root_files:
            reason_class = "integration-test-root-child"
        elif file in mount_children:
            reason_class = "kept-mount-child"
        else:
            reason_class = UNCLASSIFIED
        items.append({"kind": "path", "file": file, "value": value, "line": line, "reason_class": reason_class})

    includes = {}
    for crate in crates:
        for file, line, value in crate.includes:
            includes.setdefault((file, value), line)
    for (file, value), line in sorted(includes.items()):
        involved = EMBED_POOL_NAME.search(file) or EMBED_POOL_NAME.search("/" + value.replace("\\", "/"))
        reason_class = "embed-pool-include" if involved else UNCLASSIFIED
        items.append({"kind": "include", "file": file, "value": value, "line": line, "reason_class": reason_class})

    reached = {path for crate in crates for path in crate.reached}
    for path in METRICS.rust_files(root):
        relative = METRICS.rel(path, root)
        if "/fixtures/" in relative or path.resolve() in reached:
            continue
        items.append({"kind": "orphan", "file": relative, "value": None, "line": None, "reason_class": UNCLASSIFIED})
    return items


def crate_edges(crates):
    names = {crate.package for crate in crates}
    edges = {}
    for crate in crates:
        try:
            data = tomllib.loads(METRICS.read_text(crate.directory / "Cargo.toml"))
        except tomllib.TOMLDecodeError as error:
            raise SystemExit(f"cannot parse {crate.directory / 'Cargo.toml'}: {error}")
        tables = [data.get("dependencies", {})]
        tables += [spec.get("dependencies", {}) for spec in data.get("target", {}).values()]
        found = set()
        for table in tables:
            for key, spec in table.items():
                name = spec.get("package", key) if isinstance(spec, dict) else key
                if name != crate.package and (name in names or name.startswith("airk-")):
                    found.add(name)
        edges[crate.package] = sorted(found)
    return edges


def module_cycles(crates):
    cycles = {}
    for crate in crates:
        if not any(label == "lib" for label, _ in crate.targets):
            continue
        graph, _ = METRICS.module_graph(crate)
        cycles[crate.package] = sorted(
            component for component in METRICS.strongly_connected(graph) if len(component) > 1
        )
    return cycles


def analyze(root):
    crates = discover(root)
    return {
        "honesty": honesty_items(root, crates),
        "edges": crate_edges(crates),
        "cycles": module_cycles(crates),
    }


def identity(item):
    return (item["kind"], item["file"], item.get("value"))


def describe(item):
    if item["kind"] == "path":
        return f'{item["file"]}: #[path = "{item["value"]}"] (line {item["line"]})'
    if item["kind"] == "include":
        return f'{item["file"]}: include!("{item["value"]}") (line {item["line"]})'
    return f'{item["file"]}: orphan .rs file (reached from no Cargo target)'


def check_honesty(items, exceptions):
    failures, warnings = [], []
    entries = {}
    for entry in exceptions.get("module_honesty", []):
        key = identity(entry)
        if key in entries:
            failures.append(f"duplicate exception {key}")
        entries[key] = entry
        if entry.get("kind") not in {"path", "include", "orphan"}:
            failures.append(f"exception {key}: kind must be path, include or orphan")
        if not str(entry.get("reason", "")).strip() or entry.get("reason_class") in (None, "", UNCLASSIFIED):
            failures.append(f"exception {key}: needs a reviewed reason_class and reason")
    present = set()
    for item in items:
        key = identity(item)
        present.add(key)
        if key not in entries:
            failures.append(
                f"{describe(item)} has no reviewed exception (suggested reason_class: {item['reason_class']})"
            )
    for key in sorted(set(entries) - present, key=str):
        warnings.append(f"stale exception {key} no longer matches the tree; remove it in a reviewed update")
    return failures, warnings


def check_edges(edges, exceptions):
    failures, warnings = [], []
    allowed = exceptions.get("crate_edges", {}).get("allowed", {})
    for crate, targets in sorted(edges.items()):
        if crate not in allowed:
            failures.append(f"crate {crate} has no reviewed allowed-edge entry (depends on {targets})")
            continue
        for target in targets:
            if target not in allowed[crate]:
                failures.append(f"forbidden crate edge {crate} -> {target} (not in crate_edges.allowed.{crate})")
        for target in sorted(set(allowed[crate]) - set(targets)):
            warnings.append(f"allowed edge {crate} -> {target} is unused")
    return failures, warnings


def check_cycles(cycles, exceptions):
    failures, warnings = [], []
    baseline = exceptions.get("module_cycles", {})
    for crate, components in sorted(cycles.items()):
        recorded = [set(component) for component in baseline.get(crate, [])]
        current_members = set()
        for component in components:
            members = set(component)
            current_members |= members
            known = set().union(*recorded) if recorded else set()
            joined = sorted(members - known)
            if joined:
                failures.append(
                    f"{crate}: module(s) {joined} joined a top-level module cycle {sorted(members)} "
                    "that the reviewed baseline does not record"
                )
                continue
            holders = [index for index, cycle in enumerate(recorded) if members & cycle]
            if len(holders) > 1:
                merged = [sorted(recorded[index]) for index in holders]
                failures.append(f"{crate}: baseline cycles {merged} merged into one cycle {sorted(members)}")
        for cycle in recorded:
            left = sorted(cycle - current_members)
            if left:
                warnings.append(f"{crate}: {left} left their baseline cycle; --write-baseline can shrink it")
    for crate in sorted(set(baseline) - set(cycles)):
        warnings.append(f"module_cycles has an entry for {crate}, which has no library target")
    return failures, warnings


def evaluate(facts, exceptions):
    checks = {}
    for name, (failures, warnings) in (
        ("module_honesty", check_honesty(facts["honesty"], exceptions)),
        ("crate_edges", check_edges(facts["edges"], exceptions)),
        ("module_cycles", check_cycles(facts["cycles"], exceptions)),
    ):
        checks[name] = {"status": "fail" if failures else "pass", "failures": failures, "warnings": warnings}
    checks["registration_completeness"] = {
        "status": "not_applicable", "failures": [], "warnings": [],
        "detail": "no operation registry exists yet",
    }
    checks["declared_state_ownership"] = {
        "status": "not_applicable", "failures": [], "warnings": [],
        "detail": "no component declares owned state in a machine-readable form yet",
    }
    ok = all(check["status"] != "fail" for check in checks.values())
    return {"schema": REPORT_SCHEMA, "ok": ok, "checks": checks}


def build_baseline(facts, previous):
    kept = {identity(entry): entry for entry in previous.get("module_honesty", [])}
    honesty = []
    for item in facts["honesty"]:
        old = kept.get(identity(item), {})
        reason_class = old.get("reason_class") or item["reason_class"]
        if reason_class == UNCLASSIFIED and item["reason_class"] != UNCLASSIFIED:
            reason_class = item["reason_class"]
        honesty.append({
            "kind": item["kind"],
            "file": item["file"],
            "value": item["value"],
            "reason_class": reason_class,
            "reason": old.get("reason") or REASONS.get(reason_class, ""),
        })
    return {
        "schema": SCHEMA,
        "note": "Reviewed expectations for scripts/dev/boundary_check.py. Changing any entry is a reviewed "
        "diff; never edit it to make a check pass without review. Identities are line-independent.",
        "crate_edges": {
            "note": "Allowed normal (non-dev, non-build) dependencies between workspace crates, seeded from "
            "the edges present when the gate was introduced. Adding an edge is a reviewed diff against "
            "docs/architecture.md and docs/simplify-plan.md section 4.1.",
            "allowed": facts["edges"],
        },
        "module_cycles": {
            "note": "Ratchet baseline of top-level module cycles per library crate. A module may leave a "
            "cycle without review; joining a cycle, a new cycle or a merge of cycles fails.",
            **facts["cycles"],
        },
        "module_honesty": honesty,
        "reason_classes": REASONS,
    }


def normalize(exceptions):
    """Drop `note` keys from the cycle map so it holds only crate entries."""
    cycles = dict(exceptions.get("module_cycles", {}))
    cycles.pop("note", None)
    return {**exceptions, "module_cycles": cycles}


def render(report):
    out = []
    for name, check in report["checks"].items():
        status = check["status"].upper().replace("_", " ")
        detail = f" ({check['detail']})" if check.get("detail") else ""
        out.append(f"{name}: {status}{detail}")
        out += [f"  FAIL {message}" for message in check["failures"]]
        out += [f"  warn {message}" for message in check["warnings"]]
    out.append("boundary check: " + ("passed" if report["ok"] else "FAILED"))
    return "\n".join(out) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--exceptions", type=Path, default=DEFAULT_EXCEPTIONS)
    parser.add_argument("--json", action="store_true", help="print the JSON report")
    parser.add_argument(
        "--write-baseline", action="store_true",
        help="regenerate the exceptions file from the current tree for a reviewed update (keeps reasons)",
    )
    args = parser.parse_args(argv)
    facts = analyze(args.root.resolve())
    previous = {}
    if args.exceptions.is_file():
        previous = json.loads(args.exceptions.read_text(encoding="utf-8"))
        if previous.get("schema") != SCHEMA:
            sys.stderr.write(f"{args.exceptions}: expected schema {SCHEMA}\n")
            return 2
    if args.write_baseline:
        baseline = build_baseline(facts, previous)
        args.exceptions.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8", newline="\n")
        unclassified = [entry for entry in baseline["module_honesty"] if not entry["reason"]]
        sys.stdout.write(
            f"wrote {args.exceptions}: {len(baseline['module_honesty'])} module-honesty exceptions, "
            f"{len(unclassified)} needing a written reason\n"
        )
        return 0
    report = evaluate(facts, normalize(previous))
    sys.stdout.write(json.dumps(report, indent=2) + "\n" if args.json else render(report))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
