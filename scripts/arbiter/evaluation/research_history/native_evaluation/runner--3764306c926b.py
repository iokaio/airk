# SPDX-License-Identifier: Apache-2.0
"""Private mechanical projection of original human answers/support into exact ranges.

This does not create human judgments of AIRK actions, authority, gaps or sufficiency.
"""
import argparse
from collections import Counter
import hashlib
import io
import json
from pathlib import Path
import re
import string

ROOT=Path(__file__).resolve().parents[5]


def normalize(text):
    text=''.join(c for c in text.lower() if c not in string.punctuation)
    return ' '.join(re.sub(r'\b(a|an|the)\b',' ',text).split())


def answer_text(original):
    if isinstance(original,str):return original
    if original['yes_no'] is not None:return 'yes' if original['yes_no'] else 'no'
    return ' '.join(original['extractive_spans']) if original['extractive_spans'] else original['free_form_answer']


def answer_score(predicted,expected):
    left,right=normalize(predicted),normalize(expected)
    if left in ('yes','no') or right in ('yes','no'):return float(left==right)
    a,b=left.split(),right.split()
    if not a or not b:return float(a==b)
    common=sum((Counter(a)&Counter(b)).values())
    return 2*common/(len(a)+len(b))


def contains(span,fact):
    return span['file']==fact['file'] and span['byte_start']<=fact['byte_start'] and span['byte_end']>=fact['byte_end']


def covered_location(location,spans):
    position=location['byte_start']
    for start,end in sorted((span['byte_start'],span['byte_end']) for span in spans if span['file']==location['file']):
        if end<=position:continue
        if start>position:return False
        position=max(position,end)
        if position>=location['byte_end']:return True
    return False


def complete_support(alternative,spans):
    return bool(alternative['facts']) and all(any(covered_location(location,spans) for location in fact['locations']) for fact in alternative['facts'])


def next_read_utility(task,before_spans,after_spans):
    """Mechanical missing-human-support utility, never action-authority/sufficiency."""
    values=[]
    for alternative in task['alternatives']:
        def covered(spans):return sum(any(covered_location(location,spans) for location in fact['locations']) for fact in alternative['facts'])
        before,after=covered(before_spans),covered(after_spans)
        values.append({'human_annotation_id':alternative['human_annotation_id'],'before_facts':before,'after_facts':after,'adds_missing_original_support':after>before})
    return {'useful':any(value['adds_missing_original_support'] for value in values),'alternatives':values}


def grade(task,predicted,actual_returned_spans,validated_cited_spans):
    """Caller must authenticate actual source bytes and citation references first."""
    values=[]
    for alternative in task['alternatives']:
        expected=alternative['answer'];f1=answer_score(predicted,expected)
        support=complete_support(alternative,actual_returned_spans) and complete_support(alternative,validated_cited_spans)
        values.append({'human_annotation_id':alternative['human_annotation_id'],'token_f1':f1,'exact_match':normalize(predicted)==normalize(expected),'complete_support':support,'completion':f1>=.8 and support})
    return {'completion':any(v['completion'] for v in values),'alternatives':values}


def streaming_sha256(path, block_bytes=65536):
    """Exact bounded file digest; never retain the raw publisher parquet."""
    digest = hashlib.sha256()
    buffer = bytearray(block_bytes)
    with Path(path).open('rb') as stream:
        while count := stream.readinto(buffer):
            digest.update(memoryview(buffer)[:count])
    return digest.hexdigest()


def main():
    import pyarrow.dataset as dataset
    import pyarrow as arrow
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--corpus',type=Path,required=True);parser.add_argument('--fixture',type=Path,required=True)
    parser.add_argument('--hotpot',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--corpus-sha256',required=True);parser.add_argument('--source-manifest-sha256',required=True)
    parser.add_argument('--partition',choices=('CAL','CONFIRMATION'),required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    paths=(Path(__file__),args.corpus,args.fixture/'manifest.private.json')
    retained={p:p.read_bytes() for p in paths};sha=lambda raw:hashlib.sha256(raw).hexdigest()
    if sha(retained[args.corpus])!=args.corpus_sha256:raise ValueError('unreviewed corpus')
    if sha(retained[args.fixture/'manifest.private.json'])!=args.source_manifest_sha256:raise ValueError('unreviewed native source mapping')
    hotpot_sha256=streaming_sha256(args.hotpot)
    if hotpot_sha256!='c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6':raise ValueError('unreviewed human sentence bytes')
    corpus=json.loads(retained[args.corpus]);fixture=json.loads(retained[args.fixture/'manifest.private.json'])
    def verify_sources():
        for row in fixture['files']:
            raw=(args.fixture/row['file']).read_bytes()
            if len(raw)!=row['bytes'] or sha(raw)!=row['sha256']:raise ValueError('native source bytes differ')
    verify_sources()
    mapping={p['id']:p for p in fixture['paragraphs']};documents={d['id']:d for d in corpus['documents']}
    if corpus.get('experiment_partition')!=args.partition or fixture.get('experiment_partition')!=args.partition:raise ValueError('selected partition rubric admission required')
    wanted={task['question_id']for task in corpus['tasks']}
    arrow.set_cpu_count(1);arrow.set_io_thread_count(1)
    scanner=dataset.dataset(args.hotpot,format='parquet').scanner(
        columns=['id','context','type','level'],filter=dataset.field('id').isin(sorted(wanted)),
        batch_size=32,batch_readahead=0,fragment_readahead=0,use_threads=False,
        fragment_scan_options=dataset.ParquetFragmentScanOptions(use_buffered_stream=True,buffer_size=8192,pre_buffer=False))
    hotpot={}
    for batch in scanner.to_batches():
        for row in batch.to_pylist():
            if row['id']not in wanted or row['id']in hotpot:raise ValueError('foreign/duplicate opaque support projection')
            hotpot[row['id']]=row
    if set(hotpot)!=wanted:raise ValueError('opaque support projection missing selected IDs')
    tasks=[];parents={}
    def root(key):
        parents.setdefault(key,key)
        while parents[key]!=key:key=parents[key]
        return key
    for task in corpus['tasks']:
        alternatives=[];groups=set()
        for original in task['human_alternatives']:
            if original['unanswerable'] or original['alignment']!='all_original_evidence_maps_to_allowed_text':continue
            facts=[]
            for fact in original['evidence']:
                locations=[]
                for identifier in fact['allowed_locations']:
                    paragraph=mapping[identifier];start,end=paragraph['byte_start'],paragraph['byte_end']
                    if task['dataset']=='HotpotQA':
                        row=hotpot[task['question_id']];titles=row['context']['title'];sentences=row['context']['sentences'][titles.index(fact['source_title'])]
                        sentence=sentences[fact['sentence_index']].encode()
                        if sha(sentence)!=fact['source_evidence_sha256']:raise ValueError('human sentence hash differs')
                        start+=len(''.join(sentences[:fact['sentence_index']]).encode());end=start+len(sentence)
                    locations.append({'file':paragraph['file'],'byte_start':start,'byte_end':end,'original_url':paragraph['original_url'],'source_group':paragraph['source_group']})
                    groups.add(paragraph['source_group'])
                    raw=documents[identifier]['text'].encode();local_start=start-paragraph['byte_start'];local_end=end-paragraph['byte_start']
                    if not raw[local_start:local_end]:raise ValueError('empty required evidence range')
                facts.append({'original_evidence_sha256':fact.get('source_evidence_sha256'),'locations':locations})
            answer=original['original_human_answer']
            kind='short_answer' if isinstance(answer,str) else 'yes_no' if answer['yes_no'] is not None else 'extractive_spans' if answer['extractive_spans'] else 'free_form'
            alternatives.append({'human_annotation_id':original['human_annotation_id'],'answer':answer_text(answer),'answer_kind':kind,'facts':facts})
        if not alternatives:continue
        groups=sorted(groups)
        if task['split']=='test':
            for group in groups[1:]:parents[root(group)]=root(groups[0])
        categories={key:hotpot[task['question_id']][key] for key in ('type','level')} if task['dataset']=='HotpotQA' else {'answer_kinds':sorted({a['answer_kind'] for a in alternatives})}
        tasks.append({'id':task['id'],'dataset':task['dataset'],'split':task['split'],'publisher_split':task['publisher_split'],'question':task['question'],'original_question_categories':categories,'disagreement':task['disagreement'],'source_component':task['source_component'],'goal_selected':task['id'] in fixture['goal_ids'],'source_groups':groups,'alternatives':alternatives})
    for task in tasks:
        if task['split']=='test':task['bootstrap_source_component']=next(original['source_component']for original in corpus['tasks']if original['id']==task['id'])
    if any(p.read_bytes()!=raw for p,raw in retained.items()) or streaming_sha256(args.hotpot)!=hotpot_sha256:raise ValueError('rubric input drift')
    verify_sources()
    output.mkdir(parents=True)
    report={'schema':'airk.private-human-goal-rubric/1','experiment_partition':args.partition,'tasks':tasks,'input_seals':{**{str(p):sha(raw) for p,raw in retained.items()},str(args.hotpot):hotpot_sha256},
        'authority':'Primary approved exact rubric before final scoring: tokenF1>=.80, exact yes/no, complete support/citations from SAME original human alternative; exact match secondary; no invented aliases or unions.',
        'projection':'Original human answers and evidence mechanically mapped to original source byte ranges. No human labels of AIRK actions/authority/gap/sufficiency. Source connected components conservatively group bootstrap dependence only; never union evidence into gold.',
        'caller_requirements':'Authenticate actual returned source bytes/ranges and citation reference identities before grade(); include failures/all model turns/internal reads separately. No query-only support credit.'}
    (output/'rubric.private.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');(output/'runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({'tasks':len(tasks),'final':sum(t['split']=='test' for t in tasks),'goal':sum(t['goal_selected'] for t in tasks),'final_source_components':len({t['bootstrap_source_component'] for t in tasks if t['split']=='test'}),'sha256':sha((output/'rubric.private.json').read_bytes())}))


if __name__=='__main__':main()
