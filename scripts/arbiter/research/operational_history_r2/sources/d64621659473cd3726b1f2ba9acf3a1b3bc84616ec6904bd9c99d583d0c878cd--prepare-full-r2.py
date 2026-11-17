# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
base = Path(__file__).resolve().parent
target = base / 'run-full-native-r2.py'
assert not target.exists()
text = (base / 'run-full-native.py').read_text(encoding='utf-8')
text = text.replace('full-native-inventory-r1.json', 'full-native-inventory-r2.json').replace('full-native-r1.', 'full-native-r2.')
target.write_text(text, encoding='utf-8')
