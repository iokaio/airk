# SPDX-License-Identifier: Apache-2.0
"""Milestone S4 mechanical step: replace hidden `#[path]` mounts with files at
their canonical module locations (docs/simplify-plan.md S4).

Every Rust file's canonical location follows from its module path. A child
of a crate root or `mod.rs` file lives beside it as `name.rs`. A child of any
other `parent.rs` lives in `parent/name.rs`. Moving a file there and deleting
its `#[path]` attribute changes no module path, so test identities and item
paths stay identical.

Kept as they are, and reported:

- thin platform switches (`#[cfg(windows)]` / `_windows.rs` / `_unix.rs`
  mounts). Their files move with the directory of the file that declares them;
- `include!` of Rust source, and every file that uses or is reached through
  one. The embed-pool scheme needs feature-build checks, so it is a separate
  step;
- mounts that leave their crate target (`../` mounts from `tests/` into
  `src/`), files reached from more than one declaration, mounts inside inline
  module blocks, and any move whose destination already exists.

The script also updates markdown links that name a moved file, and exact
repository-relative path strings in code, tests and scripts. Markdown prose
is left unchanged, because it is often dated history.

    python scripts/dev/s4_module_honesty.py --plan     # dry run: list moves
    python scripts/dev/s4_module_honesty.py --apply    # git mv + edits
    python scripts/dev/s4_module_honesty.py --check    # exit 1 if a movable mount remains
"""
import argparse
from dataclasses import dataclass, field
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
MOD_DECL = re.compile(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*([;{])")
PATH_ATTR = re.compile(r'^\s*#\[path\s*=\s*"([^"]+)"\]\s*$')
CFG_ATTR = re.compile(r"#\[cfg(?:_attr)?\((.*)\)\]")
INCLUDE_RS = re.compile(r'include!\(\s*"([^"]+\.rs)"\s*\)')
INCLUDE_DATA = re.compile(r'(include_(?:str|bytes)!\(\s*")([^"]+)("\s*\))')
PLATFORM_FILE = re.compile(r"_(windows|unix|macos|linux)\.rs$")
PLATFORM_CFG = re.compile(r"\b(windows|unix|target_os|target_family)\b")
AUTO_TARGET_DIRS = {"tests", "benches", "examples"}
TEXT_SUFFIXES = {".rs", ".py", ".ts", ".cjs", ".js", ".mjs", ".json", ".toml", ".yml", ".yaml", ".ps1", ".sh", ".md"}


@dataclass
class Declaration:
    parent: Path            # declaring file
    line: int               # 1-based line of `mod`
    attribute_line: int | None  # 1-based line of `#[path]`, when present
    name: str
    value: str | None       # `#[path]` value
    platform: bool
    inline: bool            # declared inside an inline module block
    child: Path             # resolved file


@dataclass
class Node:
    path: Path
    declarations: list = field(default_factory=list)
    roots: set = field(default_factory=set)  # crate target roots reaching it
    included: bool = False
    has_include: bool = False


def rel(path):
    return path.resolve().relative_to(ROOT).as_posix()


def targets(crate):
    roots = []
    for candidate in (crate / "src" / "lib.rs", crate / "src" / "main.rs", crate / "build.rs"):
        if candidate.is_file():
            roots.append(candidate)
    manifest = (crate / "Cargo.toml").read_text(encoding="utf-8")
    for value in re.findall(r'^\s*path\s*=\s*"([^"]+\.rs)"', manifest, re.M):
        if (crate / value).is_file():
            roots.append(crate / value)
    for folder in ("src/bin", "tests", "benches", "examples"):
        base = crate / folder
        if base.is_dir():
            roots.extend(sorted(child for child in base.iterdir() if child.suffix == ".rs"))
            roots.extend(sorted(child / "main.rs" for child in base.iterdir() if (child / "main.rs").is_file()))
    unique = []
    for path in roots:
        if path.resolve() not in [item.resolve() for item in unique]:
            unique.append(path)
    return [path.resolve() for path in unique]


def walk(crate):
    nodes = {}
    roots = targets(crate)

    def node(path):
        return nodes.setdefault(path, Node(path))

    def visit(path, root, mod_rs_like, via_include=False):
        if not path.is_file():
            return
        current = node(path)
        seen = root in current.roots
        current.roots.add(root)
        current.included |= via_include
        if seen:
            return
        lines = path.read_text(encoding="utf-8").splitlines()
        child_base = path.parent if mod_rs_like else path.parent / path.stem
        inline, depth, pending_path, pending_line, pending_cfg = [], 0, None, None, []
        for number, line in enumerate(lines, 1):
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            attribute = PATH_ATTR.match(line)
            if attribute:
                pending_path, pending_line = attribute.group(1), number
            cfg = CFG_ATTR.search(line)
            if cfg:
                pending_cfg.append(cfg.group(1))
            for include in INCLUDE_RS.finditer(line):
                current.has_include = True
                visit((path.parent / include.group(1)).resolve(), root, mod_rs_like, True)
            declaration = MOD_DECL.match(line)
            if declaration:
                _, name, terminator = declaration.groups()
                if terminator == ";":
                    prefix = [entry[0] for entry in inline]
                    if pending_path:
                        base = path.parent if (mod_rs_like or not prefix) else child_base
                        child = (base.joinpath(*prefix) / pending_path).resolve()
                        child_mod_rs = True
                        platform = bool(PLATFORM_FILE.search(pending_path)) or any(PLATFORM_CFG.search(value) for value in pending_cfg)
                    else:
                        base = child_base.joinpath(*prefix)
                        flat = base / f"{name}.rs"
                        child, child_mod_rs = (flat.resolve(), False) if flat.is_file() else ((base / name / "mod.rs").resolve(), True)
                        platform = False
                    node(child).declarations.append(Declaration(path, number, pending_line if pending_path else None, name,
                                                                pending_path, platform, bool(prefix), child))
                    visit(child, root, child_mod_rs)
                else:
                    inline.append((name, depth))
                pending_path, pending_cfg = None, []
            elif stripped and not stripped.startswith("#"):
                pending_path, pending_cfg = None, []
            depth += line.count("{") - line.count("}")
            while inline and depth <= inline[-1][1]:
                inline.pop()

    for root in roots:
        visit(root, root, True)
    return nodes, roots


def is_mod_rs_like(path, roots):
    return path.name == "mod.rs" or path in roots


def plan(crate):
    """Return (moves {old: new}, attribute removals [(file, line)], path rewrites, skipped [(file, reason)])."""
    nodes, roots = walk(crate)
    final = {}       # file -> final location
    removals = []    # (declaring file, attribute line)
    rewrites = []    # (declaring file, attribute line, child) whose value is recomputed after moves
    skipped = []

    def blocked(current):
        if len(current.declarations) != 1:
            return "reached from more than one declaration"
        if current.included or current.has_include:
            return "uses or is reached through include!"
        declaration = current.declarations[0]
        if declaration.inline:
            return "declared inside an inline module block"
        if declaration.value and declaration.value.startswith(".."):
            return "mount leaves its directory (../)"
        parent = declaration.parent
        if parent in roots and parent.parent.name in AUTO_TARGET_DIRS and parent.parent.parent == crate.resolve():
            # Canonical children of tests/x.rs would be tests/y.rs, which Cargo
            # auto-discovers as another target. #[path] is the honest form here.
            return "child of an auto-discovered target root (tests/, benches/, examples/)"
        if parent in roots and parent.parent.name == "bin" and parent.parent.parent.name == "src":
            return "child of an auto-discovered src/bin target root"
        if len({rel(root).split("/")[2] if rel(root).count("/") > 2 else rel(root) for root in current.roots}) > 1:
            return "compiled into more than one crate target"
        return None

    # Files that stay loaded through a #[path] attribute. rustc resolves the
    # plain `mod` children of such a file beside it, as for `mod.rs`.
    kept_path = {}

    def keep(path, declaration, parent_final, reason):
        final[path] = path
        kept_path[path] = declaration.value is not None
        if declaration.value:
            skipped.append((rel(path), reason))
            if parent_final != declaration.parent:
                rewrites.append((declaration.parent, declaration.attribute_line, path))
        return path

    def place(path):
        if path in final:
            return final[path]
        current = nodes.get(path)
        if path in roots or current is None or not current.declarations:
            final[path] = path
            kept_path[path] = False
            return path
        declaration = current.declarations[0]
        parent_final = place(declaration.parent)
        if kept_path.get(declaration.parent):
            # Keep this child's declaration form and its position relative to
            # its #[path]-loaded parent; follow only the parent's directory.
            target = Path(os.path.normpath(parent_final.parent / os.path.relpath(path, declaration.parent.parent)))
            final[path] = target
            kept_path[path] = declaration.value is not None
            if declaration.value:
                skipped.append((rel(path), "child of a file loaded through a kept #[path]"))
            return target
        if declaration.platform:
            # Platform switches keep their attribute and follow their declaring file's directory.
            final[path] = Path(os.path.normpath(parent_final.parent / declaration.value))
            kept_path[path] = True
            return final[path]
        reason = blocked(current)
        if reason:
            return keep(path, declaration, parent_final, reason)
        parent_is_mod_rs = parent_final.name == "mod.rs" or parent_final in roots
        directory = parent_final.parent if parent_is_mod_rs else parent_final.parent / parent_final.stem
        target = directory / f"{declaration.name}.rs"
        if path.name == "mod.rs" and declaration.value is None:
            target = directory / declaration.name / "mod.rs"
        if target != path and target.exists():
            # Another file already owns the canonical location (for example a
            # feature-switched alternative implementation of the same module).
            return keep(path, declaration, parent_final, f"canonical location occupied: {target.relative_to(ROOT).as_posix()}")
        final[path] = target
        kept_path[path] = False
        if declaration.value:
            removals.append((declaration.parent, declaration.attribute_line))
        return target

    for path in sorted(nodes):
        place(path)
    moves = {old: new for old, new in final.items() if old != new}
    if len(set(moves.values())) != len(moves):
        raise SystemExit(f"{crate.name}: two files would move to one destination")
    return moves, removals, rewrites, skipped, final


def relative(from_directory, to_path):
    # os.path.relpath returns native separators; Rust paths and links use '/'.
    return os.path.relpath(to_path, from_directory).replace(os.sep, "/")


def tracked_files():
    output = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    return [ROOT / line for line in output.splitlines() if Path(line).suffix in TEXT_SUFFIXES]


def apply(crates):
    all_moves, all_removals, all_rewrites, all_finals = {}, [], [], {}
    for crate in crates:
        moves, removals, rewrites, _, final = plan(crate)
        all_moves.update(moves)
        all_removals.extend(removals)
        all_rewrites.extend(rewrites)
        all_finals.update(final)
    # 1. Edit declaring files in place (before moving them): drop attributes, recompute kept paths.
    edits = {}
    for parent, line in all_removals:
        edits.setdefault(parent, {})[line] = None
    for parent, line, child in all_rewrites:
        new_parent = all_finals.get(parent, parent)
        new_child = all_finals.get(child, child)
        edits.setdefault(parent, {})[line] = relative(new_parent.parent, new_child)
    for parent, changes in edits.items():
        lines = parent.read_text(encoding="utf-8").split("\n")
        for line, value in sorted(changes.items(), reverse=True):
            if value is None:
                assert PATH_ATTR.match(lines[line - 1]), (parent, line)
                del lines[line - 1]
            else:
                lines[line - 1] = re.sub(r'"[^"]+"', lambda _match, value=value: f'"{value}"', lines[line - 1], count=1)
        parent.write_bytes("\n".join(lines).encode("utf-8"))
    # 2. Rewrite relative include_str!/include_bytes! in every crate Rust file:
    #    both the including file and its target may move.
    resolved_moves = {old.resolve(): new for old, new in all_moves.items()}
    for source in sorted((ROOT / "crates").rglob("*.rs")):
        if "target" in source.relative_to(ROOT).parts:
            continue
        text = source.read_text(encoding="utf-8")
        new_source = resolved_moves.get(source.resolve(), source)
        def fix(match, source=source, new_source=new_source):
            target = (source.parent / match.group(2)).resolve()
            final_target = resolved_moves.get(target, target)
            if new_source == source and final_target == target:
                return match.group(0)
            return match.group(1) + relative(new_source.parent, final_target) + match.group(3)
        updated = INCLUDE_DATA.sub(fix, text)
        if updated != text:
            source.write_bytes(updated.encode("utf-8"))
    # 3. Move files with history.
    for old, new in sorted(all_moves.items(), key=lambda item: len(item[1].parts)):
        new.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "mv", rel(old), new.relative_to(ROOT).as_posix()], cwd=ROOT, check=True)
    relink(all_moves)
    return all_moves


def staged_renames():
    """Old -> new paths of the renames staged against HEAD (for --relink)."""
    output = subprocess.run(["git", "diff", "--cached", "-M50%", "--name-status", "HEAD"], cwd=ROOT,
                            capture_output=True, text=True, check=True).stdout
    moves = {}
    for line in output.splitlines():
        parts = line.split("	")
        if parts[0].startswith("R") and parts[1].endswith(".rs"):
            moves[(ROOT / parts[1]).resolve()] = (ROOT / parts[2]).resolve()
    return moves


def relink(all_moves):
    # 4. Update markdown links and exact repository-relative path strings elsewhere.
    by_old = {old.resolve(): new for old, new in all_moves.items()}
    strings = {rel_old: new.relative_to(ROOT).as_posix() for rel_old, new in ((old.relative_to(ROOT).as_posix(), new) for old, new in all_moves.items())}
    pattern = re.compile("|".join(re.escape(item) for item in sorted(strings, key=len, reverse=True))) if strings else None
    link = re.compile(r"(\]\()([^)#\s]+)((?:#[^)]*)?\))")
    for path in tracked_files():
        # Captured records describe the tree they were taken from.
        if not path.is_file() or "docs/simplify/records/" in rel(path):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue  # Not UTF-8 text (for example a legacy-encoded fixture); never rewritten.
        updated = text
        if path.suffix == ".md":
            def fix_link(match, path=path):
                target = match.group(2)
                if re.match(r"^[a-z]+:", target):
                    return match.group(0)
                resolved = (path.parent / target).resolve()
                if resolved in by_old:
                    return match.group(1) + relative(path.parent, by_old[resolved]) + match.group(3)
                return match.group(0)
            updated = link.sub(fix_link, updated)
        # Markdown prose is often dated history; only its links are rewritten.
        if pattern and path.suffix != ".md":
            updated = pattern.sub(lambda match: strings[match.group(0)], updated)
        if updated != text:
            path.write_bytes(updated.encode("utf-8"))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--plan", action="store_true")
    mode.add_argument("--apply", action="store_true")
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--relink", action="store_true", help="rerun link/path updates for staged renames")
    args = parser.parse_args(argv)
    if args.relink:
        moves = staged_renames()
        relink(moves)
        print(f"relinked {len(moves)} staged renames")
        return 0
    crates = sorted(path for path in (ROOT / "crates").iterdir() if (path / "Cargo.toml").is_file())
    total_moves, total_skipped = 0, []
    for crate in crates:
        moves, removals, rewrites, skipped, _ = plan(crate)
        total_moves += len(moves)
        total_skipped.extend(skipped)
        if args.plan:
            for old, new in sorted(moves.items()):
                print(f"move {rel(old)} -> {new.relative_to(ROOT).as_posix()}")
            print(f"{crate.name}: {len(moves)} moves, {len(removals)} #[path] removals, {len(rewrites)} kept-path rewrites")
    for path, reason in sorted(set(total_skipped)):
        print(f"kept {path}: {reason}")
    if args.check:
        print(f"movable files remaining: {total_moves}")
        return 1 if total_moves else 0
    if args.apply:
        moves = apply(crates)
        print(f"applied {len(moves)} moves")
    return 0


if __name__ == "__main__":
    sys.exit(main())
