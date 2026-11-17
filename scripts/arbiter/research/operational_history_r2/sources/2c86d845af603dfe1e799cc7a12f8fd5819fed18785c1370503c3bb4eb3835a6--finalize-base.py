# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json
p=Path('artifacts/optimizations/f09-draft/actual-host-qualification')
f=p/'BASE.json';record=json.loads(f.read_text(encoding='utf-8'));record['packages/vscode/src/test/f09NativeQualification.test.ts']={'absent':True};f.write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
f=p/'prepare-overlay.py';s=f.read_text(encoding='utf-8');s=s.replace("for name,identity in record.items():\n if hashlib", "for name,identity in record.items():\n if identity.get('absent'):\n  if (repo/name).exists():raise SystemExit('New test path now exists; rebase/review: '+name)\n  continue\n if hashlib")
f.write_text(s,encoding='utf-8',newline='\n')
