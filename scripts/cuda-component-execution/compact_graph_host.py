# SPDX-License-Identifier: Apache-2.0
"""Apply the reviewed 32 GiB host bounds to the sealed Compact graph runner."""

import ast
from collections import Counter
import hashlib
import types


PRODUCER_RUNNER_SHA256 = "fe47b26e5902372ef27ad7067bc3ba256699dbadbd9abefd04941bd138e0c68c"
HOST_BYTES = 32 << 30

# The original runner is a source pin in the already built cu15 producer.
# Rewrite only the listed host comparisons and Windows Job setup arguments in
# the private graph copy. The original cold and device values have other uses.
HOST_SITES = {
    "input_facts": {24: 1},
    "validate_session_diagnostic": {16: 3, 24: 1},
    "configure_job": {16: 2, 24: 2},
    "run_loaded": {16: 1, 24: 1},
}


def load_graph_runner(path, raw):
    if hashlib.sha256(raw).hexdigest() != PRODUCER_RUNNER_SHA256:
        raise ValueError("32 GiB Compact graph policy requires the sealed producer runner")
    tree = ast.parse(raw, filename=str(path))
    functions = {node.name: node for node in tree.body if isinstance(node, ast.FunctionDef)}
    for name, expected in HOST_SITES.items():
        changed = Counter()
        for node in ast.walk(functions[name]):
            if (isinstance(node, ast.BinOp) and isinstance(node.op, ast.LShift)
                    and isinstance(node.left, ast.Constant) and node.left.value in expected
                    and isinstance(node.right, ast.Constant) and node.right.value == 30):
                changed[node.left.value] += 1
                node.left.value = 32
        if changed != expected:
            raise ValueError(f"sealed Compact graph host limit sites differ: {name}")
    module = types.ModuleType("fresh_compact_32g_graph_runner")
    module.__file__ = str(path)
    exec(compile(tree, str(path), "exec", dont_inherit=True), module.__dict__)
    module.LIMITS = dict(module.LIMITS, process_bytes=HOST_BYTES, job_bytes=HOST_BYTES)
    module.BOUNDS = dict(module.BOUNDS, host_limit_bytes=HOST_BYTES)
    return module
