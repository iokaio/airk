# SPDX-License-Identifier: Apache-2.0
"""Independently grade frozen native205 vector results by original source bytes."""
import argparse
from collections import Counter
from fractions import Fraction
import hashlib
import json
from pathlib import Path

from goal_rubric import covered_location
from goal_sources import SourceLedger

ROOT=Path(__file__).resolve().parents[2]


def evidence_recall(task,spans):
    alternatives=[]
    for alternative in task['alternatives']:
        facts=alternative['facts']
        count=sum(any(covered_location(location,spans) for location in fact['locations']) for fact in facts)
        alternatives.append(Fraction(count,len(facts)) if facts else Fraction(0))
    best=max(alternatives,default=Fraction(0))
    return best,int(best==1)


def cluster_interval(values,clusters):
    import numpy as np
    groups=sorted(set(clusters));sums=np.asarray([sum(value for value,group in zip(values,clusters) if group==key) for key in groups],dtype=np.float64)
    counts=np.asarray([sum(group==key for group in clusters) for key in groups],dtype=np.float64)
    choices=np.random.default_rng(1908).integers(0,len(groups),size=(10000,len(groups)))
    samples=sums[choices].sum(axis=1)/counts[choices].sum(axis=1)
    return [float(value) for value in np.quantile(samples,[.025,.975])]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native',type=Path,required=True);parser.add_argument('--questions',type=Path,required=True)
    parser.add_argument('--encoders',type=Path,required=True);parser.add_argument('--rubric',type=Path,required=True)
    parser.add_argument('--fixture',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    paths=(Path(__file__),args.native,args.questions,args.encoders,args.rubric,Path(__file__).with_name('goal_sources.py'),Path(__file__).with_name('goal_rubric.py'),Path(__file__).with_name('encoder-baseline-freeze.json'),Path(__file__).with_name('encoder-query-freeze.json'))
    retained={path:path.read_bytes() for path in paths};sha=lambda raw:hashlib.sha256(raw).hexdigest()
    if sha(retained[args.questions])!='2e74a873125834e17455c40879c0ba161626053fbd39742bcbbc73077d3dd046' or sha(retained[args.encoders])!='3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81' or sha(retained[args.rubric])!='6004b1e1427661b236664352c9e1fdada5993d184685ca61bc13eba1a6b85423':raise ValueError('frozen native question/encoder/original-human input identity changed')
    native=json.loads(retained[args.native]);questions={row['id']:row for row in json.loads(retained[args.questions])['questions']}
    encoders={row['family']:row for row in json.loads(retained[args.encoders])};tasks={row['id']:row for row in json.loads(retained[args.rubric])['tasks'] if row['split']=='test'}
    if native.get('complete') is not True or native['question_input_sha256']!=sha(retained[args.questions]) or set(questions)!=set(tasks) or len(tasks)!=205:raise ValueError('native final campaign incomplete or wrong population')
    if len(native['families'])!=6 or {row['family'] for row in native['families']}!=set(encoders):raise ValueError('native campaign does not contain exact six encoder families')
    graded={}
    for family in native['families']:
        name=family['family'];records=family['records']
        if family['model_json_sha256']!=encoders[name]['model_json_sha256'] or len(records)!=205 or {row['id'] for row in records}!=set(tasks):raise ValueError('native family model/population changed')
        rows=[];states=Counter()
        for record in records:
            identifier=record['id'];task=tasks[identifier];question=questions[identifier]
            if not Path(record['source_root']).samefile(question['source_root']):raise ValueError('native request source scope changed')
            value=record['primary_vector'];state=value.get('semantic_readiness',{}).get('state','unreported');states[state]+=1
            if len(value['hits'])>10:raise ValueError('native result exceeded frozen top10')
            ledger=SourceLedger(args.fixture,question['source_root']);ledger.observe(value)
            recall,complete=evidence_recall(task,list(ledger.records.values()))
            rows.append({'id':identifier,'dataset':task['dataset'],'original_question_categories':task['original_question_categories'],'original_human_disagreement':task['disagreement'],'source_component':task['bootstrap_source_component'],'evidence_recall':float(recall),'evidence_recall_fraction':[recall.numerator,recall.denominator],'complete_original_alternative':complete,'source_spans':list(ledger.records.values()),'native_truncated':value['truncated'],'returned_hits':len(value['hits']),'semantic_state':state})
        rows.sort(key=lambda row:row['id']);fractions=[Fraction(*row['evidence_recall_fraction']) for row in rows];mean=sum(fractions,Fraction(0))/205;complete_count=sum(row['complete_original_alternative'] for row in rows)
        clusters=[row['source_component'] for row in rows]
        graded[name]={'questions':205,'mean_evidence_recall':float(mean),'mean_evidence_recall_fraction':[mean.numerator,mean.denominator],'complete_count':complete_count,'complete_original_alternative_recall':complete_count/205,'semantic_states':dict(states),'all_native_semantic_ready':states['ready']==205,
            'absolute_quality_pass':mean>=Fraction(85,100) and complete_count*100>=70*205 and states['ready']==205,
            'descriptive_mean_recall_95_interval':cluster_interval([row['evidence_recall'] for row in rows],clusters),'descriptive_complete_recall_95_interval':cluster_interval([row['complete_original_alternative'] for row in rows],clusters),
            'dataset_families':{dataset:{'questions':sum(row['dataset']==dataset for row in rows),'mean_evidence_recall':sum(row['evidence_recall'] for row in rows if row['dataset']==dataset)/sum(row['dataset']==dataset for row in rows),'complete_count':sum(row['complete_original_alternative'] for row in rows if row['dataset']==dataset)} for dataset in sorted({row['dataset'] for row in rows})},'rows':rows}
    comparisons=[]
    for candidate in ('trained-v2-u8','trained-v2-s8'):
        current=graded[candidate];current_mean=Fraction(*current['mean_evidence_recall_fraction'])
        for control in ('original-shipped-u8-ort130-evaluation','original-mixed-u8-ort130-evaluation'):
            baseline=graded[control];gain=current['complete_count']-baseline['complete_count'];mean_delta=current_mean-Fraction(*baseline['mean_evidence_recall_fraction'])
            comparisons.append({'candidate':candidate,'control':control,'complete_gain_count':gain,'complete_gain':gain/205,'mean_recall_delta':float(mean_delta),'paired_floor_pass':gain*100>=3*205 and mean_delta>=Fraction(-2,100) and current['all_native_semantic_ready'] and baseline['all_native_semantic_ready'],
                'descriptive_complete_gain_95_interval':cluster_interval([a['complete_original_alternative']-b['complete_original_alternative'] for a,b in zip(current['rows'],baseline['rows'])],[row['source_component'] for row in current['rows']])})
    if any(path.read_bytes()!=raw for path,raw in retained.items()):raise ValueError('native grading source/input drift')
    output.mkdir(parents=True)
    result={'schema':'airk.private-native-retrieval-human-grading/1','release_qualified':False,'input_seals':{str(path):sha(raw) for path,raw in retained.items()},'families':graded,'paired_comparisons':comparisons,'a1_quality_pass':all(graded[name]['absolute_quality_pass'] for name in ('trained-v2-u8','trained-v2-s8')) and all(row['paired_floor_pass'] for row in comparisons),
        'scope':'Actual six-family native vector205 human-evidence grading only, exact original byte ranges/same-alternative recall. Baseline numerical failures remain controls, never promoted. No result fields normalized away.95% source-component paired descriptive bootstrap,10000samples/seed1908; no99% noninferiority or native transport/resource/goal/package qualification.'}
    (output/'grading.private.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');(output/'runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({'a1_quality_pass':result['a1_quality_pass'],'families':{name:{key:value for key,value in row.items() if key not in ('rows','dataset_families')} for name,row in graded.items()},'comparisons':comparisons}))


if __name__=='__main__':main()
