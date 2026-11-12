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


def measure(source):
    names = modules(source)
    lines, edges = {}, defaultdict(lambda: defaultdict(int))
    for name in names:
        total = 0
        for path in files_of(source, name):
            if is_test(source, path):
                continue
            text = path.read_text(encoding='utf-8')
            cut = INLINE_TESTS.search(text)
            if cut:
                text = text[:cut.start()]
            total += text.count('\n')
            for target in REFERENCE.findall(text):
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


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--crate', default='airk-cli')
    parser.add_argument('--json', type=Path)
    args = parser.parse_args(argv)
    source = ROOT / 'crates' / args.crate / 'src'
    lines, edges = measure(source)
    found = cycles(lines, edges)
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
