# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import re

def change(name, transform):
    path = Path(name)
    value = path.read_text(encoding='utf-8')
    updated = transform(value)
    if value == updated:
        raise RuntimeError(f'No candidate version change in {name}')
    path.write_text(updated, encoding='utf-8', newline='')

change('Cargo.toml', lambda text: text.replace('version = "1.5.9"', 'version = "1.5.10"', 1))
change('Cargo.lock', lambda text: re.sub(r'(\[\[package\]\]\nname = "airk-[^"]+"\nversion = ")1\.5\.9(")', r'\g<1>1.5.10\2', text))
for name in ['packages/vscode/package.json', 'packages/vscode/package-lock.json']:
    change(name, lambda text: text.replace('"version": "1.5.9"', '"version": "1.5.10"'))
for name in ['CONTRIBUTING.md', 'docs/versioning.md']:
    change(name, lambda text: text.replace('1.5.9', '1.5.10'))
change('README.md', lambda text: text.replace('current development package version is **1.5.9**', 'current development package version is **1.5.10**'))
change('packages/vscode/README.md', lambda text: text.replace('Development version: 1.5.9.', 'Development version: 1.5.10.'))
change('docs/README.md', lambda text: text.replace('Current 1.5.9 uses', 'Current 1.5.10 uses').replace('Current development version: **1.5.9**', 'Current development version: **1.5.10**'))
