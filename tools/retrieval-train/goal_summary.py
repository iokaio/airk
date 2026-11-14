# SPDX-License-Identifier: Apache-2.0
"""Paired fixed-population goal accounting; never select only successful trials."""
from fractions import Fraction

BASELINES = ('ordinary_followup', 'deterministic_packet')


def add_descriptive_intervals(result, conditions, tasks):
    """Original source components are sampling groups, never evidence unions."""
    import numpy as np
    from grade_native_retrieval import cluster_interval
    task_map = {row['id']: row for row in tasks}
    for name, rows in conditions.items():
        groups = [task_map[row['task_id']]['bootstrap_source_component'] for row in rows]
        result['conditions'][name]['descriptive_completion_95_interval'] = cluster_interval([int(row['success']) for row in rows], groups)
        result['conditions'][name]['descriptive_all_task_turns_95_interval'] = cluster_interval([row['external_model_turns'] for row in rows], groups)
    for comparison in result['paired_common_success']:
        identifiers = comparison['common_success_ids']
        if not identifiers:
            comparison['descriptive_turn_reduction_95_interval'] = None
            continue
        candidate = {row['task_id']: row for row in conditions[comparison['candidate']]}
        baseline = {row['task_id']: row for row in conditions[comparison['baseline']]}
        groups = sorted({task_map[key]['bootstrap_source_component'] for key in identifiers})
        numerator = np.asarray([sum(baseline[key]['external_model_turns'] - candidate[key]['external_model_turns'] for key in identifiers if task_map[key]['bootstrap_source_component'] == group) for group in groups], dtype=np.float64)
        denominator = np.asarray([sum(baseline[key]['external_model_turns'] for key in identifiers if task_map[key]['bootstrap_source_component'] == group) for group in groups], dtype=np.float64)
        draws = np.random.default_rng(1908).integers(0, len(groups), size=(10000, len(groups)))
        denominators = denominator[draws].sum(axis=1)
        # A zero-turn completed baseline is retained as an undefined denominator,
        # rather than silently dropped to improve the descriptive interval.
        comparison['descriptive_turn_reduction_95_interval'] = [float(value) for value in np.quantile(numerator[draws].sum(axis=1) / denominators, [.025, .975])] if np.all(denominators > 0) else None
    for comparison in result['critical_evidence_comparisons']:
        groups=[task_map[row['id']]['bootstrap_source_component'] for row in comparison['per_question']]
        comparison['descriptive_shown_recall_delta_95_interval']=cluster_interval([float(Fraction(*row['shown_recall_delta_fraction'])) for row in comparison['per_question']],groups)
        comparison['descriptive_cited_recall_delta_95_interval']=cluster_interval([float(Fraction(*row['cited_recall_delta_fraction'])) for row in comparison['per_question']],groups)
    result['descriptive_interval_policy'] = 'Original source-component clustered bootstrap,10000 draws/seed1908,2.5/97.5 percentiles. No confidence-bound acceptance claim.'
    return result


def summarize(conditions, expected_ids, arbiter_conditions):
    if len(expected_ids) != 100 or len(set(expected_ids)) != 100:
        raise ValueError('exact frozen 100 goal IDs required')
    names = set(BASELINES) | set(arbiter_conditions)
    if len(arbiter_conditions) != 4 or len(names) != 6 or set(conditions) != names:
        raise ValueError('two baseline and four distinct Arbiter conditions required')
    normalized = {}; summaries = {}
    for name, rows in conditions.items():
        if len(rows) != 100 or {row['task_id'] for row in rows} != set(expected_ids):
            raise ValueError('condition population incomplete or duplicated')
        values = {row['task_id']: row for row in rows}; normalized[name] = values
        for row in rows:
            if type(row.get('success')) is not bool or type(row.get('external_model_turns')) is not int or not 0 <= row['external_model_turns'] <= 8:
                raise ValueError('invalid trial success/turn accounting')
            if type(row.get('citation_identity_errors')) is not int or row['citation_identity_errors'] < 0:
                raise ValueError('invalid citation accounting')
        count = sum(row['success'] for row in rows)
        summaries[name] = {'questions': 100, 'success_count': count,
                           'completion': count / 100, 'completion_floor_pass': count >= 75,
                           'all_task_mean_external_turns': sum(row['external_model_turns'] for row in rows) / 100,
                           'failed_task_mean_external_turns': (sum(row['external_model_turns'] for row in rows if not row['success']) / (100 - count)) if count < 100 else None,
                           'failures': [{'task_id': row['task_id'], 'failure': row.get('failure'), 'external_model_turns': row['external_model_turns']} for row in rows if not row['success']],
                           'citation_identity_errors': sum(row['citation_identity_errors'] for row in rows)}
        work = {}
        for field in ('canonical_calls', 'canonical_returned_bytes', 'native_rpc_attempts',
                      'native_rpc_returned_bytes', 'canonical_seconds', 'external_inference_seconds',
                      'preprocessing_seconds', 'wall_seconds', 'client_initialization_seconds'):
            known = [row[field] for row in rows if type(row.get(field)) in (int, float)]
            work[field] = {'observed_tasks': len(known), 'unreported_tasks': 100-len(known),
                           'observed_sum': sum(known), 'all_task_mean': sum(known)/100 if len(known)==100 else None}
        native = {field: [] for field in ('queue_us', 'compile_us', 'inference_us', 'policy_us')}
        packet_count = 0;management_commands = 0
        for row in rows:
            for call in row.get('all_native_calls', []):
                management_commands += call.get('identity_management_cli_invocations', 0)
                if call.get('tool') != 'answer':continue
                packet_count += 1
                value = call.get('full_canonical_value')
                for field in native:
                    observed = value.get('work', {}).get(field) if isinstance(value, dict) else None
                    if type(observed) in (int, float):native[field].append(observed)
        work['reported_native_packet_timings'] = {field: {'observed_packets': len(values),
            'unreported_packets': packet_count-len(values), 'observed_sum_us': sum(values)} for field, values in native.items()}
        work['identity_management_cli_invocations'] = management_commands
        work['scope'] = 'Reported native Work timings and explicit CLI management observations. Missing fields remain unreported; observed zero is distinct. All source/internal read calls, tool transport bytes and external inference/preprocessing/wall are separate.'
        summaries[name]['work_accounting'] = work
        if all('human_evidence' in row and 'agent_final_citation_references' in row and 'agent_final_valid_citation_references' in row for row in rows):
            shown = sum((Fraction(*row['human_evidence']['shown_recall_fraction']) for row in rows), Fraction(0))/100
            cited = sum((Fraction(*row['human_evidence']['cited_recall_fraction']) for row in rows), Fraction(0))/100
            references = sum(row['agent_final_citation_references'] for row in rows)
            valid = sum(row['agent_final_valid_citation_references'] for row in rows)
            if not 0 <= valid <= references:raise ValueError('invalid final citation reference accounting')
            summaries[name]['critical_evidence'] = {'shown_mean_recall':float(shown),'shown_mean_recall_fraction':[shown.numerator,shown.denominator],
                'cited_mean_recall':float(cited),'cited_mean_recall_fraction':[cited.numerator,cited.denominator],
                'agent_valid_references':valid,'agent_invalid_references':references-valid,'agent_total_references':references,
                'agent_citation_precision':valid/references if references else None,
                'zero_citation_tasks':sum(row['agent_final_citation_references']==0 for row in rows)}
    strongest_count = max(summaries[name]['success_count'] for name in BASELINES)
    # Baseline-only selection frozen by primary before any A6 final scores.
    # Mean over each baseline's own completed tasks; no optional outcome enters.
    def baseline_order(name):
        completed = [row for row in conditions[name] if row['success']]
        mean = Fraction(sum(row['external_model_turns'] for row in completed), len(completed)) if completed else Fraction(9)
        return (-len(completed), mean, 0 if name == 'deterministic_packet' else 1)
    selected_baseline = min(BASELINES, key=baseline_order)
    comparisons = []
    critical_comparisons = []
    for name in arbiter_conditions:
        for baseline in BASELINES:
            shared = [key for key in expected_ids if normalized[name][key]['success'] and normalized[baseline][key]['success']]
            candidate_turns = sum(normalized[name][key]['external_model_turns'] for key in shared)
            baseline_turns = sum(normalized[baseline][key]['external_model_turns'] for key in shared)
            reduction = Fraction(baseline_turns - candidate_turns, baseline_turns) if baseline_turns else None
            comparisons.append({'candidate': name, 'baseline': baseline, 'common_success_questions': len(shared),
                                'common_success_ids': shared, 'candidate_turns': candidate_turns,
                                'baseline_turns': baseline_turns,
                                'turn_reduction': float(reduction) if reduction is not None else None,
                                'turn_reduction_fraction': [reduction.numerator, reduction.denominator] if reduction is not None else None,
                                'turn_floor_pass': reduction is not None and reduction >= Fraction(1, 10),
                                'agent_invalid_reference_count_increased': summaries[name]['citation_identity_errors'] > summaries[baseline]['citation_identity_errors']})
        summaries[name]['within_two_points_of_strongest_baseline'] = summaries[name]['success_count'] >= strongest_count - 2
        candidate = summaries[name].get('critical_evidence');baseline = summaries[selected_baseline].get('critical_evidence')
        if candidate is not None and baseline is not None:
            shown_delta = Fraction(*candidate['shown_mean_recall_fraction'])-Fraction(*baseline['shown_mean_recall_fraction'])
            cited_delta = Fraction(*candidate['cited_mean_recall_fraction'])-Fraction(*baseline['cited_mean_recall_fraction'])
            precision_pass = Fraction(candidate['agent_valid_references'],candidate['agent_total_references']) >= Fraction(baseline['agent_valid_references'],baseline['agent_total_references']) if candidate['agent_total_references'] and baseline['agent_total_references'] else None
            per_question=[]
            for key in expected_ids:
                current,control = normalized[name][key],normalized[selected_baseline][key]
                a=Fraction(*current['human_evidence']['shown_recall_fraction'])-Fraction(*control['human_evidence']['shown_recall_fraction'])
                b=Fraction(*current['human_evidence']['cited_recall_fraction'])-Fraction(*control['human_evidence']['cited_recall_fraction'])
                per_question.append({'id':key,'dataset':current.get('dataset'),'original_question_categories':current.get('original_question_categories'),
                    'original_human_disagreement':current.get('original_human_disagreement'),'shown_recall_delta_fraction':[a.numerator,a.denominator],
                    'cited_recall_delta_fraction':[b.numerator,b.denominator],'candidate_failure':current.get('failure'),'baseline_failure':control.get('failure')})
            critical_comparisons.append({'candidate':name,'baseline':selected_baseline,'shown_mean_recall_delta':float(shown_delta),
                'cited_mean_recall_delta':float(cited_delta),'shown_critical_evidence_floor_pass':shown_delta>=0,
                'cited_critical_evidence_floor_pass':cited_delta>=0,'agent_citation_precision_no_regression':precision_pass,
                'citation_comparison_state':'observed' if precision_pass is not None else 'not_estimable_zero_references',
                'shown_per_question_losses':sum(Fraction(*row['shown_recall_delta_fraction'])<0 for row in per_question),
                'shown_per_question_gains':sum(Fraction(*row['shown_recall_delta_fraction'])>0 for row in per_question),
                'cited_per_question_losses':sum(Fraction(*row['cited_recall_delta_fraction'])<0 for row in per_question),
                'cited_per_question_gains':sum(Fraction(*row['cited_recall_delta_fraction'])>0 for row in per_question),'per_question':per_question})
    return {'conditions': summaries, 'paired_common_success': comparisons,
            'critical_evidence_comparisons':critical_comparisons,
            'selected_strongest_baseline': selected_baseline,
            'all_optional_completion_floors_pass': all(summaries[name]['completion_floor_pass'] for name in arbiter_conditions),
            'all_candidate_completion_comparisons_pass': all(summaries[name]['within_two_points_of_strongest_baseline'] for name in arbiter_conditions),
            'primary_turn_floor_pass': all(row['turn_floor_pass'] for row in comparisons if row['baseline'] == selected_baseline),
            'all_optional_agent_citation_precision_comparisons_pass': (all(row['agent_citation_precision_no_regression'] is True for row in critical_comparisons) if len(critical_comparisons)==4 else None),
            'scope': 'Exact fixed-population descriptive accounting only. Both controls reported; no success-only all-task mean. These metrics alone do not establish source/resource/critical-evidence/noninterference/native transport or release qualification.'}
