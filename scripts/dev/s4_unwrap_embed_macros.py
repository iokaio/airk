# SPDX-License-Identifier: Apache-2.0
# S4 embed-pool step: committed so reviewers can re-run it. Apply after the
# git mv moves of that step, on its parent commit: python scripts/dev/s4_unwrap_embed_macros.py .
# Unwrap the provider_context! macro scaffolding after the pool sources have
# been moved with git mv. Every remaining instantiation used cfg(all()), so the
# metavariable attributes are removed and the macro bodies are emitted inline.
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
pool = root / "crates/airk-cli/src/inference_engine/pool"
OWNER = re.compile(r"^\s*#\[\$(owner_cfg|charge_test_cfg)\]\s*$")


def unwrap(text, name, header, invocation):
    lines = text.split("\n")
    start = lines.index(f"macro_rules! {name} {{")
    assert lines[start + 1].strip() == header, lines[start + 1]
    # The body closes with "    };" then "}".
    end = start + 2
    depth = 0
    while True:
        line = lines[end]
        if line == "    };" and lines[end + 1] == "}":
            break
        end += 1
    body = []
    for line in lines[start + 2:end]:
        if OWNER.match(line):
            continue
        assert line == "" or line.startswith("        "), line
        body.append(line[8:])
    after = lines[end + 2:]
    if invocation is not None:
        assert after[0] == invocation, after[0]
        after = after[1:]
    return "\n".join(lines[:start] + body + after)


accounting = pool / "accounting.rs"
text = accounting.read_text(encoding="utf-8")
text = unwrap(text, "define_provider_accounting", "($owner_cfg:meta, $charge_test_cfg:meta) => {", None)
text = text.replace(
    "// The wrapper selects owner-only fields without changing shared charge logic.\n", "")
accounting.write_bytes(text.encode("utf-8"))

core = pool / "mod.rs"
text = core.read_text(encoding="utf-8")
text = unwrap(text, "define_provider_work", "($owner_cfg:meta) => {", "provider_context!(define_provider_work);")
core.write_bytes(text.encode("utf-8"))
for path in (accounting, core):
    assert "$" not in path.read_text(encoding="utf-8").replace("${", ""), path
print("unwrapped")
