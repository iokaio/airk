# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path('artifacts/optimizations/f09-draft/actual-host-qualification')
names=['packages/vscode/src/test/nativeServiceFixture.ts','packages/vscode/src/test/serviceTestHost.ts']
record={}
for name in names:
 f=Path(name);record[name]={'sha256':hashlib.sha256(f.read_bytes()).hexdigest()}
 for layer in ['base','source']:
  dst=p/layer/name;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(f.read_bytes())
(p/'BASE.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
f=p/'source/packages/vscode/src/test/nativeServiceFixture.ts';s=f.read_text(encoding='utf-8')
s=s.replace('static async create(binary:string):Promise<NativeServiceFixture>',"static async create(binary:string,mode:'managed'|'owned'='managed'):Promise<NativeServiceFixture>")
start=s.index("            await fixture.run(['runtime','managed'");end=s.index("            const prepared=",start)
s=s[:start]+'''            const approved=createHash('sha256').update(bytes).digest('hex');
            if(mode==='owned'){
                // Explicit qualification only: stage copies the approved payload
                // into this fixture's temp install; no user install/default state.
                await fixture.run(['runtime','stage','--source',source,'--install-dir',fixture.install,
                    '--state-dir',path.join(owner,'s'),'--approved-sha256',approved,'--verification-basis','organization-digest'],180000);
                await fixture.run(['runtime','activate','--install-dir',fixture.install,'--runtime-sha256',approved],180000);
            }else await fixture.run(['runtime','managed','--source',source,'--install-dir',fixture.install,
                '--state-dir',path.join(owner,'s'),'--approved-sha256',approved,
                '--verification-basis','organization-digest','--lifecycle','user-on-demand'],180000);
'''+s[end:]
s=s.replace("assert.equal(record(inspected.managed_policy).lifecycle,'user_on_demand');", "if(mode==='managed')assert.equal(record(inspected.managed_policy).lifecycle,'user_on_demand');else assert.equal(inspected.managed_policy,null);")
s=s.replace("// user-on-demand owner. A failed drain preserves the directory for diagnosis.","// fixture owner. A failed drain preserves the directory for diagnosis.")
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/packages/vscode/src/test/serviceTestHost.ts';s=f.read_text(encoding='utf-8').replace("import { RuntimeHost } from '../serviceBootstrap';","import { RuntimeHost, type RuntimeBinding } from '../serviceBootstrap';")
s=s.replace('codeFile?:boolean}={}', 'codeFile?:boolean;nativeSourceScope?:{binding:RuntimeBinding;source:string;collection:string}}={}')
s=s.replace("const install=path.join(root,'installation'),control=path.join(install,'control'),data=path.join(root,'data');", "const install=options.nativeSourceScope?.binding.installDir??path.join(root,'installation'),control=options.nativeSourceScope?.binding.controlDir??path.join(install,'control'),data=path.join(root,'data');")
s=s.replace("    const binding={installDir:install", "    const binding=options.nativeSourceScope?.binding??{installDir:install",1)
s=s.replace("        const source=path.join(root,`source-${i}`);await fs.mkdir(source);folders.push({name:`Source ${i}`,uri:{scheme:'file',fsPath:await fs.realpath(source)}});", "        const external=i===0?options.nativeSourceScope:undefined;\n        const source=external?.source??path.join(root,`source-${i}`);if(!external)await fs.mkdir(source);\n        folders.push({name:external?.collection??`Source ${i}`,uri:{scheme:'file',fsPath:await fs.realpath(source)}});")
s=s.replace("version:binding.version,verification_basis:'trusted_distribution'", "version:binding.version,verification_basis:binding.verificationBasis")
s=s.replace("    test.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{", "    // Real-source qualification leaves invoke/NativeRunner entirely production.\n    if(!options.nativeSourceScope)test.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{")
f.write_text(s,encoding='utf-8',newline='\n')
