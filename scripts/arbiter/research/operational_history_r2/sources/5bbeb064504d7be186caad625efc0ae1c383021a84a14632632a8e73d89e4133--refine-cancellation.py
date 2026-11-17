# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/packages/vscode/src/test/serviceWorkspace.test.ts')
s=p.read_text(encoding='utf-8').replace('let suspend:Promise<void>|undefined;const requests:string[][]=[];', 'let suspend:Promise<void>|undefined,entered:(()=>void)|undefined;const requests:string[][]=[];')
s=s.replace('        if(suspend)await suspend;', '        entered?.();if(suspend)await suspend;')
s=s.replace('        let release!:()=>void;suspend=new Promise<void>(resolve=>{release=resolve;});\n        const pending=workspace.selectedObservation(binding,source,read);await new Promise<void>(resolve=>setImmediate(resolve));allowed=false;release();', '        let release!:()=>void;suspend=new Promise<void>(resolve=>{release=resolve;});\n        const admitted=new Promise<void>(resolve=>{entered=resolve;});\n        const pending=workspace.selectedObservation(binding,source,read);await admitted;allowed=false;release();')
p.write_text(s,encoding='utf-8',newline='\n')
