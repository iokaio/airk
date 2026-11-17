# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/actual-host-qualification/source/packages/vscode/src/test/f09NativeQualification.test.ts')
s=p.read_text(encoding='utf-8').replace("import {probeAssistance} from '../native';", "import {probeAssistance,runText} from '../native';")
s=s.replace("            await assert.rejects(probeAssistance(entry,spec.root,version));", "            await assert.rejects(runText(fixture.binary,['explorer-assistance','--descriptor',descriptor],fixture.owner,15000),/explorer_assistance_descriptor_invalid/);")
p.write_text(s,encoding='utf-8',newline='\n')
