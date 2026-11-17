# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib
p=Path('artifacts/optimizations/f09-draft/lazy-discovery');record={}
for name in ['packages/vscode/src/serviceBootstrap.ts','packages/vscode/src/test/serviceBootstrap.test.ts']:
 f=Path(name);record[name]={'sha256':hashlib.sha256(f.read_bytes()).hexdigest()}
 for layer in ['base','source']:
  dst=p/layer/name;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(f.read_bytes())
(p/'BASE.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
f=p/'source/packages/vscode/src/serviceBootstrap.ts';s=f.read_text(encoding='utf-8')
s=s.replace("private discovering?:{execution:Promise<BootstrapObservation>;full:Promise<BootstrapObservation>};", "private discovering?:{execution:Promise<BootstrapObservation>;full?:Promise<BootstrapObservation>;connections:number;fullSettled:boolean};")
start=s.index("    private discovery():");end=s.index('    /** Current verified execution metadata',start)
s=s[:start]+'''    private discovery():NonNullable<RuntimeHost['discovering']>{
        if(this.discovering)return this.discovering;
        const attempt:NonNullable<RuntimeHost['discovering']>={execution:this.discoverOnce(),connections:0,fullSettled:false};
        this.discovering=attempt;return attempt;
    }
    private retireDiscovery(attempt:NonNullable<RuntimeHost['discovering']>):void{
        if(attempt.connections===0&&(!attempt.full||attempt.fullSettled)&&this.discovering===attempt)this.discovering=undefined;
    }
    private fullDiscovery(attempt:NonNullable<RuntimeHost['discovering']>):Promise<BootstrapObservation>{
        if(attempt.full)return attempt.full;
        attempt.full=attempt.execution.then(async value=>{
            const observation=structuredClone(value);
            if(observation.installed){
                try{observation.diagnosis=await this.invoke(observation.bundle.main,['runtime','diagnose','--install-dir',observation.installDir]);}
                catch{this.active();observation.error??='runtime_diagnostics_unavailable';}
            }
            return observation;
        }).finally(()=>{attempt.fullSettled=true;this.retireDiscovery(attempt);});
        // Full is owned only after an explicit caller or mandatory fallback.
        // A connection waiter never cancels an accepted shared full discovery.
        void attempt.full.catch(()=>{});
        return attempt.full;
    }
    discover():Promise<BootstrapObservation>{return this.fullDiscovery(this.discovery()).then(value=>structuredClone(value));}
'''+s[end:]
s=s.replace('        const attempt=this.discovery(),observed=await attempt.execution;this.active();', '        const attempt=this.discovery();attempt.connections++;\n        try{\n        const observed=await attempt.execution;this.active();',1)
s=s.replace('const complete=await attempt.full;', 'const complete=await this.fullDiscovery(attempt);')
needle='        }finally{client.dispose();}\n    }\n    private async discoverOnce()'
assert needle in s;s=s.replace(needle,'        }finally{client.dispose();}\n        }finally{attempt.connections--;this.retireDiscovery(attempt);}\n    }\n    private async discoverOnce()',1)
f.write_text(s,encoding='utf-8',newline='\n')
