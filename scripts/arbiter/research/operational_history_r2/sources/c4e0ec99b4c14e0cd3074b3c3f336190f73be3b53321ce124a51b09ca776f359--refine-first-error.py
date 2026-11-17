# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/packages/vscode/src/serviceConsoleController.ts')
s=p.read_text(encoding='utf-8').replace('        try{await this.events();}catch(error){','        let eventFailure:string|undefined;\n        try{await this.events();}catch(error){',1).replace('            this.state.message=code(error);\n            if(!(error instanceof ServiceError)', '            eventFailure=code(error);this.state.message=eventFailure;\n            if(!(error instanceof ServiceError)',1)
s=s.replace('        try{await this.page(false);}catch(error){this.state.message??=code(error);}', "        try{await this.page(false);}catch(error){this.state.message=eventFailure??code(error);if(eventFailure)void this.host.diagnosticFailure?.('console_page_after_event_reset',error,this.bootstrap).catch(()=>{});}")
p.write_text(s,encoding='utf-8',newline='\n')
