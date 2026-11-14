# SPDX-License-Identifier: Apache-2.0
"""Describe actual native argmax utility without inventing probability calibration."""
import argparse
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODELS = {'arbiter-small-intern-08b', 'arbiter-small-strands-2b',
          'arbiter-large-intern-4b', 'arbiter-large-kev-4b'}


def summarize_rows(transcript, scores, labels, expected_ids):
    if len(scores) != 19 or len(transcript) != 19 or len(labels) != 19:
        raise ValueError('exact nineteen calibration episodes required')
    collections = [{row['id']: row for row in values} for values in (transcript, scores, labels)]
    if any(len(rows) != 19 or set(rows) != set(expected_ids) for rows in collections):
        raise ValueError('calibration episode identity changed')
    task_rows, score_rows, label_rows = collections;rows = []
    for identifier in expected_ids:
        task, scored, label = task_rows[identifier], score_rows[identifier], label_rows[identifier]
        if scored['applied'] != task['applied']:
            raise ValueError('actual native application state changed')
        value = {'id': identifier, 'applied': scored['applied'],
                 'before_any_original_alternative_complete': label['before_any_original_alternative_complete'],
                 'has_projected_useful_choice': label['has_useful_choice']}
        if scored['applied'] is not True:
            value.update(reason=scored['reason'], selected_choice=None, unique_argmax=False,
                         projected_selected_useful=None)
        else:
            prompt = task['prompt'];aliases = prompt['aliases'];choices = {row['choice_id']: row for row in label['choices']}
            if scored['candidates'] != prompt['candidates'] or scored['input_digest'] != task['compiled']['input_digest'] or scored['input_tokens'] != len(task['compiled']['input_ids']):
                raise ValueError('actual native compiled scoring input changed')
            if len(aliases) != len(prompt['candidates']) or len(aliases) != len(choices) or {row['choice'] for row in aliases} != set(choices) or [row['alias'] for row in aliases] != [row['id'] for row in prompt['candidates']]:
                raise ValueError('native aliases differ from legal counterfactual labels')
            raw = scored['raw_scores']
            if len(raw) != len(aliases) or not 1 <= len(raw) <= 8 or any(type(value) not in (int, float) or not math.isfinite(value) for value in raw):
                raise ValueError('invalid full native trained-head scores')
            best = max(raw);winners = [index for index, score in enumerate(raw) if score == best]
            selected = aliases[winners[0]]['choice'] if len(winners) == 1 else None
            value.update(selected_choice=selected, unique_argmax=selected is not None,
                         projected_selected_useful=choices[selected]['useful'] if selected else None,
                         raw_scores=raw, aliases=aliases, input_digest=scored['input_digest'],
                         input_tokens=scored['input_tokens'], inference_us=scored['inference_us'])
        rows.append(value)
    applied = [row for row in rows if row['applied']];selected = [row for row in applied if row['unique_argmax']]
    opportunity = [row for row in rows if row['has_projected_useful_choice'] and not row['before_any_original_alternative_complete']]
    return {'episodes': 19, 'applied_episodes': len(applied), 'refused_episodes': 19-len(applied),
            'unique_argmax_episodes': len(selected), 'exact_tie_abstentions': len(applied)-len(selected),
            'selected_projected_useful_count': sum(row['projected_selected_useful'] for row in selected),
            'selected_projected_useful_fraction': sum(row['projected_selected_useful'] for row in selected)/len(selected) if selected else None,
            'missing_support_opportunity_episodes': len(opportunity),
            'missing_support_opportunity_selected_useful': sum(row.get('projected_selected_useful') is True for row in opportunity),
            'prior_complete_episodes': sum(row['before_any_original_alternative_complete'] for row in rows),
            'rows': rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('transcripts', 'labels', 'native-input', 'profile-inputs', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--scores', type=Path, action='append', required=True)
    args = parser.parse_args();output = args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts') or len(args.scores) != 4:
        parser.error('four distinct complete score receipts and fresh private output required')
    paths = [Path(__file__), Path(__file__).with_name('next-read-calibration-policy-freeze.json'),
             args.transcripts, args.labels, args.native_input, args.profile_inputs, *args.scores]
    retained = {path: path.read_bytes() for path in paths};sha = lambda raw: hashlib.sha256(raw).hexdigest()
    if len({path.resolve() for path in args.scores}) != 4 or sha(retained[args.native_input]) != 'a6149309a5838550a91cf47dd7489380509f82781550fb720fe78f8c059287b7':
        raise ValueError('wrong frozen calibration population/score artifacts')
    transcripts = json.loads(retained[args.transcripts]);labels = json.loads(retained[args.labels]);inputs = json.loads(retained[args.native_input])
    expected_ids = [row['id'] for row in inputs['questions']]
    if len(expected_ids) != 19 or len(set(expected_ids)) != 19 or transcripts['capture_sha256'] not in labels['input_seals'].values() or transcripts['profile_inputs_sha256'] != sha(retained[args.profile_inputs]):
        raise ValueError('native capture/profile/operational-label binding changed')
    entries = {row['model']: row for row in transcripts['profiles']}
    if len(transcripts['profiles']) != 4 or set(entries) != MODELS:
        raise ValueError('all four original native model slots required')
    results = {};raw_files = {}
    for path in args.scores:
        value = json.loads(retained[path]);name = value['model']
        if name not in entries or name in results or value.get('schema') != 'airk.arbiter.native-calibration-scores/1' or value.get('complete') is not True or value.get('status') != 'complete' or value.get('evaluation') is not True or value.get('release_qualified') is not False:
            raise ValueError('missing/duplicate/incomplete native model score receipt')
        entry = entries[name]
        for field in ('profile', 'profile_source_sha256', 'compiler', 'task_policy'):
            if value[field] != entry[field]:raise ValueError('native score/transcript '+field+' differs')
        if value['transcript_sha256'] != sha(retained[args.transcripts]) or value['profile_inputs_sha256'] != sha(retained[args.profile_inputs]) or value['capture_sha256'] != transcripts['capture_sha256'] or not value.get('execution_identity') or not value.get('binary_sha256'):
            raise ValueError('native scored artifact/runtime execution binding missing')
        results[name] = summarize_rows(entry['rows'], value['rows'], labels['rows'], expected_ids)
        raw_files[name] = {'path': str(path), 'sha256': sha(retained[path]),
                           'native_identity': {key: value[key] for key in ('profile', 'profile_source_sha256', 'compiler', 'task_policy', 'runtime', 'binary_sha256', 'execution_identity', 'load_us')}}
    if any(path.read_bytes() != raw for path, raw in retained.items()):raise ValueError('calibration summary source/input drift')
    output.mkdir(parents=True)
    report = {'schema': 'airk.private-native-empirical-read-utility/1', 'evaluation': True,
              'release_qualified': False, 'probability_calibration_claim': False,
              'capture_read_population': labels['capture_read_population'],
              'capture_scope': labels['capture_scope'],
              'algorithm': 'Native unique finite argmax; exact ties abstain. No temperature or confidence threshold consumed.',
              'input_seals': {str(path): sha(raw) for path, raw in retained.items()},
              'models': results, 'native_artifacts': raw_files,
              'scope': 'Nineteen frozen calibration-only operational projections, not human AIRK action/authority/gap/sufficiency labels. Unannotated text is not declared irrelevant. Small sample; no final goal quality, resource, transport, product-selectability or release qualification. Rust owner must emit any distributable calibration/Profile asset; this private analysis is not a native DTO.'}
    (output/'empirical.private.json').write_text(json.dumps(report, indent=2)+'\n')
    (output/'runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({'models': {name: {key: value for key, value in result.items() if key != 'rows'} for name, result in results.items()}}))


if __name__ == '__main__':main()
