# SPDX-License-Identifier: Apache-2.0
"""Top-level module graph of one crate: production lines per module, `crate::<module>`
references between modules, and the cycles they form (docs/simplify/airk-cli-split.md).

    python scripts/dev/module_graph.py [--crate airk-cli] [--json OUT]

Test files (`tests.rs`, `*_tests.rs`, files under `tests/` folders) and a trailing
inline `#[cfg(test)] mod … {` block are excluded. A reference counts once per
occurrence of `crate::<module>` in the source text.
"""
import argparse
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MOD = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-z0-9_]+)\s*;', re.M)
REFERENCE = re.compile(r'crate::([a-z0-9_]+)')
INLINE_TESTS = re.compile(r'\n#\[cfg\(test\)\]\s*\nmod\s+\w+\s*\{')


def grouped(text):
    """First-level names of `crate::{a, b::c, …}` grouped imports."""
    names = []
    for match in re.finditer(r'crate::\{', text):
        index, depth, start = match.end(), 1, match.end()
        while depth and index < len(text):
            character = text[index]
            if character == '{':
                depth += 1
            elif character == '}':
                depth -= 1
                if depth == 0:
                    names.append(text[start:index])
            elif character == ',' and depth == 1:
                names.append(text[start:index])
                start = index + 1
            index += 1
    found = []
    for item in names:
        item = re.sub(r'//[^\n]*', '', item).strip()
        head = re.match(r'([a-z0-9_]+)', item)
        if head:
            found.append(head.group(1))
    return found


def modules(source):
    return sorted(set(MOD.findall((source / 'lib.rs').read_text(encoding='utf-8'))))


def files_of(source, name):
    found = [source / f'{name}.rs'] if (source / f'{name}.rs').exists() else []
    if (source / name).is_dir():
        found += sorted((source / name).rglob('*.rs'))
    return found


def is_test(source, path):
    relative = path.relative_to(source)
    return (path.name == 'tests.rs' or path.name.endswith('_tests.rs')
            or 'tests' in relative.parts[:-1])


def measure(source, include_tests=False):
    names = modules(source)
    lines, edges = {}, defaultdict(lambda: defaultdict(int))
    for name in names:
        total = 0
        for path in files_of(source, name):
            test_file = is_test(source, path)
            if test_file and not include_tests:
                continue
            text = path.read_text(encoding='utf-8')
            cut = INLINE_TESTS.search(text)
            if not test_file:
                total += (text[:cut.start()] if cut else text).count('\n')
            if cut and not include_tests:
                text = text[:cut.start()]
            for target in REFERENCE.findall(text) + grouped(text):
                if target in names and target != name:
                    edges[name][target] += 1
        lines[name] = total
    return lines, {name: dict(targets) for name, targets in edges.items()}


def cycles(lines, edges):
    """Strongly connected components with more than one module (Tarjan, iterative)."""
    index, low, on, stack, found, counter = {}, {}, set(), [], [], 0
    for start in lines:
        if start in index:
            continue
        work = [(start, iter(edges.get(start, {})))]
        index[start] = low[start] = counter
        counter += 1
        stack.append(start)
        on.add(start)
        while work:
            node, children = work[-1]
            advanced = False
            for child in children:
                if child not in index:
                    index[child] = low[child] = counter
                    counter += 1
                    stack.append(child)
                    on.add(child)
                    work.append((child, iter(edges.get(child, {}))))
                    advanced = True
                    break
                if child in on:
                    low[node] = min(low[node], index[child])
            if advanced:
                continue
            work.pop()
            if work:
                parent = work[-1][0]
                low[parent] = min(low[parent], low[node])
            if low[node] == index[node]:
                component = []
                while True:
                    member = stack.pop()
                    on.discard(member)
                    component.append(member)
                    if member == node:
                        break
                if len(component) > 1:
                    found.append(sorted(component, key=lambda name: -lines[name]))
    return sorted(found, key=lambda component: -sum(lines[name] for name in component))


def back_edges(lines, edges, groups):
    """Module references that point from a group to one later in `order` (or a module
    with no group): each one blocks extracting the earlier group as its own crate."""
    order = groups['order']
    owner = {module: group for group, members in groups['groups'].items() for module in members}
    unassigned = sorted(module for module in lines if module not in owner)
    found = []
    for source_module, targets in edges.items():
        source_group = owner.get(source_module)
        for target_module, count in targets.items():
            target_group = owner.get(target_module)
            if source_group is None or target_group is None:
                continue
            if order.index(target_group) > order.index(source_group):
                found.append((source_group, target_group, source_module, target_module, count))
    sizes = {group: sum(lines.get(module, 0) for module in members)
             for group, members in groups['groups'].items()}
    return sorted(found), unassigned, sizes


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--crate', default='airk-cli')
    parser.add_argument('--json', type=Path)
    parser.add_argument('--include-tests', action='store_true',
                        help='count references in unit tests too (each crate must compile its tests)')
    parser.add_argument('--groups', type=Path,
                        help='a module-group file (docs/simplify/airk-cli-groups.json); reports back edges')
    args = parser.parse_args(argv)
    source = ROOT / 'crates' / args.crate / 'src'
    lines, edges = measure(source, args.include_tests)
    found = cycles(lines, edges)
    if args.groups:
        groups = json.loads(args.groups.read_text(encoding='utf-8'))
        backward, unassigned, sizes = back_edges(lines, edges, groups)
        for group in groups['order']:
            print(f'group {group}: {sizes.get(group, 0)} lines')
        for module in unassigned:
            print(f'unassigned: {module} ({lines[module]} lines)')
        for source_group, target_group, source_module, target_module, count in backward:
            print(f'back edge {source_group} -> {target_group}: {source_module} -> {target_module} ({count})')
        print(f'{len(backward)} back edges, {len(unassigned)} unassigned modules')
        return 1 if backward or unassigned else 0
    if args.json:
        args.json.write_text(json.dumps({'lines': lines, 'edges': edges, 'cycles': found}, indent=1) + '\n',
                             encoding='utf-8')
    print(f'{args.crate}: {len(lines)} modules, {sum(lines.values())} production lines')
    for component in found:
        print(f'cycle: {len(component)} modules, {sum(lines[name] for name in component)} lines: '
              + ' '.join(component))
    return 0


if __name__ == '__main__':
    sys.exit(main())
