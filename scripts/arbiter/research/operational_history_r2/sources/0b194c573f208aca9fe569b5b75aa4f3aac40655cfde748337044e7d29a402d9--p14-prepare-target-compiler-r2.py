# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path

base = Path('D:/code/Github/airk/artifacts/v190')
js = (base / 'p14-installed-editor-target-compile-r1.cjs').read_text()
py = (base / 'p14-installed-editor-compile-proof-r1.py').read_text()
needle = "const ts=require(path.join(pkg,'node_modules/typescript'));"
assert js.count(needle) == 1
js = js.replace(needle, "process.chdir(pkg); // Match the canonical package compiler's type-root context.\n" + needle)
assert py.count("A/'p14-installed-editor-target-compile-r1.cjs'") == 1
py = py.replace("A/'p14-installed-editor-target-compile-r1.cjs'", "A/'p14-installed-editor-target-compile-r2.cjs'")
for name, content in [('p14-installed-editor-target-compile-r2.cjs', js), ('p14-installed-editor-compile-proof-r2.py', py)]:
    with (base / name).open('x', encoding='utf-8') as output:
        output.write(content)
result = {'scope': 'ignored compiler tool only; package cwd for existing type roots',
          'files': {name: hashlib.sha256((base / name).read_bytes()).hexdigest() for name in [
              'p14-installed-editor-target-compile-r2.cjs', 'p14-installed-editor-compile-proof-r2.py']},
          'prior_failure': 'p14-installed-editor-compile-r1.json, preserved; no emission',
          'public_source_change': False, 'execution': 'pending'}
with (base / 'p14-target-compiler-r2-prepared.json').open('x') as output:
    json.dump(result, output, indent=2)
print(json.dumps(result))
