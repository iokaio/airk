# SPDX-License-Identifier: Apache-2.0
"""Read-only plan and identity verification for historical standalone patches."""
from pathlib import Path
import argparse, collections, hashlib, json, re, subprocess, tarfile, zipfile
ROOT=Path(__file__).resolve().parents[5]
DEST=Path(__file__).resolve().parent
E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z'
DELTA=E/'source-broad-delta-r1.json'
def digest(raw): return hashlib.sha256(raw).hexdigest()
def file_pin(path):
    path=Path(path);raw=path.read_bytes()
    return {'path':str(path.relative_to(ROOT)).replace('\\','/'),'sha256':digest(raw),'bytes':len(raw)}
def raw_source(row):
    p=ROOT/row['path']
    if p.is_symlink() or not p.resolve().is_relative_to(ROOT):raise ValueError('aliased source')
    raw=p.read_bytes()
    if len(raw)>16*1024*1024 or len(raw)!=row['bytes'] or digest(raw)!=row['source_sha256']:raise ValueError('source changed: '+row['path'])
    return raw
def archive_members(archive,wanted):
    result={}
    if archive.suffix=='.zip':
        with zipfile.ZipFile(archive) as z:
            for name in z.namelist():
                rel=name.partition('/')[2]
                if rel in wanted:
                    info=z.getinfo(name)
                    if info.file_size>16*1024*1024:raise ValueError('oversize upstream patch')
                    result[rel]=z.read(name)
    else:
        with tarfile.open(archive,'r:*') as t:
            for member in t:
                rel=member.name.partition('/')[2]
                if rel in wanted and member.isfile():
                    if member.size>16*1024*1024:raise ValueError('oversize upstream patch')
                    result[rel]=t.extractfile(member).read()
    if set(result)!=set(wanted):raise ValueError('missing upstream archive members')
    return result
def matched_member(actual,member):
    if actual==member:return 'raw_bytes'
    if actual.replace(b'\r\n',b'\n')==member.replace(b'\r\n',b'\n'):return 'CRLF_to_LF_only'
    raise ValueError('upstream patch content differs')
def build_identity(raw,identity):
    if identity in ('raw_inventory_bytes','raw_bytes'):
        return 'raw',{'source_sha256':digest(raw)}
    if identity!='CRLF_to_LF_only':raise ValueError('unknown upstream representation')
    raw.decode('utf-8',errors='strict')
    if b'\0' in raw:raise ValueError('NUL in text representation')
    normalized=raw.replace(b'\r\n',b'\n')
    oid=hashlib.sha1(b'blob '+str(len(normalized)).encode()+b'\0'+normalized).hexdigest()
    return 'CRLF-to-LF',{'git_blob_oid':oid}
def upstream(rows):
    result={};cache={}
    for row in rows:
        match=re.match(r'(artifacts/advanced-split/ort130-x64(?:-owned-r[1-5])?)/source/(.+)',row['path'])
        if not match:continue
        base,relative=match.groups()
        if base not in cache:
            folder=ROOT/base;inventory=folder/'source-files.json';owner=folder/'owner.json'
            pins={p['path']:p for p in json.loads(inventory.read_bytes())}
            metadata=json.loads(owner.read_bytes())
            if metadata['upstream_commit']!='f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7':raise ValueError('ORT source epoch differs')
            cache[base]=(pins,[file_pin(inventory),file_pin(owner),file_pin(folder/'source/LICENSE')])
        pins,evidence=cache[base]
        if pins[relative]['sha256']!=row['source_sha256'] or pins[relative]['bytes']!=row['bytes']:raise ValueError('ORT inventory patch differs')
        result[row['path']]={'reason':'Unmodified upstream ORT dependency patch authenticated by original full source inventory; not AIRK-authored implementation.','evidence':evidence,'upstream':{'repository':'https://github.com/microsoft/onnxruntime','revision':'f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7','path':relative},'identity':'raw_inventory_bytes'}
    prefix='artifacts/local/one-airk-1.8.14/onnxruntime-source/'
    selected=[r for r in rows if r['path'].startswith(prefix)]
    if selected:
        folder=ROOT/'artifacts/local/one-airk-1.8.14'
        archive=folder/'onnxruntime-1.22.1-source.tar.gz'
        metadata=folder/'upstream-metadata.json'
        if json.loads(metadata.read_bytes())['revision']!='89746dc19a0a1ae59ebf4b16df9acab8f99f3925':raise ValueError('old ORT source epoch differs')
        members=archive_members(archive,{r['path'][len(prefix):] for r in selected})
        evidence=[file_pin(archive),file_pin(metadata),file_pin(folder/'onnxruntime-source/LICENSE')]
        for row in selected:
            relative=row['path'][len(prefix):];member=members[relative];kind=matched_member(raw_source(row),member)
            result[row['path']]={'reason':'Upstream ORT 1.22.1 archive member; only explicitly recorded line-ending conversion may differ. Not authored AIRK source.','evidence':evidence,'upstream':{'repository':'https://github.com/microsoft/onnxruntime','revision':'89746dc19a0a1ae59ebf4b16df9acab8f99f3925','path':relative},'identity':kind,'archive_member_sha256':digest(member)}
    base=ROOT/'artifacts/advanced-split/ort130-x64'
    deps=base/'source/cmake/deps.txt'
    urls={line.split(';')[0]:line.split(';')[1:] for line in deps.read_text().splitlines() if line.startswith(('flatbuffers;','protobuf;','re2;'))}
    for dep in ('flatbuffers','protobuf','re2'):
        selected=[r for r in rows if re.match(r'artifacts/advanced-split/ort130-x64(?:-owned-r[1-5])?/b/',r['path']) and '/'+dep+'-src/' in r['path']]
        if not selected:continue
        url,sha1=urls[dep];archive=base/'source/mirror'/url.removeprefix('https://')
        with archive.open('rb') as stream:
            if hashlib.file_digest(stream,'sha1').hexdigest()!=sha1:raise ValueError('upstream dependency archive differs')
        members=archive_members(archive,{r['path'].split('/'+dep+'-src/')[1] for r in selected})
        license_path=base/('b/Release/_deps/'+dep+'-src/LICENSE')
        if not license_path.is_file():license_path=license_path.with_name('LICENSE.txt')
        evidence=[file_pin(deps),file_pin(archive),file_pin(license_path)]
        revisions={'flatbuffers':'v23.5.26','protobuf':'v33.6','re2':'2024-07-02'}
        repositories={'flatbuffers':'https://github.com/google/flatbuffers','protobuf':'https://github.com/protocolbuffers/protobuf','re2':'https://github.com/google/re2'}
        for row in selected:
            relative=row['path'].split('/'+dep+'-src/')[1];member=members[relative];kind=matched_member(raw_source(row),member)
            result[row['path']]={'reason':'Unmodified dependency-source patch; original ORT deps.txt archive SHA1 verified and archive member compared; not AIRK-authored implementation.','evidence':evidence,'upstream':{'repository':repositories[dep],'revision':revisions[dep],'path':relative},'identity':kind,'archive_member_sha256':digest(member)}
    if len(result)!=412:raise ValueError('reviewed upstream patch roster differs')
    for row in rows:
        if row['path'] in result:
            item=result[row['path']]
            item['representation'],fields=build_identity(raw_source(row),item['identity'])
            item['upstream'].update(fields)
    return result
def plan():
    data=DELTA.read_bytes();rows=json.loads(data)['lanes']['fp16_readiness']
    if len(rows)!=1359 or len({r['source_sha256'] for r in rows})!=912:raise ValueError('reviewed delta changed')
    external=upstream(rows)
    names=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard','scripts','crates','packages','tools'],cwd=ROOT).decode().split('\0')
    live={}
    for n in names:
        p=ROOT/n
        if n and p.suffix.lower() in {'.patch','.diff'} and p.is_file() and not p.resolve().is_relative_to(DEST):live.setdefault(digest(p.read_bytes()),[]).append(n)
    objects={line.partition(' ')[0]:line.partition(' ')[2] for line in subprocess.check_output(['git','rev-list','--objects','HEAD'],cwd=ROOT).decode().splitlines()}
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip()
    out=[];copies={}
    for row in rows:
        raw=raw_source(row);h=row['source_sha256']
        entry={'source':row['path'],'original_sha256':h,'original_bytes':len(raw)}
        if row['path'] in external:entry.update(disposition='upstream_build_input',**external[row['path']])
        elif h in live:entry.update(disposition='exact_maintained_patch',destination=sorted(live[h])[0],maintained_sha256=h,maintained_bytes=len(raw))
        elif row['git_blob_oid'] in objects:entry.update(disposition='exact_git_blob',git_blob=row['git_blob_oid'],reachable_from_commit=head,git_owner_path=objects[row['git_blob_oid']])
        else:
            relative=copies.setdefault(h,'sources/'+h[:16]+'--'+Path(row['path']).name)
            destination=str((DEST/relative).relative_to(ROOT)).replace('\\','/')
            entry.update(disposition='preserved_authored_patch',destination=destination,maintained_sha256=h,maintained_bytes=len(raw),changes=['path only; raw patch bytes unchanged'],fresh_application_supported=False)
        out.append(entry)
    return {'schema':'airk.historical-patch-rescue/1','delta':file_pin(DELTA),'repository_head':head,'paths':len(rows),'unique_sha256':912,'preserved_unique':len(copies),'counts':dict(collections.Counter(r['disposition'] for r in out)),'files':out}
def verify(originals):
    data=json.loads((DEST/'rescue-map.json').read_bytes());count=0;checked_evidence=set()
    rows=data['files']
    if len(rows)!=1359 or len({r['source'] for r in rows})!=1359 or len({r['original_sha256'] for r in rows})!=912:
        raise ValueError('frozen complete patch roster differs')
    expected_counts={'preserved_authored_patch':932,'exact_maintained_patch':15,'upstream_build_input':412}
    if dict(collections.Counter(r['disposition'] for r in rows))!=expected_counts:
        raise ValueError('frozen disposition counts differ')
    if len({r['destination'] for r in rows if r['disposition']=='preserved_authored_patch'})!=817:
        raise ValueError('canonical authored patch owner count differs')
    if file_pin(DELTA)!=data['delta']:raise ValueError('original delta evidence changed')
    delta_rows=json.loads(DELTA.read_bytes())['lanes']['fp16_readiness']
    if {(r['source'],r['original_sha256'],r['original_bytes']) for r in rows}!={(r['path'],r['source_sha256'],r['bytes']) for r in delta_rows}:
        raise ValueError('delta source population differs')
    relocation=json.loads((DEST/'source-provenance.json').read_bytes())
    fields=('source','original_sha256','original_bytes','destination','maintained_sha256','maintained_bytes')
    expected={tuple(r[k] for k in fields) for r in rows if r['disposition'] in {'preserved_authored_patch','exact_maintained_patch'}}
    actual=[tuple(r[k] for k in fields) for r in relocation['files']]
    if relocation['schema']!='airk.source-relocation/1' or len(actual)!=947 or set(actual)!=expected:
        raise ValueError('standard relocation mapping differs')
    dispositions=json.loads((DEST/'source-disposition.json').read_bytes())
    indexed={r['source']:r for r in rows if r['disposition']=='upstream_build_input'}
    if dispositions['schema']!='airk.source-disposition/1' or len(dispositions['files'])!=412 or {r['source'] for r in dispositions['files']}!=set(indexed):
        raise ValueError('standard upstream disposition mapping differs')
    for row in dispositions['files']:
        old=indexed[row['source']]
        if row['source_sha256']!=old['original_sha256'] or row['source_bytes']!=old['original_bytes'] or any(row[k]!=old[k] for k in ('disposition','reason','evidence','upstream','identity','representation')):
            raise ValueError('standard upstream evidence differs')
    for row in data['files']:
        if originals:
            raw=(ROOT/row['source']).read_bytes()
            if digest(raw)!=row['original_sha256'] or len(raw)!=row['original_bytes']:raise ValueError('original patch changed')
        if row['disposition'] in {'preserved_authored_patch','exact_maintained_patch'}:
            raw=(ROOT/row['destination']).read_bytes()
            if digest(raw)!=row['maintained_sha256'] or len(raw)!=row['maintained_bytes']:raise ValueError('maintained patch differs')
            count+=1
        elif row['disposition']=='upstream_build_input':
            raw=raw_source({'path':row['source'],'bytes':row['original_bytes'],'source_sha256':row['original_sha256']})
            representation,fields=build_identity(raw,row['identity'])
            if row['representation']!=representation or any(row['upstream'].get(k)!=v for k,v in fields.items()):raise ValueError('upstream representation identity differs')
            for pin in row['evidence']:
                key=(pin['path'],pin['sha256'],pin['bytes'])
                if key not in checked_evidence:
                    path=ROOT/pin['path']
                    with path.open('rb') as stream:h=hashlib.file_digest(stream,'sha256').hexdigest()
                    if h!=pin['sha256'] or path.stat().st_size!=pin['bytes']:raise ValueError('upstream evidence changed')
                    checked_evidence.add(key)
        elif row['disposition']=='exact_git_blob':
            raw=subprocess.check_output(['git','cat-file','blob',row['git_blob']],cwd=ROOT)
            if digest(raw)!=row['original_sha256']:raise ValueError('Git patch differs')
        else:raise ValueError('unknown disposition')
    print(json.dumps({'paths':len(data['files']),'authored_mapped':count,'unique_preserved':data['preserved_unique'],'originals_compared':originals,'patches_applied':False}))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--plan',action='store_true');p.add_argument('--originals',action='store_true');a=p.parse_args()
    if a.plan:print(json.dumps(plan(),indent=2))
    else:verify(a.originals)
