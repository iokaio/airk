# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/packages/vscode/src/serviceConsoleController.ts')
s=p.read_text(encoding='utf-8').replace("            if(error.code==='event_scope_changed')this.clearRegistryObservation(true);", "            if(error.code==='event_scope_changed'){this.observationToken=randomUUID();this.clearRegistryObservation(true);}")
p.write_text(s,encoding='utf-8',newline='\n')
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/packages/vscode/src/test/serviceConsole.test.ts');s=p.read_text(encoding='utf-8')
s=s.replace("        await f.controller.refresh();seen.length=0;broken=true;await f.controller.refresh();", "        await f.controller.refresh();const identity=f.controller.observationIdentity;seen.length=0;broken=true;await f.controller.refresh();\n        assert.notEqual(f.controller.observationIdentity,identity,'scope refusal invalidates old UI completions even if the fresh observation fails');")
p.write_text(s,encoding='utf-8',newline='\n')
