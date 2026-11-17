# SPDX-License-Identifier: Apache-2.0
"""Private neutral TRAIN source fixture; no native DTOs, planning, NN or final use."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import sys
import tarfile
import types
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


ROOT=next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
MODELS=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str(ROOT/'artifacts/advanced-split/models'))).resolve()
HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'native-parent-bag-proposal-r1'
def sha(raw):return hashlib.sha256(raw).hexdigest()
def sealed(path,pin):
    raw=Path(path).read_bytes()
    if sha(raw)!=pin:raise ValueError('reviewed TRAIN source changed: '+str(path))
    return raw
def relative(name):
    p=PurePosixPath(name)
    if p.is_absolute() or not p.parts or any(x in ('','..','.') for x in p.parts) or '\\' in name or ':' in name:raise ValueError('unsafe original source path')
    return p
def function_span(raw,document):
    encoded=document['text'].encode();lines=raw.splitlines(keepends=True);line=document['source_line']
    if not encoded:raise ValueError('empty original function cannot be a positive parent')
    if type(line)is not int or not 1<=line<=len(lines):raise ValueError('invalid original function line')
    start=sum(map(len,lines[:line-1]));indent=len(lines[line-1])-len(lines[line-1].lstrip(b' \t'))
    candidates={at for at in (start,start+indent) if raw[at:at+len(encoded)]==encoded}
    if len(candidates)!=1:raise ValueError('original function byte mapping ambiguous/missing; never guess or dedent')
    begin=candidates.pop()
    return begin,begin+len(encoded)
def train_reader():
    folder=ROOT/'tools/retrieval-train'
    a=sealed(folder/'training_recipe_audit.py','7c27d0609977325305169881fdff7ad67960f9598b139e6518b41dd769a965d7')
    b=sealed(folder/'training_recipe.py','30ad6dfa26af9ca9f779116e9491ad25a197f88f1765f5d26fdc0796dfa6492d')
    saved=sys.modules.get('training_recipe_audit');module=types.ModuleType('training_recipe_audit');module.__file__=str(folder/'training_recipe_audit.py')
    try:
        sys.modules['training_recipe_audit']=module;exec(compile(a,module.__file__,'exec'),module.__dict__)
        recipe=types.ModuleType('_verified_train_recipe');recipe.__file__=str(folder/'training_recipe.py');exec(compile(b,recipe.__file__,'exec'),recipe.__dict__)
        return recipe
    finally:
        if saved is None:sys.modules.pop('training_recipe_audit',None)
        else:sys.modules['training_recipe_audit']=saved
def build(corpus,requests,archive,output,old_rows,new_rows):
    # Both row rosters are fixed before any native unit/loss is observed.
    if len(old_rows)!=315 or len(new_rows)!=315 or [(r['task_id'],r['question'],r['positive']) for r in old_rows]!=[(r['task_id'],r['question'],r['positive']) for r in new_rows]:raise ValueError('original315 positive/question/order identities changed')
    tasks=corpus['tasks']+requests['tasks'];documents=corpus['documents']+requests['documents'];by_id={d['id']:d for d in documents}
    if len(by_id)!=len(documents):raise ValueError('duplicate TRAIN parent')
    by_task={t['id']:t for t in tasks}
    for row in old_rows:
        task=by_task[row['task_id']]
        if task['split']!='train' or task['dataset'] not in ('QASPER','requests-original-docstrings') or task['question']!=row['question']:raise ValueError('foreign TRAIN question scope')
    if any(row['positive'] not in by_id or row['distractor'] not in by_id for row in new_rows):raise ValueError('fixed parent master escaped licensed TRAIN pool')
    if any(t['split']!='train' or t['dataset']!='requests-original-docstrings' for t in requests['tasks']):raise ValueError('Requests task outside original TRAIN scope')
    files=[];parents=[];bodies={};groups={}
    for d in documents:
        if d['dataset'] not in ('QASPER','requests-original-docstrings') or sha(d['text'].encode())!=d['paragraph_sha256']:raise ValueError('foreign or changed TRAIN parent')
        if d['dataset']=='QASPER':groups.setdefault(d['source_group'],[]).append(d)
    def parent(d,name,start,end):
        parents.append({'id':d['id'],'file':name,'byte_start':start,'byte_end':end,'paragraph_sha256':d['paragraph_sha256'],'dataset':d['dataset'],'source_group':d['source_group'],'original_url':d['url'],'license':d['license']})
    for group,docs in sorted(groups.items()):
        paper=group.removeprefix('qasper:')
        if not group.startswith('qasper:') or '/' in paper or '\\' in paper:raise ValueError('invalid original paper identity')
        docs.sort(key=lambda d:tuple(map(int,d['id'].split(':')[-2:])))
        name=f'sources/qasper/{paper}/{paper}.txt';body=bytearray()
        for d in docs:
            if body:body.extend(b'\n\n')
            start=len(body);body.extend(d['text'].encode());parent(d,name,start,len(body))
        bodies[name]=bytes(body)
    request_docs=[d for d in documents if d['dataset']=='requests-original-docstrings']
    if len(request_docs)!=218:raise ValueError('original218 Requests parent roster changed')
    needed={d['source_path']:d['source_file_sha256'] for d in request_docs}
    with tarfile.open(archive,mode='r:*') as tar:
        members=tar.getmembers()
        for path,pin in sorted(needed.items()):
            relative(path);matches=[m for m in members if m.name==path or m.name.endswith('/'+path)]
            if len(matches)!=1:raise ValueError('missing/ambiguous original Requests module')
            member=matches[0];relative(member.name)
            if not member.isfile() or member.size>16*1024*1024:raise ValueError('nonregular/oversized original Requests module')
            with tar.extractfile(member) as stream:raw=stream.read(16*1024*1024+1)
            if sha(raw)!=pin:raise ValueError('original Requests module bytes changed')
            name='sources/requests/'+path;bodies[name]=raw
            for d in request_docs:
                if d['source_path']==path:
                    if d['source_file_sha256']!=pin:raise ValueError('conflicting original source version')
                    start,end=function_span(raw,d);parent(d,name,start,end)
    for name,body in sorted(bodies.items()):
        relative(name);files.append({'file':name,'sha256':sha(body),'bytes':len(body)})
    if set(by_id)!={p['id'] for p in parents}:raise ValueError('missing/extra admitted TRAIN parent mapping')
    # Caller owns fresh directory and bounded runtime; source writes occur only
    # after complete exact source/annotation checks, never under an indexed root.
    for name,body in sorted(bodies.items()):
        path=output/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(body)
    return files,parents
def main():
    require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
    for name in ('HERE','PREP','H','MODELS'):
     if name in globals():require_private_root(globals()[name])
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    if args.output.exists() or args.output.parent.resolve()!=MODELS.resolve() or not args.output.name.startswith('native-parent-bag-source-'):raise ValueError('fresh direct sibling private TRAIN source output required; never nest under frozen inputs')
    corpus_path=MODELS/'human-corpus-r1/corpus.private.json';requests_path=MODELS/'requests-train-rights/pairs.private.json'
    corpus_raw=sealed(corpus_path,'6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f');requests_raw=sealed(requests_path,'916b398de0fc9774041fe174d0e585c862bc68e9aa18fa04028c3126f693a18f')
    old_raw=sealed(MODELS/'minilm-trained-r3/training-rows.private.json','1c0f106de7372b33a8b355aa5087fa466d830a5c4c6e00e14fedf5781f0659ac');new_raw=sealed(MODELS/'a1-visible-prefix-mining-candidate-r1/candidate-training-rows.private.json','314825dccad822874d3405305df789317c8e90a0a0e4cdcdb590acd27a555c48')
    reader=train_reader();corpus=reader.train_only_corpus(corpus_raw);requests=json.loads(requests_raw)
    rights=json.loads(reader.source._field(corpus_raw,'qasper_rights_decisions'))
    allowed_papers={'qasper:'+r['paper_id'] for r in rights if r['publisher_split']=='train' and r['license']=='CC-BY-4.0'}
    if any(d['source_group'] not in allowed_papers for d in corpus['documents']):raise ValueError('QASPER source outside licensed original TRAIN paper families')
    archive=MODELS/'requests-train-rights/source.tgz'
    if sha(archive.read_bytes())!=requests['archive_sha256']:raise ValueError('original source archive changed')
    files,parents=build(corpus,requests,archive,args.output,json.loads(old_raw),json.loads(new_raw))
    manifest={'schema':'airk.private-native-TRAIN-source-roster/1','corpus_sha256':sha(corpus_raw),'requests_sha256':sha(requests_raw),'parent_rows_sha256':sha(old_raw),'fixed_negative_rows_sha256':sha(new_raw),'files':files,'paragraphs':parents,'annotation_policy':'Whole original parent labels retained; no fragment-positive labels or new question/answer data'}
    (args.output/'manifest.private.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps({'source_files':len(files),'parent_documents':len(parents),'native_planning_or_NN':False}),flush=True)
if __name__=='__main__':main()
