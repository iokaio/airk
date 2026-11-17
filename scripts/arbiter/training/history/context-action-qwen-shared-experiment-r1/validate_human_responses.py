# SPDX-License-Identifier: Apache-2.0
"""Validate ACTUAL human responses only. Never create labels from machine/proxy output."""
import argparse,hashlib,json
from pathlib import Path
HERE=Path(__file__).resolve().parent

def require(value,message):
    if not value:raise ValueError(message)

def assess_case(episode,response):
    require(response['id']==episode['id'] and response['native_input_digest']==episode['native_input_digest'],'human response points to another native view')
    expected={c['native_card']['id']:c for c in episode['whole_cards']};rows=response['whole_cards'];require(len(rows)==len(expected) and {r['native_card_id'] for r in rows}==set(expected),'whole card roster cannot be changed/dropped')
    if response.get('finalized_by_human') is not True:return {'semantic_gold_eligible':False,'reason':'human response not finalized'}
    require(response.get('checked_entire_model_visible_context') is True and response.get('recorded_at_UTC'),'actual visible-context review provenance required')
    for row in rows:
        require(row['symbol']==expected[row['native_card_id']]['symbol'],'native symbol/card mapping differs')
        require(row['judgment'] in ('acceptable','rejected','unjudged'),'explicit human card judgment required')
        if row['judgment']!='unjudged':require(row['checked_entire_card_statement'] is True and row.get('rationale'),'whole-statement review and rationale required')
    if any(r['judgment']=='unjudged' for r in rows):return {'semantic_gold_eligible':False,'reason':'unjudged cards retained; partial certified coverage only'}
    accepted={r['native_card_id'] for r in rows if r['judgment']=='acceptable'}
    require(type(response['no_valid_card']) is bool,'human NO_VALID_CARD decision required')
    require((not accepted)==response['no_valid_card'],'do not force a card or invent an abstention')
    aliases={v['alias'] for v in episode['model_visible_current192_units']}
    for card_id in accepted:
        kind=expected[card_id]['native_card']['action']['kind']
        if kind=='sufficient':
            require(response['shown_context_sufficient'] is True and response.get('expected_supported_answer')
                    and response.get('citation_aliases') and set(response['citation_aliases'])<=aliases,'Sufficient needs actual supported answer and shown-source citations')
            alternatives=episode.get('original_human_reference_for_reviewer_only',[])
            if alternatives and all(a['unanswerable'] for a in alternatives):return {'semantic_gold_eligible':False,'reason':'human/publisher-unanimous-unanswerable conflict needs adjudication; never relabel silently'}
        elif kind=='clarify':require(response.get('clarification_needed_description'),'unanswerability cannot become Clarify without actual specific ambiguity judgment')
        elif kind=='no_justified_read':require(response.get('all_remaining_choices_considered') is True,'NoJustifiedRead requires all actual remaining choices considered')
    return {'semantic_gold_eligible':True,'acceptable_card_ids':sorted(accepted),'no_valid_card':response['no_valid_card'],'scope':'TRAIN_PILOT_ONLY'}

def validate(queue,response,queue_sha):
    require(response['schema']=='airk.private-human-whole-card-responses/1' and response['queue_sha256']==queue_sha,'exact human queue identity required')
    require(response['scope']=='TRAIN_RUBRIC_PILOT_ONLY_NOT_CAL_OR_HELDOUT','historical TRAIN pilot cannot be renamed independent CAL/heldout')
    reviewer=response['reviewer'];require(reviewer.get('id') and reviewer.get('human') is True and reviewer.get('provenance')
        and reviewer.get('blind_to_selected_Context_model_outputs') is True,'actual identified human source/blindness required; machine proposals are not human approval')
    episodes={e['id']:e for e in queue['episodes']};seen=set();results=[]
    for case in response['cases']:
        require(case['id'] in episodes and case['id'] not in seen,'foreign/duplicate human case');seen.add(case['id'])
        results.append({'id':case['id'],**assess_case(episodes[case['id']],case)})
    return {'reviewed_cases':len(results),'semantic_gold_cases':sum(r['semantic_gold_eligible'] for r in results),'results':results,
        'independent_CAL_or_heldout_qualified':False,'label_authority':'actual human responses/provenance only; original QA projections remain separate'}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--responses',type=Path,required=True);args=p.parse_args()
    queue_path=HERE/'label-audit-r1/human-review-pilot36.private.json';raw=queue_path.read_bytes();queue=json.loads(raw)
    report=validate(queue,json.loads(args.responses.read_bytes()),hashlib.sha256(raw).hexdigest());print(json.dumps(report))
if __name__=='__main__':main()
