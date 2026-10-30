# SPDX-License-Identifier: Apache-2.0
"""Build the fixed hashed linear intent asset from original, fictional templates.

No project text is retained. Public-repository harvesting/qualification is a separate
gate; the shipped candidate records only the measurements actually performed here.
"""
import hashlib
import json
from pathlib import Path
import random
import re
import struct

import numpy as np
from scipy.sparse import csr_matrix
from sklearn.linear_model import LogisticRegression
from retention import retain

ROOT = Path(__file__).resolve().parents[2]
LABELS = ['search','find_definition','references','read_file','outline','list_files','file_info','imports','facts','changes','status','diagnostics','help']
TEMPLATES = {
    'search': ['find documents about {s}', 'search for {s}', 'look for text {s}', 'find evidence about {s}', 'search indexed text for {s}', 'locate mentions of the topic {s}'],
    'find_definition': ['where is {s} defined', 'find the definition of {s}', 'locate declaration {s}', 'show the symbol {s}', 'find definition {s}', 'where can I find class {s}'],
    'references': ['who calls {s}', 'find references to {s}', 'show usages of {s}', 'where is {s} used', 'textual occurrences of {s}', 'list references for {s}'],
    'read_file': ['read {p}', 'show lines 2 to 8 in {p}', 'open the indexed text of {p}', 'read indexed lines from {p}', 'show file content {p}', 'give me text from {p}'],
    'outline': ['outline {p}', 'show the structure of {p}', 'list headings in {p}', 'show declarations in file {p}', 'get outline for {p}', 'what is the outline of {p}'],
    'list_files': ['list files under {p}', 'show indexed files in {p}', 'file inventory for {p}', 'which files are indexed under {p}', 'enumerate paths in {p}', 'browse files in {p}'],
    'file_info': ['file information for {p}', 'what language is {p}', 'show parsing status of {p}', 'file hash for {p}', 'inspect file metadata for {p}', 'extraction details for {p}'],
    'imports': ['show imports mentioning {s}', 'list imported dependencies for {s}', 'find import statements for {s}', 'where is {s} imported', 'show include declarations for {s}', 'find written imports of {s}'],
    'facts': ['find requirements about {s}', 'show document facts about {s}', 'find decisions mentioning {s}', 'list architecture statements about {s}', 'show design facts for {s}', 'find acceptance criteria for {s}'],
    'changes': ['what changed since the baseline', 'validate remembered evidence', 'show indexed changes', 'which evidence changed since indexing', 'check changes to saved evidence', 'compare indexed change observations'],
    'status': ['show index status', 'is the index ready', 'how many files are indexed', 'check retrieval readiness', 'show semantic readiness', 'what is the current index publication'],
    'diagnostics': ['show service diagnostics', 'inspect runtime jobs and watcher state', 'diagnose the native workers', 'show runtime troubleshooting details', 'check detailed diagnostics', 'inspect current worker progress'],
    'help': ['help with AIRK', 'how do I use commands', 'show AIRK usage guidance', 'explain how to search with AIRK', 'show help topics', 'list available commands'],
}

def features(text):
    words = re.findall(r'[^\W]+', text.lower(), re.UNICODE)
    def bucket(text):
        value=14695981039346656037
        for byte in text.encode():
            value=((value ^ byte)*1099511628211)&0xffffffffffffffff
        return value & 65535
    terms=set()
    for word in words:
        terms.add('w:'+word)
        marked='<'+word+'>'
        for n in range(3,6):
            terms.update('c:'+marked[i:i+n] for i in range(len(marked)-n+1))
    terms.update('b:'+a+' '+b for a,b in zip(words,words[1:]))
    if '/' in text or '\\' in text: terms.add('path')
    if ':' in text: terms.add('range')
    if '"' in text or "'" in text: terms.add('quote')
    return sorted({bucket(term) for term in terms})

def matrix(texts):
    rows=[];cols=[]
    for row,text in enumerate(texts):
        indices=features(text);rows.extend([row]*len(indices));cols.extend(indices)
    return csr_matrix((np.ones(len(rows)),(rows,cols)),shape=(len(texts),65536))

def main():
    rng=random.Random(84173)
    # Identifiers/paths are fictional and deliberately independent of any user's project.
    names=['QueueClient','request_budget','PacketStore','CacheReader','retryPolicy','RenderPanel','EventSource','IndexGate']
    paths=['docs/design.md','src/client.rs','tests/queue.ts','lib/reader.py','docs/guide.md']
    texts=[];labels=[]
    for i,label in enumerate(LABELS):
        for template in TEMPLATES[label]:
            for name in names:
                for path in paths:
                    text=template.format(s=name,p=path)
                    for prefix in ['', 'please ', 'can you ', 'I need to ']:
                        texts.append(prefix+text);labels.append(i)
    order=list(range(len(texts)));rng.shuffle(order)
    # Preserve the actual expanded examples locally, in training order. Hash-named
    # files retain earlier corpora rather than replacing them on regeneration.
    dataset=b''.join((json.dumps({'example_id':i,'text':texts[i],'intent':LABELS[labels[i]],'split':'train','origin':'synthetic-template'},ensure_ascii=False,sort_keys=True)+'\n').encode('utf-8') for i in order)
    dataset_hash=hashlib.sha256(dataset).hexdigest()
    dataset_path=ROOT/'target/parser-train/data'/f'synthetic-{dataset_hash[:16]}.jsonl'
    dataset_path.parent.mkdir(parents=True,exist_ok=True)
    if dataset_path.exists():
        if hashlib.sha256(dataset_path.read_bytes()).hexdigest()!=dataset_hash:
            raise ValueError('Existing synthetic corpus differs from its hash; it was preserved.')
    else:
        with dataset_path.open('xb') as stream: stream.write(dataset)
    retain(dataset_path)
    model=LogisticRegression(C=2,max_iter=250,solver='liblinear',random_state=84173)
    # One-vs-rest is explicit to keep fitting independent of deprecated multi_class defaults.
    from sklearn.multiclass import OneVsRestClassifier
    fitted=OneVsRestClassifier(model,n_jobs=1).fit(matrix([texts[i] for i in order]),np.array([labels[i] for i in order]))
    weights=np.stack([m.coef_[0] for m in fitted.estimators_],axis=1)
    bias=np.array([m.intercept_[0] for m in fitted.estimators_])
    scale=120/np.max(np.abs(weights));quantized=np.rint(weights*scale).astype(np.int8);bias=np.rint(bias*scale).astype('<i4')
    payload=quantized.tobytes()+bias.tobytes()
    asset=b'AIRKINT1'+struct.pack('<IIIIii',1,65536,13,1,0,80)+hashlib.sha256('\n'.join(LABELS).encode()).digest()+hashlib.sha256(payload).digest()+payload
    destination=ROOT/'target/parser-train/models'/('intent-'+hashlib.sha256(asset).hexdigest()[:16]+'.bin');destination.parent.mkdir(parents=True,exist_ok=True)
    if not destination.exists():destination.write_bytes(asset)
    retain(destination)
    fixtures=[{'text':text,'features':features(text)} for text in ['who calls QueueClient','read "docs/design.md":2-8','Évidence_client αβ','where is retry_budget defined']]
    (Path(__file__).parent/'fixtures.json').write_text(json.dumps(fixtures,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    training={'seed':84173,'examples':len(texts),'dataset':{'path':dataset_path.relative_to(ROOT).as_posix(),'sha256':dataset_hash,'rows':len(texts)},'asset_bytes':len(asset),'asset_sha256':hashlib.sha256(asset).hexdigest(),'feature_spec':1,'threshold':0,'margin':80,'corpus':'original fictional templates; no public repository clones or source text','qualification':'candidate; held-out repository and independent 150-utterance acceptance remain unmeasured'}
    (Path(__file__).parent/'manifest-smoke.json').write_text(json.dumps(training,indent=2)+'\n')
    print(json.dumps(training))

if __name__=='__main__':main()
