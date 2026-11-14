# SPDX-License-Identifier: Apache-2.0
"""Project real counterfactual reads onto original human support annotations."""
import argparse
import hashlib
import json
from pathlib import Path

from goal_rubric import next_read_utility,complete_support
from goal_sources import SourceLedger

ROOT=Path(__file__).resolve().parents[2]


def counterfactual_utilities(task,source_factory,before,outcomes):
    original=source_factory();original.observe(before);before_spans=list(original.records.values());labels=[]
    for outcome in outcomes:
        # Each counterfactual starts from precisely the same actual prior reads.
        # Never leak another candidate's read into this candidate's label.
        projected=source_factory();projected.observe(before);projected.observe(outcome['canonical_result'])
        utility=next_read_utility(task,before_spans,list(projected.records.values()))
        labels.append({'choice_id':outcome['choice']['id'],'useful':utility['useful'],'original_alternative_projection':utility['alternatives']})
    return {'before_any_original_alternative_complete':any(complete_support(alt,before_spans) for alt in task['alternatives']),'choices':labels}


def validate_search_outcome(outcome, native):
    """Audit the actual search bytes without reproducing Core's literal DTO/policy.

    Native SourceLiteral owns full record/span/closed-policy validation. This
    projection additionally refuses invented query bytes or expanded read bounds.
    """
    choice, arguments = outcome['choice'], outcome['arguments']
    query = arguments.get('query')
    if choice['operation'] != 'search' or not choice['target'].startswith('airk.literal-search/1:') or not isinstance(query, str) or not query:
        raise ValueError('counterfactual search lacks native literal provenance')
    if arguments != {'query': query, 'limit': 4, 'max_bytes': 4096, 'full_chunks': True, 'response_mode': 'full', 'include_status': True}:
        raise ValueError('counterfactual search budget/catalog differs')
    if not any(query.encode() in hit['text'].encode() for record in native['records'] for hit in record['canonical_result'].get('hits', [])):
        raise ValueError('counterfactual search query is not exact already-returned source bytes')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture',type=Path,required=True);parser.add_argument('--native-input',type=Path,required=True)
    parser.add_argument('--rubric',type=Path,required=True);parser.add_argument('--fixture',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output required')
    retained={path:path.read_bytes() for path in (Path(__file__),args.capture,args.native_input,args.rubric,Path(__file__).with_name('goal_rubric.py'),Path(__file__).with_name('goal_sources.py'))}
    sha=lambda data:hashlib.sha256(data).hexdigest()
    if sha(retained[args.native_input])!='a6149309a5838550a91cf47dd7489380509f82781550fb720fe78f8c059287b7':raise ValueError('unreviewed calibration request population')
    if sha(retained[args.rubric])!='6004b1e1427661b236664352c9e1fdada5993d184685ca61bc13eba1a6b85423':raise ValueError('unreviewed original human annotation projection')
    capture=json.loads(retained[args.capture]);request=json.loads(retained[args.native_input]);rubric=json.loads(retained[args.rubric])
    population=capture.get('read_population')
    if population not in ('lexical','trained-v2-native-default-hybrid'):raise ValueError('actual native capture population omitted/unknown')
    inputs={question['id']:question for question in request['questions']};tasks={task['id']:task for task in rubric['tasks'] if task['split']=='calibration'}
    if len(capture['rows'])!=19 or {row['id'] for row in capture['rows']}!=set(inputs) or len(inputs)!=19:raise ValueError('capture changed frozen calibration IDs')
    result=[]
    for row in capture['rows']:
        identifier=row['id'];question=inputs[identifier];task=tasks[identifier];native=row['request']
        if population=='trained-v2-native-default-hybrid' and (row['before'].get('execution',{}).get('served_mode')!='hybrid' or row['before'].get('execution',{}).get('incomplete') is not False):raise ValueError('native hybrid calibration silently fell back/incomplete')
        if native['question']!=question['question'] or task['question']!=question['question']:raise ValueError('native question differs from original human calibration question')
        if len(native['records'])!=1 or native['records'][0]['canonical_result']!=row['before'] or native['records'][0]['scope']!=native['scope']:raise ValueError('actual prior-read scope/result changed')
        expected={choice['id']:choice for choice in native['choices']};outcomes=row['counterfactual_choices']
        if len(outcomes)!=len(expected) or len(expected)!=len(native['choices']) or {value['choice']['id'] for value in outcomes}!=set(expected):raise ValueError('counterfactual choice identities changed')
        for outcome in outcomes:
            choice=outcome['choice'];arguments=outcome['arguments']
            if choice!=expected[choice['id']] or choice['operation'] not in ('read_evidence','context','search'):raise ValueError('foreign counterfactual native choice')
            if choice['operation']=='search':
                validate_search_outcome(outcome,native)
                continue
            path,span=choice['target'].rsplit(':',1);first,last=span.split('-',1)
            if choice['operation']=='context':
                actual=(arguments['path'],arguments['line'],arguments['before'],arguments['after']);wanted=(path,int(first),0,32)
                if int(last)!=int(first)+32:raise ValueError('native context target range differs')
            else:actual=(arguments['path'],arguments['start_line'],arguments['end_line']);wanted=(path,int(first),int(last))
            if actual!=wanted or arguments['max_bytes']!=4096 or arguments['revision']!=native['scope']['publication']['token']:raise ValueError('counterfactual read arguments differ from actual legal choice')
        factory=lambda:SourceLedger(args.fixture,question['source_root'])
        utility=counterfactual_utilities(task,factory,row['before'],outcomes)
        result.append({'id':identifier,**utility,'applicable':bool(utility['choices']),'has_useful_choice':any(choice['useful'] for choice in utility['choices'])})
    if any(path.read_bytes()!=data for path,data in retained.items()):raise ValueError('calibration label source/input drift')
    output.mkdir(parents=True)
    report={'schema':'airk.private-mechanical-human-read-utility/1','release_qualified':False,'rows':result,'input_seals':{str(path):sha(data) for path,data in retained.items()},'capture_read_population':population,'capture_scope':capture['scope'],
        'scope':'Actual counterfactual legal-read utility projected separately within each original human alternative; never a union, synthetic teacher label or human judgment of AIRK authority/gap/sufficiency. No original-annotation changes or final questions. Capture route/transport limits retained verbatim; projection alone does not qualify the resident/SDK transport or final cascade.',
        'outcome':'Adds a missing original human support fact within at least one original alternative. Other unannotated context is not asserted irrelevant. Report cases whose prior reads already complete another alternative separately.'}
    (output/'labels.private.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8');(output/'runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({'questions':len(result),'choices':sum(len(row['choices']) for row in result),'questions_with_useful_choice':sum(row['has_useful_choice'] for row in result),'sha256':sha((output/'labels.private.json').read_bytes())}))


if __name__=='__main__':main()
