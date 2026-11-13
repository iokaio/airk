# SPDX-License-Identifier: Apache-2.0
"""Move one module group of `airk-cli` into its own workspace crate (docs/simplify/airk-cli-split.md).

    python scripts/dev/extract_crate.py --group os --crate airk-host [--dry-run]

Behavior-preserving and mechanical, so a reviewer can re-run it on the parent commit:

* each module file or folder of the group moves (`git mv`) to `crates/<crate>/src/`, at the
  same folder depth, so relative `include_str!` paths keep resolving;
* the new `lib.rs` declares each module `pub` with its original attributes (for example a
  `#[cfg(...)]`), plus the `ipc` aliases `airk-cli` defines;
* `airk-cli/src/lib.rs` replaces each declaration with a re-export of the same visibility
  (`pub use` / `pub(crate) use`), so every `crate::<module>` path inside `airk-cli` and every
  `airk_cli::<module>` path in its integration tests keeps compiling unchanged;
* inside the moved files, `crate::<m>` and `airk_cli::<m>` for a module of an earlier extracted
  crate become `airk_<that crate>::<m>`; for a module of this group, `crate::<m>`; a reference to
  a module still in `airk-cli` stops the script (a back edge);
* the new manifest takes `airk-cli`'s dependency set and features (a superset, never a new
  third-party identity) plus the crates extracted before it; `airk-cli` depends on the new crate
  and forwards `local-inference`.

Visibility is widened afterwards only for items another crate uses (`scripts/dev/widen_visibility.py`
on the compiler's privacy errors). `build.rs` pieces are moved by hand and recorded in the plan.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "crates" / "airk-cli"
GROUPS = ROOT / "docs" / "simplify" / "airk-cli-groups.json"
CRATES = {"os": "airk-host", "endpoint": "airk-endpoint", "inference": "airk-inference",
          "catalog": "airk-catalog-control", "supervisor": "airk-supervisor", "installer": "airk-installer",
          "frontend": "airk-frontend", "https": "airk-https", "service": "airk-service"}
ALIASES = {"ipc": "pub use airk_ipc as ipc;", "local_ipc": "pub use ipc::endpoint as local_ipc;",
           "service_ipc": "pub use ipc::frame as service_ipc;"}
DECLARATION = re.compile(r"^(?P<vis>pub(?:\([^)]*\))?\s+)?mod\s+(?P<name>[a-z0-9_]+)\s*;\s*$")


def ident(crate):
    return crate.replace("-", "_")


def declarations(text):
    """Each `mod` declaration of lib.rs with the attribute and doc lines directly above it."""
    lines = text.split("\n")
    found = {}
    for number, line in enumerate(lines):
        match = DECLARATION.match(line.strip())
        if not match:
            continue
        start = number
        while start > 0 and lines[start - 1].strip().startswith(("#[", "///")):
            start -= 1
        found[match["name"]] = (start, number, (match["vis"] or "").strip())
    return lines, found


def extracted_before(order, group):
    """Groups listed before `group` whose crate already exists."""
    return [g for g in order[:order.index(group)] if (ROOT / "crates" / CRATES[g]).is_dir()]


def rewrite(text, owner, group, earlier):
    """Paths inside moved code: see the module docstring."""
    problems = []

    def replace(match):
        prefix, name = match.group(1), match.group(2)
        if name in ALIASES or name == "owner_acl":
            return f"crate::{name}"
        home = owner.get(name)
        if home is None:
            return match.group(0)
        if home == group:
            return f"crate::{name}"
        if home in earlier:
            return f"{ident(CRATES[home])}::{name}"
        problems.append(f"{prefix}::{name} -> group {home} (still in airk-cli)")
        return match.group(0)

    text = re.sub(r"\b(crate|airk_cli)::([a-z0-9_]+)", replace, text)
    # `airk_cli::{a, b}` names the old crate root; in the new crate that is `crate::{…}`
    # (each named module resolves through the aliases `lib.rs` declares).
    text = re.sub(r"\bairk_cli::\{", "crate::{", text)
    return text, problems


def manifest(crate, group, earlier, cli_manifest):
    """The new crate's Cargo.toml: airk-cli's dependency tables, earlier crates, the same features."""
    sections = re.split(r"(?m)^(?=\[)", cli_manifest)
    keep = []
    for section in sections:
        header = section.split("\n", 1)[0].strip()
        if header in ("[dependencies]", "[dev-dependencies]") or header.startswith("[target."):
            keep.append(section.rstrip() + "\n")
    deps = "".join(f"{CRATES[g]}.workspace = true\n" for g in earlier)
    # Test helpers of earlier crates (`cfg(any(test, feature = "test-support"))`) for this crate's tests.
    dev = "".join(f'{CRATES[g]} = {{ workspace = true, features = ["test-support"] }}\n' for g in earlier)
    forwarded = ", ".join(['"airk-kernel/local-inference"', '"airk-retrieval/local-inference"', '"bundled-model"']
                          + [f'"{CRATES[g]}/local-inference"' for g in earlier])
    support = ", ".join(f'"{CRATES[g]}/test-support"' for g in earlier)
    body = "\n".join(keep)
    # airk-cli already depends on the crates extracted before; list each once, below.
    for name in CRATES.values():
        body = re.sub(rf"(?m)^{re.escape(name)}(?:\.workspace = true| = \{{[^\n]*\}})\n", "", body)
    body = body.replace("[dependencies]\n", "[dependencies]\n" + deps, 1)
    if "[dev-dependencies]\n" in body:
        body = body.replace("[dev-dependencies]\n", "[dev-dependencies]\n" + dev, 1)
    else:
        body += "\n[dev-dependencies]\n" + dev
    # The dev-only airkctl edge belongs to airk-cli's parity tests.
    body = re.sub(r"(?m)^# Test-only: the parity test.*\n(?:#.*\n)*airkctl = .*\n", "", body)
    return (f"# SPDX-License-Identifier: Apache-2.0\n[package]\nname = \"{crate}\"\n"
            f"description = \"AIRK service: the {group} part of the former airk-cli crate\"\n"
            "version.workspace = true\nedition.workspace = true\nrust-version.workspace = true\n"
            "license.workspace = true\nrepository.workspace = true\npublish.workspace = true\n\n"
            f"[features]\ndefault = []\nlocal-inference = [{forwarded}]\nbundled-model = []\n"
            f"parser-training = []\n# Test helpers for the tests of crates built on this one (never production).\n"
            f"test-support = [{support}]\n\n" + body)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--group", required=True, choices=sorted(CRATES))
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    groups = json.loads(GROUPS.read_text(encoding="utf-8"))
    order, members = groups["order"], groups["groups"]
    group, crate = args.group, CRATES[args.group]
    owner = {module: g for g, names in members.items() for module in names}
    earlier = extracted_before(order, group)
    target = ROOT / "crates" / crate
    if target.exists():
        raise SystemExit(f"{target} exists")
    lib_text = (CLI / "src" / "lib.rs").read_text(encoding="utf-8")
    lines, found = declarations(lib_text)
    modules = [m for m in members[group] if m in found]
    missing = [m for m in members[group] if m not in found]
    if missing:
        raise SystemExit(f"not declared in airk-cli/src/lib.rs: {missing}")

    # Plan every moved file and its rewritten text before touching anything.
    moves, rewritten, problems = [], {}, []
    for module in modules:
        for path in [CLI / "src" / f"{module}.rs", CLI / "src" / module]:
            if path.exists():
                moves.append(path)
                files = [path] if path.is_file() else sorted(path.rglob("*.rs"))
                for file in files:
                    text, issues = rewrite(file.read_text(encoding="utf-8"), owner, group, earlier)
                    rewritten[file] = text
                    problems += [f"{file.relative_to(ROOT)}: {issue}" for issue in issues]
    if problems:
        print("\n".join(sorted(set(problems))), file=sys.stderr)
        raise SystemExit(f"{len(set(problems))} references to modules still in airk-cli; fix the back edges first")
    print(f"{crate}: {len(modules)} modules, {len(rewritten)} files; earlier crates: "
          f"{[CRATES[g] for g in earlier]}")
    if args.dry_run:
        return 0

    (target / "src").mkdir(parents=True)
    for path in moves:
        subprocess.run(["git", "mv", str(path), str(target / "src" / path.name)], cwd=ROOT, check=True)
    for old, text in rewritten.items():
        new = target / "src" / old.relative_to(CLI / "src")
        new.write_text(text, encoding="utf-8", newline="")

    # New lib.rs: the declarations, made `pub`, with their attributes.
    used_aliases = {name for name in list(ALIASES) + ["owner_acl"]
                    if any(f"crate::{name}" in text for text in rewritten.values())}
    header = ["// SPDX-License-Identifier: Apache-2.0",
              f"//! The {group} part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).",
              f"extern crate self as {ident(crate)};"]
    # Grouped imports (`use crate::{a, b}`) are not rewritten; alias each earlier-crate module
    # they name so `crate::<module>` keeps resolving in the moved code.
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import module_graph
    grouped = set()
    for text in rewritten.values():
        grouped.update(module_graph.grouped(text))
    header += [f"pub(crate) use {ident(CRATES[owner[name]])}::{name};"
               for name in sorted(grouped) if owner.get(name) in earlier]
    header += [ALIASES[name] for name in ("ipc", "local_ipc", "service_ipc") if name in used_aliases or
               ("ipc" == name and {"local_ipc", "service_ipc"} & used_aliases)]
    if "owner_acl" in used_aliases:
        header += ["#[cfg(windows)]", "pub mod owner_acl {", "    pub use crate::local_ipc::require_owner_private;", "}"]
    body = []
    for module in modules:
        start, number, _visibility = found[module]
        attributes = [line.replace("#[cfg(test)]", '#[cfg(any(test, feature = "test-support"))]')
                      for line in lines[start:number]]
        body += attributes + [f"pub mod {module};"]
    (target / "src" / "lib.rs").write_text("\n".join(header + body) + "\n", encoding="utf-8", newline="")

    # airk-cli lib.rs: each declaration becomes a re-export of the same visibility.
    for module in sorted(modules, key=lambda m: -found[m][1]):
        start, number, visibility = found[module]
        export = f"{visibility + ' ' if visibility else 'pub(crate) '}use {ident(crate)}::{module};"
        lines[number] = export
    (CLI / "src" / "lib.rs").write_text("\n".join(lines), encoding="utf-8", newline="")

    cli_manifest = (CLI / "Cargo.toml").read_text(encoding="utf-8")
    (target / "Cargo.toml").write_text(manifest(crate, group, earlier, cli_manifest), encoding="utf-8",
                                       newline="")
    cli_manifest = cli_manifest.replace("[dependencies]\n", f"[dependencies]\n{crate}.workspace = true\n", 1)
    cli_manifest = cli_manifest.replace(
        "[dev-dependencies]\n",
        f'[dev-dependencies]\n{crate} = {{ workspace = true, features = ["test-support"] }}\n', 1)
    cli_manifest = re.sub(r'(?m)^(local-inference = \[)(.*)\]$',
                          lambda m: f'{m.group(1)}{m.group(2)}, "{crate}/local-inference"]', cli_manifest)
    (CLI / "Cargo.toml").write_text(cli_manifest, encoding="utf-8", newline="")
    workspace = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    workspace = workspace.replace('airk-mcp = { path = "crates/airk-mcp" }\n',
                                  f'airk-mcp = {{ path = "crates/airk-mcp" }}\n{crate} = {{ path = "crates/{crate}" }}\n', 1)
    (ROOT / "Cargo.toml").write_text(workspace, encoding="utf-8", newline="")
    print(f"moved {len(moves)} paths into crates/{crate}; next: build.rs pieces, then widen_visibility.py")
    return 0


if __name__ == "__main__":
    sys.exit(main())
