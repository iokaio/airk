# SPDX-License-Identifier: Apache-2.0
import hashlib,json,re
DOMAIN='airk.f2.general-replay.fixed45/1\nseed1908\n'

def sha(raw):return hashlib.sha256(raw).hexdigest()
def canonical(value):return json.dumps(value,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()
def base_family(source_group):
    if not re.fullmatch(r'qasper:\d{4}\.\d{4,5}',source_group):raise ValueError('unrecognized arXiv family')
    return source_group

def gate(task,excluded):
    family=base_family(task['source_group'])
    if task['dataset']!='QASPER'or task['split']!='train'or task['publisher_split']!='train'or family in excluded:raise ValueError('nonTRAIN or reserved family')

def full_window(raw,start,end,expected):
    if type(start)is not int or type(end)is not int or not 0<=start<end<=len(raw):raise ValueError('invalid byte span')
    value=raw[start:end];value.decode('utf8')
    if sha(value)!=expected:raise ValueError('window bytes changed')
    return value

def round_robin(candidates,limit=45):
    if len({r['id']for r in candidates})!=len(candidates):raise ValueError('duplicate candidate question')
    groups={}
    for row in candidates:
        groups.setdefault(row['family'],[]).append(row)
    for family,rows in groups.items():
        if len({r['id']for r in rows})!=len(rows):raise ValueError('duplicate candidate question')
        rows.sort(key=lambda r:sha((DOMAIN+'question:'+r['id']).encode()))
    order=sorted(groups,key=lambda f:sha((DOMAIN+'family:'+f).encode()))
    selected=[];turn=0
    while len(selected)<limit:
        added=False
        for family in order:
            if turn<len(groups[family]):selected.append(groups[family][turn]);added=True
            if len(selected)==limit:return selected
        if not added:raise ValueError('not enough fixed unique TRAIN questions')
        turn+=1
    return selected

def positive_locations(task,docs,original_by_annotation):
    out=[]
    for alt in task['human_alternatives']:
        if alt['unanswerable']or alt['alignment']!='all_original_evidence_maps_to_allowed_text':continue
        original=original_by_annotation[alt['human_annotation_id']]
        if original['answer']['unanswerable']or sha(json.dumps(original,sort_keys=True,ensure_ascii=False).encode())!=alt['original_record_sha256']:raise ValueError('human annotation record changed')
        if len(original['answer']['evidence'])!=alt['original_evidence_count']or len(alt['evidence'])!=alt['original_evidence_count']:raise ValueError('human evidence denominator changed')
        for observed,evidence in zip(original['answer']['evidence'],alt['evidence']):
            if sha(observed.encode())!=evidence['source_evidence_sha256']:raise ValueError('human evidence string changed')
            for identifier in evidence['allowed_locations']:
                d=docs[identifier]
                if d['dataset']!='QASPER'or d['source_group']!=task['source_group']or d['license']!='CC-BY-4.0'or d['text']!=observed or d['paragraph_sha256']!=sha(observed.encode()):raise ValueError('original paragraph/rights differ')
                if any(c in observed for c in '\"“”«»„'):raise ValueError('quote-bearing material not allowed')
                out.append((alt,identifier))
    return out
