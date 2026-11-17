# SPDX-License-Identifier: Apache-2.0
"""Parse private amendments only; never mount sources, compile or run native tests."""
from pathlib import Path
import ast
import hashlib
import json
import shutil
import subprocess

HERE = Path(__file__).resolve().parent

def main():
    parsed = []
    for path in sorted(HERE.glob("*.py")):
        ast.parse(path.read_bytes(), filename=str(path))
        parsed.append(path.name)
    rustfmt = shutil.which("rustfmt.exe") or str(Path.home() / ".cargo/bin/rustfmt.exe")
    if not Path(rustfmt).is_file():
        raise ValueError("pinned installed rustfmt required for private parse")
    rust = []
    for name in ["native_parent_bag.rs", "development.rs"]:
        source = HERE / (name + ".proposed.txt")
        output = HERE / (name + ".formatted.proposed.txt")
        actual = subprocess.run([rustfmt, "--edition", "2021", "--config", "skip_children=true",
                                 "--emit", "stdout"], input=source.read_bytes(), check=True,
                                capture_output=True)
        output.write_bytes(actual.stdout)
        rust.append({"source":source.name, "parse_passed":True, "formatted":output.name,
                     "formatted_sha256":hashlib.sha256(actual.stdout).hexdigest()})
    print(json.dumps({"Python_AST_passed":parsed,"Rust_parse_only":rust,
                      "public_sources_written":False,"Cargo_or_NN_executed":False}))

if __name__ == "__main__":
    main()
