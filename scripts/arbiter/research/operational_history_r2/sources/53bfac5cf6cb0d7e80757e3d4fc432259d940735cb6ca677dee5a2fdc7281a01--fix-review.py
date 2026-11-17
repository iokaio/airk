# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/exact-scope/source/packages/vscode/src/serviceBootstrap.ts')
s=p.read_text(encoding='utf-8').replace('identifier(raw.namespace,32)','identifier(raw.namespace,16)')
s=s.replace("function legacyArguments(target:LegacyStartupTarget):string[]{return ['--source-root'", "function legacyArguments(target:LegacyStartupTarget):string[]{return ['--root'")
s=s.replace("'--data-dir',target.data_dir,'--index-collection',target.collection", "'--data-dir',target.data_dir,'--collection',target.collection")
s=s.replace("this.invoke(bundle.main,['--source-root',root,'--data-dir',dataRoot,'--index-collection',collection,'service','prepare-registration'])", "this.invoke(bundle.main,['--root',root,'--data-dir',dataRoot,'--collection',collection,'service','prepare-registration'])")
p.write_text(s,encoding='utf-8',newline='\n')
