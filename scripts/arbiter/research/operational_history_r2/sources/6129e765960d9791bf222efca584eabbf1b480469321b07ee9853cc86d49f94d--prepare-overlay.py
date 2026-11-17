# SPDX-License-Identifier: Apache-2.0
"""Prepare isolated public-source overlay only; compilation/execution are separate owner actions."""
from pathlib import Path
import argparse,json,shutil,hashlib
p=Path(__file__).resolve().parent
repo=p.parents[3]
args=argparse.ArgumentParser();args.add_argument('--output',default='compiled-overlay');options=args.parse_args()
output=(p/options.output).resolve()
if output.parent!=p or not output.name.startswith('compiled-overlay'):
 raise SystemExit('Output must be one compiled-overlay* child of this ignored package')
if output.exists():raise SystemExit('Refusing to overwrite an existing overlay')
vscode=repo/'packages/vscode'
bundle=vscode/'out/explorerAssistance.js'
if not bundle.is_file():raise SystemExit('Owner-built compiled Assistance bundle required; do not synthesize/relabel it')
record=json.loads((p/'BASE.json').read_text(encoding='utf-8'))
for name,identity in record.items():
 if identity.get('absent'):
  if (repo/name).exists():raise SystemExit('New test path now exists; rebase/review: '+name)
  continue
 if hashlib.sha256((repo/name).read_bytes()).hexdigest()!=identity['sha256']:
  raise SystemExit('Recorded source base changed; rebase/review before preparing overlay: '+name)
shutil.copytree(vscode/'src',output/'src')
shutil.copy2(vscode/'package.json',output/'package.json')
(output/'out').mkdir()
shutil.copy2(bundle,output/'out/explorerAssistance.js')
for source in (p/'source/packages/vscode/src').rglob('*'):
 if source.is_file():
  target=output/'src'/source.relative_to(p/'source/packages/vscode/src');target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)
config={'extends':str(vscode/'tsconfig.json'),'compilerOptions':{'rootDir':'./src','outDir':'./out','baseUrl':str(vscode),'paths':{'*':['node_modules/*']},'typeRoots':[str(vscode/'node_modules/@types')]},'include':['src/**/*.ts'],'exclude':['src/explorerWebview/**','src/explorerAssistance/**','src/workbenchWebview/**']}
(output/'tsconfig.json').write_text(json.dumps(config,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'overlay':str(output),'tsconfig':str(output/'tsconfig.json'),'typescript':str(vscode/'node_modules/typescript/bin/tsc'),'node_path':str(vscode/'node_modules'),'test':str(output/'out/test/f09NativeQualification.test.js'),'assistance_sha256':hashlib.sha256(bundle.read_bytes()).hexdigest()},indent=2))
