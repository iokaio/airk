# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f17-draft/display-updates/source/packages/vscode/src/test/workbenchClientLifetime.test.ts')
s=p.read_text(encoding='utf-8').replace('const statusMessages=f.messages as WorkbenchUpdate[];','const statusMessages=f.messages.slice() as WorkbenchUpdate[];').replace('const saveMessages=f.messages as WorkbenchUpdate[];','const saveMessages=f.messages.slice() as WorkbenchUpdate[];')
p.write_text(s,encoding='utf-8',newline='\n')
