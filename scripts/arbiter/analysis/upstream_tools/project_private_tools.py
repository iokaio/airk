# SPDX-License-Identifier: Apache-2.0
"""Prove private tool copies and exact historical test literals; stdout metadata only."""
import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess
from project_archives import REPOSITORY, checked, consumed, measure, strict_json

COMMIT='b3ec4dda1459b87262ee1405ad8d7684dc89e1b4'
GENERATOR='packages/vscode/src/test/native.test.ts'
REACHABLE_REF='refs/heads/feat/airksvc'
REACHABLE_HEAD='66c2a2a4bd5375abfe0ac42ec7c18c22b282c498'
PRODUCER='scripts/arbiter/research/operational_history_r2/sources/29fb5457770f12b297cda9e5f44409af2cd3211fe54dc43a9b69797f1ef7a558--codex-native-acceptance.py'
ROUTING='scripts/arbiter/research/operational_history_r2/routing-notes.json'

def pin(path):return dict(path=path.relative_to(REPOSITORY).as_posix(),**measure(path))
def git(*args):return subprocess.check_output(['git',*args],cwd=REPOSITORY)

def project():
    routing_path=checked(REPOSITORY,ROUTING);raw,routing_pin=consumed(routing_path);routing=strict_json(raw)
    producer=pin(checked(REPOSITORY,PRODUCER));code=pin(Path(__file__).resolve())
    common=[dict(path=ROUTING,**routing_pin),producer,code]
    result={'schema':'airk.source-disposition/1','files':[],'errors':[],'not_selected':0}
    if git('rev-parse',REACHABLE_REF).decode().strip()!=REACHABLE_HEAD:raise ValueError('retained historical ref moved')
    git('merge-base','--is-ancestor',COMMIT,REACHABLE_HEAD)
    historical=git('cat-file','blob',COMMIT+':'+GENERATOR)
    generator={'commit':COMMIT,'reachable_ref':REACHABLE_REF,'reachable_head':REACHABLE_HEAD,'path':GENERATOR,'blob_oid':git('rev-parse',COMMIT+':'+GENERATOR).decode().strip(),'sha256':hashlib.sha256(historical).hexdigest(),'bytes':len(historical)}
    text=historical.decode('utf-8')
    for row in routing['rows']:
        source=row['source'];tool='/skills/.system/' in source;synthetic='/airk-native-Ya1UQi/project-' in source
        if not (tool or synthetic):result['not_selected']+=1;continue
        try:
            path=checked(REPOSITORY,source);data,actual=consumed(path)
            if actual!={'sha256':row['source_sha256'],'bytes':row['source_bytes']}:raise ValueError('source changed since routing')
            item={'source':source,'source_sha256':actual['sha256'],'source_bytes':actual['bytes'],'evidence':common.copy()}
            if tool:
                home,relative=source.split('/skills/.system/',1);suffix='skills/.system/'+relative
                installed=Path('C:/Users/tdotn/.codex/skills/.system')/relative
                installed_data,installed_pin=consumed(installed)
                if installed_data!=data:raise ValueError('installed human tool input differs')
                bindings=[]
                for log in (REPOSITORY/home).parent.glob('*.stderr'):
                    log_raw,log_pin=consumed(log)
                    normalized=log_raw.decode('utf-8',errors='replace').replace('\\\\','/').replace('\\','/')
                    if home in normalized:bindings.append(dict(path=log.relative_to(REPOSITORY).as_posix(),**log_pin))
                if not bindings:raise ValueError('private home lacks retained client binding')
                item.update(disposition='observed_tool_input',reason='Exact bytes match installed human Codex system skill and private client-home bindings. This proves an observed tool input, not a fresh upstream release or successful client qualification.',tool_input={'installed':dict(absolute_path=installed.as_posix(),**installed_pin),'producer':producer,'private_home':home,'relative_path':suffix,'bindings':bindings})
                item['evidence'].extend(bindings)
            else:
                filename=path.name
                pattern=r"await fs\.writeFile\(path\.join\([ab]\.root,'"+re.escape(filename)+r"'\),(?P<literal>'(?:[^'\\]|\\.)*')\);"
                matches=list(re.finditer(pattern,text))
                if len(matches)!=1:raise ValueError('historical write anchor ambiguous or absent')
                match=matches[0];literal=match['literal'];reproduced=ast.literal_eval(literal).encode('utf-8')
                if reproduced!=data:raise ValueError('historical literal does not reproduce whole source')
                failure=path.parent.parent/'test-failure-diagnostics/failure-xtVgje/failure.json'
                failure_raw,failure_pin=consumed(failure)
                if Path(strict_json(failure_raw)['fixture_owner']).resolve()!=path.parent.parent.resolve():raise ValueError('failure receipt fixture identity differs')
                item['evidence'].append(dict(path=failure.relative_to(REPOSITORY).as_posix(),**failure_pin))
                item.update(disposition='generated_literal',reason='Entire indexed test input exactly reproduced by a unique historical committed writeFile literal; retained native failure receipt binds this private fixture owner. No passing qualification claimed.',generated={'git_generator':generator,'literal_lexeme':literal,'source_anchor':match[0],'reproduced_sha256':actual['sha256']})
            result['files'].append(item)
        except (OSError,ValueError,KeyError,SyntaxError) as error:result['errors'].append({'source':source,'error':str(error)})
    return result

if __name__=='__main__':
    result=project();print(json.dumps(result,indent=2))
    if result['errors']:raise SystemExit(1)
