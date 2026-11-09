# SPDX-License-Identifier: Apache-2.0
"""Report-only structural metrics for docs/simplify-plan.md section 8.1.

Nothing here is a gate: the script always exits 0 after printing a report,
unless its own arguments are invalid. The figures describe source structure,
not runtime behavior. Grep-based counts are approximate by construction; the
module walk follows `mod` declarations, `#[path]` attributes and `include!`
of `.rs` files from every Cargo target root, so orphan and cycle figures are
derived from the module tree that rustc would see (ignoring `cfg`).
"""
import argparse
from collections import Counter, defaultdict
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SCHEMA = "airk-simplify-metrics/1"

MOD_DECL = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*([;{])")
PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')
CFG_ATTR = re.compile(r"#\[cfg\((.*)\)\]")
INCLUDE_RS = re.compile(r'include!\(\s*"([^"]+\.rs)"\s*\)')
PLATFORM_FILE = re.compile(r"_(windows|unix|macos|linux)\.rs$")
PLATFORM_CFG = re.compile(r"\b(windows|unix|target_os|target_family)\b")
FN_START = re.compile(
    r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?"
    r'(?:extern\s+"[^"]*"\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)'
)
PUB_MOD = re.compile(r"^\s*pub\s+mod\s+[A-Za-z_]")
TIME_READ = re.compile(r"\b(?:Instant|SystemTime)::now\(\)")
SLEEP = re.compile(r"\b(?:thread|time)::sleep\b")
STATIC = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?static\s+(mut\s+)?[A-Za-z_][A-Za-z0-9_]*\s*:\s*(.*)")
MUTABLE_STATIC_TYPE = re.compile(r"\b(?:Mutex|RwLock|Atomic\w+|OnceLock|OnceCell|LazyLock|Lazy|RefCell|Cell)\b")
THREAD_LOCAL = re.compile(r"\bthread_local!")
REGISTRY_OPEN = re.compile(r"\bServiceRegistry::open\w*")
CRATE_REF = re.compile(r"\bcrate::([a-z_][a-z0-9_]*)")
CRATE_GROUP = re.compile(r"\bcrate::\{")
SUPER_REF = re.compile(r"(?<![:\w])super::([a-z_][a-z0-9_]*)")
TOOL_ATTR = re.compile(r"#\[tool\(")
TOOL_FN = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)\s*\(")
HARNESS_SUFFIXES = {".py", ".ps1", ".cjs", ".js", ".mjs", ".sh", ".ts"}
# Build output and copies inside artifacts/ are not authored harness scripts.
HARNESS_SKIP_DIRS = {"node_modules", "target", "__pycache__", "out", "extension", "extensions", "dist", "worktrees", "runtime"}
SKIP_DIRS = {"target", "node_modules", ".git", "temp", "private", "artifacts", "out", "__pycache__"}


def is_test_path(relative):
    """Test files by name, matching the plan's "non-test-file" definition."""
    parts = relative.split("/")
    name = parts[-1]
    return "tests" in parts[:-1] or name == "tests.rs" or name.endswith("_tests.rs") or name.endswith("_test.rs")


def read_text(path):
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def rel(path, root):
    return path.resolve().relative_to(root.resolve()).as_posix()


class Crate:
    """One workspace member: its target roots and module tree."""

    def __init__(self, directory, root):
        self.directory = directory
        self.root = root
        self.name = directory.name
        self.targets = self._targets()
        self.reached = {}  # file -> set of (target, top-level module or None, test-only)
        self.path_mounts = []  # (file, line, value, platform)
        self.includes = []  # (file, line, value)

    def _targets(self):
        manifest = read_text(self.directory / "Cargo.toml")
        targets = []
        lib = self.directory / "src" / "lib.rs"
        if lib.is_file():
            targets.append(("lib", lib))
        main = self.directory / "src" / "main.rs"
        explicit = re.findall(r'^\s*path\s*=\s*"([^"]+\.rs)"', manifest, re.M)
        for value in explicit:
            candidate = (self.directory / value).resolve()
            if candidate.is_file() and candidate != lib.resolve():
                targets.append((f"target:{value}", candidate))
        if main.is_file() and all(path.resolve() != main.resolve() for _, path in targets):
            targets.append(("bin", main))
        for folder in ("src/bin", "tests", "benches", "examples"):
            base = self.directory / folder
            if base.is_dir():
                for child in sorted(base.iterdir()):
                    if child.suffix == ".rs" and child.is_file():
                        targets.append((f"{folder}:{child.name}", child))
                    elif child.is_dir() and (child / "main.rs").is_file():
                        targets.append((f"{folder}:{child.name}", child / "main.rs"))
        build = self.directory / "build.rs"
        if build.is_file():
            targets.append(("build", build))
        seen, unique = set(), []
        for label, path in targets:
            key = path.resolve()
            if key not in seen:
                seen.add(key)
                unique.append((label, path))
        return unique

    def walk(self):
        for label, path in self.targets:
            self._visit(path.resolve(), label, None, True, False)

    def _visit(self, path, target, top, mod_rs_like, test_only):
        if not path.is_file():
            return
        owners = self.reached.setdefault(path, set())
        key = (target, top, test_only)
        if key in owners:
            return
        owners.add(key)
        text = read_text(path)
        lines = text.splitlines()
        directory = path.parent
        # Directory for child modules without #[path].
        child_base = directory if mod_rs_like else directory / path.stem
        inline = []  # stack of (name, brace depth at open, test-only)
        depth = 0
        pending_path = None
        pending_cfg = []
        relative = rel(path, self.root)
        for number, line in enumerate(lines, 1):
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            match = PATH_ATTR.search(line)
            if match:
                pending_path = match.group(1)
            cfg = CFG_ATTR.search(line)
            if cfg:
                pending_cfg.append(cfg.group(1))
            for include in INCLUDE_RS.finditer(line):
                self.includes.append((relative, number, include.group(1)))
                within_test = test_only or any(entry[2] for entry in inline)
                self._visit((directory / include.group(1)).resolve(), target, top, mod_rs_like, within_test)
            declaration = MOD_DECL.match(line)
            if declaration:
                name, terminator = declaration.groups()
                prefix = [entry[0] for entry in inline]
                child_test = test_only or any(entry[2] for entry in inline) or any(map(is_test_cfg, pending_cfg))
                if terminator == ";":
                    if pending_path:
                        platform = bool(PLATFORM_FILE.search(pending_path)) or any(
                            PLATFORM_CFG.search(value) for value in pending_cfg
                        )
                        self.path_mounts.append((relative, number, pending_path, platform))
                        base = directory if (mod_rs_like or not prefix) else child_base
                        child = (base.joinpath(*prefix) / pending_path).resolve()
                        child_mod_rs = True
                    else:
                        base = child_base.joinpath(*prefix)
                        candidate = base / f"{name}.rs"
                        if candidate.is_file():
                            child, child_mod_rs = candidate.resolve(), False
                        else:
                            child, child_mod_rs = (base / name / "mod.rs").resolve(), True
                    child_top = top if top is not None or prefix else name
                    if top is None and prefix:
                        child_top = prefix[0]
                    self._visit(child, target, child_top, child_mod_rs, child_test)
                else:
                    inline.append((name, depth, child_test))
                pending_path = None
                pending_cfg = []
            elif stripped and not stripped.startswith("#"):
                pending_path = None
                pending_cfg = []
            depth += line.count("{") - line.count("}")
            while inline and depth <= inline[-1][1]:
                inline.pop()

    def top_level_modules(self, target="lib"):
        """Files of each top-level module reached without a test-only mount."""
        tops = defaultdict(set)
        for path, owners in self.reached.items():
            for owner_target, top, test_only in owners:
                if owner_target == target and top is not None and not test_only:
                    tops[top].add(path)
        return tops


TEST_CFG = re.compile(r"^\s*(?:test|all\((?:.*,)?\s*test\s*(?:,.*)?\))\s*$")
TEST_ATTR = re.compile(r"^\s*#\[cfg\((.*)\)\]\s*$")


def is_test_cfg(expression):
    """True for `test` or `all(..., test, ...)`; `any(..., test)` also compiles in production."""
    return bool(TEST_CFG.match(expression)) and not expression.strip().startswith("any(")


def production_text(text):
    """Drop items gated by a test-only `#[cfg]`, so graphs measure production edges."""
    kept, skipping, depth, opened = [], False, 0, False
    for line in text.splitlines():
        if not skipping:
            attribute = TEST_ATTR.match(line)
            if attribute and is_test_cfg(attribute.group(1)):
                skipping, depth, opened = True, 0, False
                continue
            kept.append(line)
            continue
        stripped = line.strip()
        if not opened and stripped.startswith("#"):
            continue
        depth += line.count("{") - line.count("}")
        opened = opened or "{" in line
        if (opened and depth <= 0) or (not opened and stripped.endswith(";")):
            skipping = False
    return "\n".join(kept)


def discover_crates(root):
    base = root / "crates"
    if not base.is_dir():
        return []
    crates = []
    for directory in sorted(base.iterdir()):
        if (directory / "Cargo.toml").is_file():
            crate = Crate(directory, root)
            crate.walk()
            crates.append(crate)
    return crates


def rust_files(root):
    found = []
    for directory, folders, files in os.walk(root / "crates"):
        folders[:] = [name for name in folders if name not in SKIP_DIRS]
        for name in files:
            if name.endswith(".rs"):
                found.append(Path(directory) / name)
    return sorted(found)


def function_lengths(lines):
    """Lengths of rustfmt-formatted functions: closing brace at the opening indentation."""
    lengths = []
    index = 0
    while index < len(lines):
        match = FN_START.match(lines[index])
        if not match:
            index += 1
            continue
        indent, name = match.groups()
        start = index
        # Find the end of the signature: a line ending with "{" or ";".
        cursor = index
        while cursor < len(lines):
            text = lines[cursor].rstrip()
            if text.endswith(";") and "{" not in text:
                cursor = -1
                break
            if text.endswith("{") or text.endswith("{}"):
                break
            cursor += 1
        if cursor < 0 or cursor >= len(lines) or lines[cursor].rstrip().endswith("{}"):
            index += 1
            continue
        closing = indent + "}"
        end = cursor + 1
        while end < len(lines) and lines[end].rstrip() != closing:
            end += 1
        if end < len(lines):
            lengths.append((name, start + 1, end - start + 1))
        index = cursor + 1
    return lengths


def strongly_connected(graph):
    index_of, low, stack, on_stack, components = {}, {}, [], set(), []
    counter = [0]

    def connect(node):
        index_of[node] = low[node] = counter[0]
        counter[0] += 1
        stack.append(node)
        on_stack.add(node)
        for target in sorted(graph.get(node, ())):
            if target not in index_of:
                connect(target)
                low[node] = min(low[node], low[target])
            elif target in on_stack:
                low[node] = min(low[node], index_of[target])
        if low[node] == index_of[node]:
            component = []
            while True:
                item = stack.pop()
                on_stack.discard(item)
                component.append(item)
                if item == node:
                    break
            components.append(sorted(component))

    limit = sys.getrecursionlimit()
    sys.setrecursionlimit(max(limit, 10000))
    try:
        for node in sorted(graph):
            if node not in index_of:
                connect(node)
    finally:
        sys.setrecursionlimit(limit)
    return components


def module_graph(crate, target="lib"):
    tops = crate.top_level_modules(target)
    names = set(tops)
    graph = {name: set() for name in names}
    files_referencing = defaultdict(set)
    for top, paths in tops.items():
        for path in paths:
            text = production_text(read_text(path))
            references = set(CRATE_REF.findall(text))
            for group in CRATE_GROUP.finditer(text):
                references.update(_group_heads(text, group.end() - 1))
            # A top-level module file's `super::x` names the crate root's x.
            if path.parent == crate.directory / "src" or path.name == "mod.rs":
                references.update(SUPER_REF.findall(text))
            for name in references:
                if name in names and name != top:
                    graph[top].add(name)
                    files_referencing[name].add(path)
    return graph, files_referencing


def _group_heads(text, brace):
    """First path segment of each entry of a `{...}` use group starting at `brace`."""
    depth, heads, token, at_entry_start = 0, [], "", True
    for character in text[brace:]:
        if character == "{":
            depth += 1
            if depth == 1:
                at_entry_start = True
                continue
        elif character == "}":
            depth -= 1
            if depth == 0:
                if token and at_entry_start:
                    heads.append(token)
                break
            continue
        if depth != 1:
            continue
        if character.isalnum() or character == "_":
            token += character
        else:
            if token and at_entry_start:
                heads.append(token)
                at_entry_start = False
            token = ""
            if character == ",":
                at_entry_start = True
    return heads


def commit_spans(root, count):
    try:
        output = subprocess.run(
            ["git", "log", f"-{count}", "--no-merges", "--name-only", "--format=@@%H"],
            cwd=root, capture_output=True, text=True, check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    commits, current = [], None
    for line in output.splitlines():
        if line.startswith("@@"):
            current = []
            commits.append(current)
        elif line.strip() and current is not None:
            current.append(line.strip())
    return commits


def component_of(path):
    parts = path.split("/")
    if len(parts) == 1:
        return "(root)"
    if parts[0] in {"crates", "packages"} and len(parts) > 2:
        return f"{parts[0]}/{parts[1]}"
    return parts[0]


def count_harness_scripts(root):
    base = root / "artifacts"
    total, unreadable = 0, 0
    if not base.is_dir():
        return 0, 0

    def failed(_error):
        nonlocal unreadable
        unreadable += 1

    for directory, folders, files in os.walk(base, onerror=failed):
        folders[:] = [name for name in folders if name not in HARNESS_SKIP_DIRS]
        total += sum(1 for name in files if Path(name).suffix in HARNESS_SUFFIXES)
    return total, unreadable


def read_tool_names(root):
    names = set()
    for path in sorted((root / "crates" / "airk-mcp" / "src").glob("*.rs")):
        lines = read_text(path).splitlines()
        for index, line in enumerate(lines):
            if TOOL_ATTR.search(line):
                for follow in lines[index:index + 12]:
                    match = TOOL_FN.search(follow)
                    if match:
                        names.add(match.group(1))
                        break
    return names


CONTEXT_FILES = ("CLAUDE.md", "README.md", "CONTRIBUTING.md", "docs/architecture.md")
CONTEXT_METHOD = "estimated tokens = ceil(bytes / 4) per file, summed; an approximation, not a tokenizer count"
CONTEXT_TARGET_TOKENS = 25000


def context_size(root):
    """Report-only estimate of the standing agent context (plan section 4.5, decision D9)."""
    files = {}
    for name in CONTEXT_FILES:
        path = root / name
        size = path.stat().st_size if path.is_file() else None
        files[name] = {"bytes": size, "estimated_tokens": None if size is None else (size + 3) // 4}
    present = [entry for entry in files.values() if entry["bytes"] is not None]
    return {
        "method": CONTEXT_METHOD,
        "files": files,
        "missing": [name for name, entry in files.items() if entry["bytes"] is None],
        "total_bytes": sum(entry["bytes"] for entry in present),
        "estimated_tokens": sum(entry["estimated_tokens"] for entry in present),
        "target_tokens": CONTEXT_TARGET_TOKENS,
    }


def collect(root, history=True, harness=True):
    crates = discover_crates(root)
    report = {"schema": SCHEMA, "root": str(root), "metrics": {}}
    metrics = report["metrics"]

    # Module honesty.
    mounts = [mount for crate in crates for mount in crate.path_mounts]
    unique_mounts = {(file, line) for file, line, _, platform in mounts if not platform}
    unique_platform = {(file, line) for file, line, _, platform in mounts if platform}
    includes = {(file, line) for crate in crates for file, line, _ in crate.includes}
    reached = {path for crate in crates for path in crate.reached}
    all_rust = rust_files(root)
    orphans = []
    for path in all_rust:
        relative = rel(path, root)
        if "/fixtures/" in relative or relative.endswith("/build.rs"):
            if path.resolve() in reached or "/fixtures/" in relative:
                continue
        if path.resolve() not in reached:
            orphans.append((relative, len(read_text(path).splitlines())))
    metrics["module_honesty"] = {
        "path_mounts_non_platform": len(unique_mounts),
        "path_mounts_platform": len(unique_platform),
        "path_mounts_by_crate": dict(Counter(file.split("/")[1] for file, _ in unique_mounts)),
        "include_rs": len(includes),
        "orphan_files": len(orphans),
        "orphan_lines": sum(lines for _, lines in orphans),
        "orphans": [file for file, _ in orphans],
    }

    # Sizes, functions, lines, time and statics.
    crate_lines, crate_prod_lines = Counter(), Counter()
    big_files, long_fns_300, long_fns_150 = [], [], []
    pub_mods = 0
    long_lines = 0
    time_prod = time_test = sleep_prod = sleep_test = 0
    statics_prod = statics_test = thread_local_prod = thread_local_test = 0
    registry_calls, registry_files = 0, set()
    for path in all_rust:
        relative = rel(path, root)
        if "/fixtures/" in relative:
            continue
        text = read_text(path)
        lines = text.splitlines()
        crate = relative.split("/")[1]
        test = is_test_path(relative)
        crate_lines[crate] += len(lines)
        if not test:
            crate_prod_lines[crate] += len(lines)
            if len(lines) > 2000:
                big_files.append((relative, len(lines)))
            for name, start, length in function_lengths(lines):
                if length > 300:
                    long_fns_300.append((relative, name, start, length))
                if length > 150:
                    long_fns_150.append((relative, name, start, length))
        pub_mods += sum(1 for line in lines if PUB_MOD.match(line))
        long_lines += sum(1 for line in lines if len(line) > 200)
        times = len(TIME_READ.findall(text))
        sleeps = len(SLEEP.findall(text))
        statics = sum(
            1 for line in lines
            if (match := STATIC.match(line)) and (match.group(1) or MUTABLE_STATIC_TYPE.search(match.group(2)))
        )
        locals_ = len(THREAD_LOCAL.findall(text))
        if test:
            time_test += times
            sleep_test += sleeps
            statics_test += statics
            thread_local_test += locals_
        else:
            time_prod += times
            sleep_prod += sleeps
            statics_prod += statics
            thread_local_prod += locals_
        if crate == "airk-cli" and not test:
            found = REGISTRY_OPEN.findall(text)
            if found:
                registry_calls += len(found)
                registry_files.add(relative)
    metrics["size"] = {
        "rust_lines_by_crate": dict(sorted(crate_lines.items())),
        "non_test_file_lines_by_crate": dict(sorted(crate_prod_lines.items())),
        "crates_over_20k_non_test_lines": sorted(name for name, value in crate_prod_lines.items() if value > 20000),
        "files_over_2000_lines": len(big_files),
        "functions_over_300_lines": len(long_fns_300),
        "functions_over_150_lines": len(long_fns_150),
        "longest_functions": [
            {"file": file, "name": name, "line": start, "lines": length}
            for file, name, start, length in sorted(long_fns_300, key=lambda item: -item[3])[:10]
        ],
        "pub_mod_declarations": pub_mods,
        "rust_lines_over_200_chars": long_lines,
    }
    metrics["determinism"] = {
        "time_reads_production": time_prod,
        "time_reads_tests": time_test,
        "sleeps_production": sleep_prod,
        "sleeps_tests": sleep_test,
        "mutable_statics_production": statics_prod,
        "mutable_statics_tests": statics_test,
        "thread_local_production": thread_local_prod,
        "thread_local_tests": thread_local_test,
    }
    extension = root / "packages" / "vscode" / "src"
    date_now = timeouts = 0
    if extension.is_dir():
        for directory, folders, files in os.walk(extension):
            folders[:] = [name for name in folders if name not in SKIP_DIRS]
            for name in files:
                if not name.endswith((".ts", ".tsx")):
                    continue
                path = Path(directory) / name
                text = read_text(path)
                if "/test/" in rel(path, root) + "/":
                    timeouts += len(re.findall(r"\bsetTimeout\(|\bsleep\(", text))
                else:
                    date_now += len(re.findall(r"\bDate\.now\(\)", text))
    metrics["determinism"]["extension_date_now_production"] = date_now
    metrics["determinism"]["extension_timeouts_tests"] = timeouts
    metrics["coupling"] = {
        "cli_service_registry_open_calls": registry_calls,
        "cli_service_registry_open_files": len(registry_files),
    }

    # Module graphs of each library target.
    graphs = {}
    for crate in crates:
        graph, referencing = module_graph(crate)
        if not graph:
            continue
        cycles = [component for component in strongly_connected(graph) if len(component) > 1]
        fan_in = Counter({name: sum(1 for source in graph if name in graph[source]) for name in graph})
        graphs[crate.name] = {
            "top_level_modules": len(graph),
            "edges": sum(len(targets) for targets in graph.values()),
            "cyclic_components": len(cycles),
            "modules_in_cycles": sum(len(component) for component in cycles),
            "largest_cycle": len(max(cycles, key=len)) if cycles else 0,
            "fan_in_top": [
                {"module": name, "modules": value, "files": len(referencing[name])}
                for name, value in fan_in.most_common(8) if value
            ],
            "cycles": cycles,
        }
    metrics["module_graph"] = graphs

    # Duplicated read-tool catalogs.
    tools = read_tool_names(root)
    tables = []
    if tools:
        for base in (root / "crates", extension):
            if not base.is_dir():
                continue
            for directory, folders, files in os.walk(base):
                folders[:] = [name for name in folders if name not in SKIP_DIRS]
                for name in files:
                    if not name.endswith((".rs", ".ts")):
                        continue
                    path = Path(directory) / name
                    relative = rel(path, root)
                    if is_test_path(relative) or "/test/" in relative:
                        continue
                    text = read_text(path)
                    present = {
                        tool for tool in tools
                        if any(f"{quote}{prefix}{tool}{quote}" in text for quote in "\"'" for prefix in ("", "airk_"))
                    }
                    if len(present) >= 8:
                        tables.append(relative)
    metrics["catalogs"] = {"read_tools": len(tools), "files_with_8_or_more_read_tool_names": sorted(tables)}

    metrics["context_size"] = context_size(root)

    if history:
        commits = commit_spans(root, 200)
        if commits is not None:
            spans = [len({component_of(path) for path in files}) for files in commits]
            files_100 = [len(files) for files in commits[:100]]
            metrics["history"] = {
                "component_definition": "crates/<name>, packages/<name>, else top-level directory; root files are one component",
                "window_commits": len(commits),
                "share_3_plus_components": round(sum(span >= 3 for span in spans) / len(spans), 3) if spans else None,
                "share_5_plus_components": round(sum(span >= 5 for span in spans) / len(spans), 3) if spans else None,
                "files_per_commit_last_100_mean": round(statistics.mean(files_100), 1) if files_100 else None,
                "files_per_commit_last_100_median": statistics.median(files_100) if files_100 else None,
            }
    if harness:
        total, unreadable = count_harness_scripts(root)
        metrics["harness"] = {"artifacts_script_files": total, "unreadable_directories": unreadable}
    return report


def render(report):
    m = report["metrics"]
    honesty, size, det, coupling = m["module_honesty"], m["size"], m["determinism"], m["coupling"]
    out = ["# AIRK structural metrics (report only)", "", f"Schema `{report['schema']}`.", ""]
    out.append("| Metric | Value |")
    out.append("|---|---|")
    rows = [
        ("`#[path]` mounts, non-platform / platform", f"{honesty['path_mounts_non_platform']} / {honesty['path_mounts_platform']}"),
        ("`#[path]` mounts by crate", ", ".join(f"{k} {v}" for k, v in sorted(honesty["path_mounts_by_crate"].items()))),
        ("`include!` of `.rs` source", honesty["include_rs"]),
        ("Orphan `.rs` files (lines)", f"{honesty['orphan_files']} ({honesty['orphan_lines']})"),
        ("Crates over 20k non-test-file lines", ", ".join(size["crates_over_20k_non_test_lines"]) or "none"),
        ("Rust files over 2,000 lines (non-test)", size["files_over_2000_lines"]),
        ("Functions over 300 / 150 lines (non-test files)", f"{size['functions_over_300_lines']} / {size['functions_over_150_lines']}"),
        ("`pub mod` declarations", size["pub_mod_declarations"]),
        ("Rust lines over 200 characters", size["rust_lines_over_200_chars"]),
        ("Time reads, production / tests", f"{det['time_reads_production']} / {det['time_reads_tests']}"),
        ("Sleeps, production / tests", f"{det['sleeps_production']} / {det['sleeps_tests']}"),
        ("Mutable statics, production / tests", f"{det['mutable_statics_production']} / {det['mutable_statics_tests']}"),
        ("`thread_local!`, production / tests", f"{det['thread_local_production']} / {det['thread_local_tests']}"),
        ("Extension `Date.now()` (production) / timers in tests", f"{det['extension_date_now_production']} / {det['extension_timeouts_tests']}"),
        ("CLI `ServiceRegistry::open*` calls (files)", f"{coupling['cli_service_registry_open_calls']} ({coupling['cli_service_registry_open_files']})"),
        ("`#[tool]` read tools / non-test files quoting 8+ of their names", f"{m['catalogs']['read_tools']} / {len(m['catalogs']['files_with_8_or_more_read_tool_names'])}"),
    ]
    if "context_size" in m:
        context = m["context_size"]
        rows.append((
            f"Context size, estimated tokens over {', '.join(CONTEXT_FILES)} ({context['method']}; target about {context['target_tokens']:,})",
            f"{context['estimated_tokens']:,} ({context['total_bytes']:,} bytes)" + (f"; missing {', '.join(context['missing'])}" if context["missing"] else ""),
        ))
    if "history" in m:
        h = m["history"]
        rows.append(("Commits spanning 3+ / 5+ components (last 200)", f"{h['share_3_plus_components']:.1%} / {h['share_5_plus_components']:.1%}"))
        rows.append(("Files per commit, mean / median (last 100)", f"{h['files_per_commit_last_100_mean']} / {h['files_per_commit_last_100_median']}"))
    if "harness" in m:
        rows.append(("Script files in `artifacts/`, excluding out/extension/dist/runtime/worktree copies (unreadable dirs)", f"{m['harness']['artifacts_script_files']} ({m['harness']['unreadable_directories']})"))
    out.extend(f"| {name} | {value} |" for name, value in rows)
    out += ["", "## Library module graphs", "", "| Crate | Top-level modules | Edges | Cyclic components | Modules in cycles | Largest cycle | Highest fan-in (modules/files) |", "|---|---|---|---|---|---|---|"]
    for name, graph in m["module_graph"].items():
        fan = ", ".join(f"`{item['module']}` {item['modules']}/{item['files']}" for item in graph["fan_in_top"][:4])
        out.append(f"| {name} | {graph['top_level_modules']} | {graph['edges']} | {graph['cyclic_components']} | {graph['modules_in_cycles']} | {graph['largest_cycle']} | {fan} |")
    if honesty["orphans"]:
        out += ["", "## Orphan files", ""] + [f"- `{path}`" for path in honesty["orphans"]]
    out += ["", "Report only; these figures inform review and are never pass/fail gates."]
    return "\n".join(out) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--json", action="store_true", help="print the full JSON record")
    parser.add_argument("--output", type=Path, help="also write the JSON record to this file")
    parser.add_argument("--no-history", action="store_true", help="skip git history metrics")
    parser.add_argument("--no-harness", action="store_true", help="skip the artifacts/ census")
    args = parser.parse_args(argv)
    report = collect(args.root.resolve(), history=not args.no_history, harness=not args.no_harness)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    sys.stdout.write(json.dumps(report, indent=2, sort_keys=True) + "\n" if args.json else render(report))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
