# SPDX-License-Identifier: Apache-2.0
"""Opaque selected-Context data alignment. Separate Root content release required."""
import argparse
import gc
import hashlib
import json
import os
from pathlib import Path
import threading
import time
import types

DOMAIN=b"airk.selected-context-generic-read.r1/question\0"

def sha(path):
    with Path(path).open("rb")as stream:return hashlib.file_digest(stream,"sha256").hexdigest()
def need(value,reason):
    if not value:raise ValueError(reason)
def question_rank(part,qid):
    value=("airk.goal.final.v1:hotpot:"+qid).encode() if part=="CONFIRMATION" else DOMAIN+qid.encode()
    return hashlib.sha256(value).digest(),qid.encode()
def select(part,membership,rows,convert,minimum=100):
    ordered=sorted(membership,key=lambda qid:question_rank(part,qid))
    documents,tasks,audit={},[],[]
    counts={k:0 for k in ('projected','validated','eligible','mechanically_ineligible','selected','unselected_validated_tail')}
    for offset in range(0,len(ordered),32):
        wanted=ordered[offset:offset+32];captured={}
        for row in rows(wanted):
            qid=row['id'];need(qid in wanted and qid not in captured,'foreign/duplicate selected row');captured[qid]=row
        need(set(captured)==set(wanted),'selected row missing')
        validated=[]
        for qid in wanted:
            row=captured[qid]
            task,local,excluded,eligible=convert(row,membership[qid][1])
            validated.append((qid,row,task,local,excluded,eligible))
        # All projected tails validated BEFORE quota return. No quality-informed skips.
        for qid,row,task,local,excluded,eligible in validated:
            selected=len(tasks)<minimum and eligible
            counts['projected']+=1;counts['validated']+=1;counts['eligible' if eligible else 'mechanically_ineligible']+=1
            if selected:
                for identifier,document in local.items():
                    need(identifier not in documents or documents[identifier]==document,'conflicting original source version')
                    documents[identifier]=document
                task['experiment_partition']=part;task['split']='calibration' if part=='CAL' else 'test'
                task['source_component']=membership[qid][0]
                tasks.append(task);counts['selected']+=1
            elif len(tasks)>=minimum:counts['unselected_validated_tail']+=1
            audit.append({'component_digest':membership[qid][0],'original_row':row,'complete_neutral_task':task,'exclusions':excluded,'selected':selected})
        if len(tasks)==minimum:
            need(len(documents)>=30,'eligible document floor not met')
            return documents,tasks,audit,counts
    raise ValueError('fewer than100 mechanically eligible reserved questions')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol',type=Path,required=True);parser.add_argument('--protocol-sha256',required=True)
    parser.add_argument('--reservation',type=Path,required=True);parser.add_argument('--reservation-sha256',required=True)
    parser.add_argument('--root-decision',type=Path,required=True);parser.add_argument('--root-decision-sha256',required=True)
    parser.add_argument('--execute-reviewed-opaque-alignment',action='store_true',required=True)
    args=parser.parse_args();started=time.monotonic()
    need(sha(args.protocol)==args.protocol_sha256 and sha(args.reservation)==args.reservation_sha256 and sha(args.root_decision)==args.root_decision_sha256,'reviewed input drift')
    protocol=json.loads(args.protocol.read_bytes());decision=json.loads(args.root_decision.read_bytes())
    need(decision.get('schema')=='airk.private-selected-Context-opaque-alignment-decision/1' and decision.get('approved')is True and decision.get('protocol_sha256')==args.protocol_sha256 and decision.get('reservation_sha256')==args.reservation_sha256 and decision.get('opaque_content_alignment_only')is True and decision.get('neural_execution')is False,'Root content admission required')
    files=protocol['inputs'];before={row['path']:sha(row['path'])for row in files.values()}
    need(all(before[row['path']]==row['sha256']for row in files.values()),'sealed source drift')
    def imported(role,name):
        pin=files[role];raw=Path(pin['path']).read_bytes();need(hashlib.sha256(raw).hexdigest()==pin['sha256'],'source changed before import')
        module=types.ModuleType(name);module.__file__=pin['path'];exec(compile(raw,pin['path'],'exec'),module.__dict__);return module
    budget=imported('resource_owner','sealed_selected_data_budget');budget.JOB_STATE=budget.enforce_budget()
    done=threading.Event()
    def expire():
        if not done.wait(max(0,60-(time.monotonic()-started))):os._exit(124)
    threading.Thread(target=expire,daemon=True).start()
    adapter=imported('retained_adapter','sealed_selected_neutral_adapter')
    neutral=adapter.load_neutral(Path(files['neutral_owner']['path']).read_bytes())
    reservation=json.loads(args.reservation.read_bytes())
    need(reservation.get('schema')=='airk.private-selected-Context-opaque-reservation/1' and reservation.get('protocol_sha256')==protocol['metadata_protocol']['sha256'] and reservation.get('partitioning_before_any_new_question_or_gold_decode')is True and reservation.get('complete')is True and reservation.get('entire_previous_FINAL_bucket_excluded')is True and reservation.get('all_QASPER_excluded')is True,'foreign opaque reservation')
    membership={};seen_questions=set();seen_articles=set();seen_components=set()
    for part in ('CAL','CONFIRMATION'):
        membership[part]={}
        for component in reservation['reservations'][part]:
            need(component['original_disposition']=='CAL_SEALED_UNOPENED','non-CAL origin')
            qs,aa,c=component['question_ids'],set(component['article_digests']),component['component_digest']
            need(c not in seen_components and not seen_questions.intersection(qs) and not seen_articles.intersection(aa),'reserved source/component overlap')
            seen_components.add(c);seen_questions.update(qs);seen_articles.update(aa)
            for q in qs:membership[part][q]=(c,frozenset(aa))
    del reservation;gc.collect()
    anchor=Path(protocol['output_anchor']);target=anchor/'materialized'
    observer_raw=Path(files['anchor_observer']['path']).read_bytes()
    observe=lambda path:adapter.anchor_observation(path,observer_raw)
    proof,identity=adapter.guard_output(anchor,target,protocol['protected_input_roots'],observe)
    for key in ('OMP_NUM_THREADS','OPENBLAS_NUM_THREADS','MKL_NUM_THREADS','NUMEXPR_NUM_THREADS','VECLIB_MAXIMUM_THREADS'):os.environ[key]='1'
    import pyarrow
    pyarrow.set_cpu_count(1);pyarrow.set_io_thread_count(1)
    need(pyarrow.cpu_count()==1 and pyarrow.io_thread_count()==1,'thread cap')
    raw_path=files['raw_hotpot']['path'];adapter.regular(Path(raw_path))
    outputs={};receipts={}
    for part in ('CAL','CONFIRMATION'):
        documents,tasks,audit,counts=select(part,membership[part],lambda wanted:adapter.filtered_rows(raw_path,wanted),
            lambda row,allowed:adapter.convert_hotpot(row,allowed,budget.article,neutral))
        outputs[part]={'schema':'airk.private-human-evidence-corpus/1','experiment_partition':part,'documents':list(documents.values()),'tasks':tasks,
            'hotpot_alignment_audit_private':audit,'qasper_rights_decisions':[],
            'hotpot_license':'CC-BY-SA-4.0; private evaluation/reference-utility calibration only under explicit Root selected-Context scope. No distributable-weight training.',
            'annotation_authority':'Original HotpotQA human answer/support only; no AIRK sufficiency/gap/role/no-read semantic gold. Reference no-gain remains UNKNOWN.',
            'hotpot_attribution':'Yang,Qi,Zhang et al.(2018),HotpotQA; Wikipedia contributors under every retained original article URL. Publisher text unchanged except predeclared quote-bearing exclusion.',
            'reservation_sha256':args.reservation_sha256,'protocol_sha256':args.protocol_sha256,'complete':True}
        receipts[part]={'questions':len(tasks),'documents':len(documents),'counts':counts}
    need(not {d['id']for d in outputs['CAL']['documents']}.intersection(d['id']for d in outputs['CONFIRMATION']['documents']),'materialized source identity overlap')
    need(all(sha(path)==value for path,value in before.items()),'source drift during alignment')
    need((anchor.stat().st_dev,anchor.stat().st_ino)==identity and observe(anchor)==proof,'output anchor changed')
    need(time.monotonic()-started<60 and budget.peak_memory()<=268435456,'resource ceiling')
    target.mkdir()
    for part,corpus in outputs.items():
        folder=target/part;folder.mkdir()
        with (folder/'corpus.private.json').open('x',encoding='utf-8')as stream:json.dump(corpus,stream,ensure_ascii=False,separators=(',',':'));stream.write('\n');stream.flush();os.fsync(stream.fileno())
        receipts[part]['corpus_sha256']=sha(folder/'corpus.private.json')
    report={'schema':'airk.private-selected-Context-opaque-alignment/1','complete':True,'partitions':receipts,'protocol_sha256':args.protocol_sha256,'reservation_sha256':args.reservation_sha256,'root_decision_sha256':args.root_decision_sha256,'elapsed_seconds':time.monotonic()-started,'peak_committed_bytes':budget.peak_memory(),'human_semantic_gold_count':0,'source_inputs_unchanged':True,'neural_execution':False}
    need(report['elapsed_seconds']<60 and report['peak_committed_bytes']<=268435456,'postwrite resource ceiling')
    with (target/'opaque-counts.json').open('x',encoding='utf-8')as stream:json.dump(report,stream,indent=2);stream.write('\n')
    done.set();print(json.dumps(report))
if __name__=='__main__':
    try:main()
    except Exception as error:
        print(json.dumps({'complete':False,'error_class':type(error).__name__,'opaque_refusal':True}));raise SystemExit(1)
