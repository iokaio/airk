# SPDX-License-Identifier: Apache-2.0
"""Exact observed output role/owner linkage; explicitly not exact regeneration."""
import argparse
import collections
import hashlib
import importlib.util
import json
from pathlib import Path
import posixpath
import re
import time

spec=importlib.util.spec_from_file_location('js_analysis',Path(__file__).with_name('analyze.py'))
a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
ROOT=a.ROOT
LABEL=re.compile(r'^\s*//\s+([^\r\n]+\.(?:ts|tsx|js|mjs|cjs))\s*$',re.M)
CONTROLS=('packages/vscode/package.json','packages/vscode/package-lock.json',
 'packages/vscode/tsconfig.json','packages/vscode/scripts/build-explorer.cjs',
 'packages/vscode/scripts/build-bootstrap.cjs','packages/admin/package.json',
 'packages/admin/package-lock.json','packages/admin/build.mjs',
 'packages/vscode/src/test/extensionUpdateLoader.test.ts',
 'scripts/arbiter/research/javascript_history_r1/source-map.json',
 'scripts/arbiter/research/javascript_history_r1/source_policy/source-map.json')


def source_label(label):
    if label.startswith('node_modules/'):return None
    if label.startswith(('src/','../admin/','../ui/')):
        label=posixpath.normpath('packages/vscode/'+label)
    if not label.startswith(('packages/vscode/src/','packages/admin/src/','packages/ui/src/')):
        raise ValueError('unrecognized first-party module label: '+label)
    if '..' in label.split('/'):raise ValueError('source label traversal')
    return label


def vendor_key(label):
    at=label.find('node_modules/')
    if at<0:return None
    tail=label[at+len('node_modules/'):].split('/')
    name='/'.join(tail[:2]) if tail[0].startswith('@') else tail[0]
    return 'node_modules/'+name


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--analysis',type=Path,required=True);ap.add_argument('--sha256',required=True)
    args=ap.parse_args();start=time.monotonic();deadline=start+120
    snapshots={}
    def observe(path):
        path=Path(path)
        if not path.is_absolute():path=a.repository_path(str(path))
        raw=a.read(path,deadline);snapshots[str(path)]={'sha256':a.sha(raw),'bytes':len(raw)}
        return raw,{'path':path.relative_to(ROOT).as_posix(),'bytes':len(raw),'sha256':a.sha(raw)}
    raw,analysis_pin=observe(args.analysis)
    if a.sha(raw)!=args.sha256:raise ValueError('analysis identity differs')
    report=json.loads(raw)
    if report['schema']!='airk.private.javascript-provenance-analysis/1':raise ValueError('analysis schema')
    controls={p:observe(p) for p in CONTROLS}
    for p in [Path(__file__),Path(a.__file__)]:observe(p)
    captured=set()
    for path in CONTROLS[-2:]:captured.update(r['source'] for r in json.loads(controls[path][0])['files'])
    refs=a.command(['git','show-ref'],deadline)
    if a.command(['git','rev-parse','--show-object-format'],deadline).strip()!=b'sha1':raise ValueError('Git format')
    tracked=set(a.command(['git','ls-files','--','packages/vscode','packages/admin','packages/ui'],deadline).decode().splitlines())
    history=collections.defaultdict(list)
    for line in a.command(['git','rev-list','--objects','--all','--','packages/vscode','packages/admin','packages/ui'],deadline).decode().splitlines():
        oid,_,path=line.partition(' ')
        if path.endswith(('.ts','.tsx','.js','.mjs','.cjs')):history[path].append(oid)
    owners={};vendors={};gaps=[];groups=[]
    def owner(path):
        if path in owners:return path
        file=ROOT/path
        if file.is_file() and path in tracked:
            _,pin=observe(path);owners[path]={'kind':'current_owner','pin':pin,'original_output_source_version_proved':False}
        elif path in history:
            bodies=[]
            for oid in history[path]:
                data=a.command(['git','cat-file','blob',oid],deadline)
                if hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()!=oid:raise ValueError('Git blob drift')
                bodies.append({'git_blob_oid':oid,'sha256':a.sha(data),'bytes':len(data)})
            owners[path]={'kind':'reachable_historical_owner','path_label':path,'objects':bodies,'original_output_source_version_proved':False}
        else:
            owners[path]={'kind':'missing_owner'};gaps.append(path)
        return path
    lock=json.loads(controls['packages/vscode/package-lock.json'][0])['packages']
    fixture_source=controls['packages/vscode/src/test/extensionUpdateLoader.test.ts'][0].decode('utf8')
    fixture=re.search(r"'out/explorerWebview.js': '([^'\\]*)'",fixture_source)
    if not fixture:raise ValueError('exact maintained fixture literal missing')
    for group in report['groups']:
        if group['status']!='unresolved':continue
        paths=[p for p in group['paths'] if p not in captured]
        targets=[p for p in paths if '/out/' in p or '/admin-assets/' in p or p.endswith(('/preview.js','/review-indexing.js','/transpiled-candidate/explorerController.js','/transpiled-candidate/explorerShell.test.js','/transpiled-current/explorerShell.test.js','/c8-attachment-red-source-r1/trust.test.js'))]
        if not targets:continue
        for path in targets:
            data,_=observe(path);a.exact(data,group['sha256'],group['bytes'])
        text=data.decode('utf8');labels=sorted(set(LABEL.findall(text)))
        row={'sha256':group['sha256'],'bytes':group['bytes'],'paths':targets,'exact_output_reproduced':False,'source_version_established':False,'owner_links':[],'vendor_links':[]}
        if data==fixture.group(1).encode():
            row.update(role='exact_maintained_fixture_literal',exact_output_reproduced=True,source_version_established=True,owner_links=['packages/vscode/src/test/extensionUpdateLoader.test.ts'])
            owner('packages/vscode/src/test/extensionUpdateLoader.test.ts')
        elif labels:
            row['role']='source_labelled_bundle';row['observed_module_labels']=labels
            for label in labels:
                vendor=vendor_key(label)
                if vendor:
                    entry=lock.get(vendor)
                    if entry is None:gaps.append('unresolved vendor lock entry: '+vendor)
                    vendors[vendor]={'lock_entry':entry,'lock':controls['packages/vscode/package-lock.json'][1],'installed_payload_match_proved':False}
                    row['vendor_links'].append(vendor)
                else:row['owner_links'].append(owner(source_label(label)))
            row['builder_links']=['packages/vscode/scripts/build-explorer.cjs','packages/vscode/scripts/build-bootstrap.cjs']
            row['license_notice_observed']={'mit_grant':'Permission is hereby granted' in text,'preact':'Preact' in text or 'preact/' in text,'jsonc_parser':'jsonc-parser' in text}
        elif '/admin-assets/' in targets[0]:
            name=Path(targets[0]).name
            if name!='app.'+group['sha256'][:8]+'.js' or not text.startswith('/* SPDX-License-Identifier: Apache-2.0 */'):
                raise ValueError('Admin content-addressed bundle structure differs')
            row.update(role='content_addressed_admin_bundle',owner_links=[owner('packages/admin/src/main.tsx')],builder_links=['packages/admin/build.mjs'],lock_link=controls['packages/admin/package-lock.json'][1])
        elif 'Object.defineProperty(exports, "__esModule", { value: true });' in text:
            relative=targets[0].rsplit('/out/',1)[-1] if '/out/' in targets[0] else Path(targets[0]).name
            base='packages/vscode/src/'+relative[:-3]
            candidates=[base+suffix for suffix in ('.ts','.tsx','.js')]
            if Path(relative).name.endswith('.test.js'):candidates.append('packages/vscode/src/test/'+Path(relative).name[:-3]+'.ts')
            selected=next((p for p in candidates if (ROOT/p).is_file() or p in history),candidates[0])
            row.update(role='typescript_commonjs_output_structure',owner_links=[owner(selected)],builder_links=['packages/vscode/package.json','packages/vscode/tsconfig.json'],compiler_lock_entry=lock.get('node_modules/typescript'))
        else:
            row.update(role='unresolved_generation_role');gaps.append(targets[0])
        groups.append(row)
    if a.command(['git','show-ref'],deadline)!=refs:raise ValueError('Git refs drift')
    for path,identity in snapshots.items():a.exact(a.read(Path(path),deadline),identity['sha256'],identity['bytes'])
    a.check(deadline)
    result={'schema':'airk.private.javascript-role-linkage/1','analysis':analysis_pin,'owner':a.pin(Path(__file__),deadline),'qualification':False,'historical_javascript_executed':False,'bundler_or_compiler_executed':False,'scope':'Declared source owners and generation roles; no original source-version or exact bundle regeneration claim','git_refs':refs.decode().splitlines(),'controls':{p:v[1] for p,v in controls.items()},'source_owners':owners,'vendor_lock_links':vendors,'unresolved_owners_or_roles':sorted(set(gaps)),'groups':groups,'paths':sum(len(g['paths']) for g in groups),'hashes':len(groups),'elapsed_seconds':time.monotonic()-start}
    print(json.dumps(result,indent=2))


if __name__=='__main__':main()
