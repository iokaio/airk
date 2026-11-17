# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source')
f=p/'crates/airk-cli/src/runtime_install.rs';s=f.read_text(encoding='utf-8');start=s.index('    SourceScope {');end=s.index('    },',start)
block=s[start:end].replace('root: PathBuf','source_root: PathBuf').replace('collection: Option<String>','index_collection: Option<String>');s=s[:start]+block+s[end:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'crates/airk-cli/src/runtime_install_commands.rs';s=f.read_text(encoding='utf-8');start=s.index('        RuntimeCommand::SourceScope');end=s.index('        RuntimeCommand::',start+20)
block=s[start:end].replace('            root,','            source_root,').replace('            collection,','            index_collection,').replace('RootScope::new(root, collection.clone())','RootScope::new(source_root, index_collection.clone())');s=s[:start]+block+s[end:];f.write_text(s,encoding='utf-8',newline='\n')
for name in ['packages/vscode/src/serviceBootstrap.ts','packages/vscode/src/test/serviceWorkspace.test.ts','packages/vscode/src/test/serviceExtension.test.ts']:
 f=p/name;s=f.read_text(encoding='utf-8').replace("'--root'","'--source-root'").replace("'--collection'","'--index-collection'");f.write_text(s,encoding='utf-8',newline='\n')
f=p/'packages/vscode/src/serviceWorkspace.ts';s=f.read_text(encoding='utf-8');lines=s.splitlines(keepends=True);s=''.join(line for line in lines if not(line.startswith('const nativeObservations=') or line.startswith('const bindingIdentity=') or line.strip().startswith('nativeObservations.set(result,')));f.write_text(s,encoding='utf-8',newline='\n')
