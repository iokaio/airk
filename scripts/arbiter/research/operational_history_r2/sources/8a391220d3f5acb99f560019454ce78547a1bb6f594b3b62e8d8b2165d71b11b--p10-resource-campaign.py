# SPDX-License-Identifier: Apache-2.0
"""Draft P10 measurement adapters; no command in this file starts native work.

The actual combined_reads split fixture remains the service/process owner. Call
these bounded observation helpers from that fixture's explicit measurement hooks,
or use the existing benchmark-service/check-service-clients owners after approval.
--write-plan freezes only measurement budgets, never a source/runtime snapshot.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import time

SCHEMA = 'airk.p10.resource-observation/1'
MAX_EVENT_BYTES = 128 << 10
MAX_EVENTS = 4096
MAX_REPORT_INPUT_BYTES = 128 << 20
MAX_CENSUS_ENTRIES = 20000


def budget_plan():
    return {
        'schema': 'airk.p10.resource-campaign-plan/1', 'draft': True,
        'native_launch_authorized_by_this_file': False,
        'working_version': '1.8.8',
        'source_and_fresh_release_pair': 'pending primary freeze/approval; no identity inferred from a file name',
        'owner': 'actual combined_reads split process fixture; candidate must be AIRK_CATALOG_CANDIDATE_BINARY from approved matching release pair',
        'scenarios': ['complete', 'cancel'],
        'scenario_contracts': {'complete': {'running_overlap_required':True},
                               'cancel': {'wait':'Accepted + catalog destination reserved; no storage/capacity/jobs while exact setup lease held',
                                          'running_overlap_required':False, 'contention_coverage':'not_applicable_metadata_only_cancel',
                                          'cleanup':'positive absence disposal after explicit Cancel acknowledgment and lease release'}},
        'trials_per_scenario': 1,
        'fixture': {'shape': 'existing combined_reads fixture, unchanged; four original files per source, three source roots', 'source_roots': 3, 'original_files_per_source': 4, 'total_original_files': 12, 'total_original_source_bytes': 621, 'entries': [{'root_alias': 0, 'path': 'api/policy.md', 'bytes': 49}, {'root_alias': 0, 'path': 'impl/policy.md', 'bytes': 50}, {'root_alias': 0, 'path': 'private/policy.md', 'bytes': 53}, {'root_alias': 0, 'path': 'api-other/policy.md', 'bytes': 55}, {'root_alias': 1, 'path': 'api/policy.md', 'bytes': 49}, {'root_alias': 1, 'path': 'impl/policy.md', 'bytes': 50}, {'root_alias': 1, 'path': 'private/policy.md', 'bytes': 53}, {'root_alias': 1, 'path': 'api-other/policy.md', 'bytes': 55}, {'root_alias': 2, 'path': 'api/policy.md', 'bytes': 49}, {'root_alias': 2, 'path': 'impl/policy.md', 'bytes': 50}, {'root_alias': 2, 'path': 'private/policy.md', 'bytes': 53}, {'root_alias': 2, 'path': 'api-other/policy.md', 'bytes': 55}], 'selected_source': 'source2/api/policy.md', 'selected_files': 1, 'remainder_files': 3, 'unrelated_query_source': 'source0/api/policy.md', 'unrelated_grant': 'new exact Project0/index0 grant with distinct derived native context; never the revoked original grant', 'model': 'existing synthetic two-dimensional vocabulary provider in happy case; no new model topology', 'require_semantic_ready': False, 'destination_watch_enabled': False},
        'capacity_choices': {'destination_growth': 256 << 20, 'destination_temporary': 256 << 20,
                             'original_rewrite': 128 << 20, 'cleanup_allowance': 64 << 20,
                             'basis': 'reuse existing passing native fixture ceilings, not observed usage or predicted savings'},
        'resources': {'basis':'reuse existing actual fixture configuration; capture full canonical pre-load Resources snapshot; no new tuning','existing_fixture_memory_bytes':4 << 30,'clients':4},
        'sampling': {'resource_interval_ms': 250, 'disk_interval_ms': 1000,
                     'maximum_events': MAX_EVENTS, 'maximum_case_ms': 600000,
                     'maximum_data_roots': 3, 'minimum_available_fixture_volume_bytes': 2 << 30, 'maximum_entries_per_disk_sample_all_roots': MAX_CENSUS_ENTRIES,
                     'maximum_event_bytes': MAX_EVENT_BYTES, 'maximum_report_input_bytes': MAX_REPORT_INPUT_BYTES,
                     'disk_roots': ['original_data', 'destination_data', 'unrelated_data'],
                     'process_counter_owner': 'benchmark-scale.py process_metrics on exact owned Popen handles; measured only for an explicit owned launcher, never summed as child-worker total',
                     'process_interval_ms': 250, 'maximum_named_process_handles': 8},
        'queries': {'clients': 4, 'baseline_rounds': 2, 'during_maximum_rounds': 30,
                    'settled_rounds': 2, 'maximum_inflight': 4, 'reply_max_bytes': 16384,
                    'query': 'boundaryneedle', 'path_prefix': 'api/',
                    'limit': 1, 'mode': 'lexical', 'attempts_per_sample': 1,
                    'basis': 'reuse benchmark-service query/wave and actual MCP Session; one session per client',
                    'complete_minimum_successes_with_split_observed_running': 4,
                    'minimum_baseline_successes':4, 'minimum_settled_successes':4,
                    'compare': 'same unchanged unrelated index/grant/publication; original fence refusals classified separately'},
        'scope_reconfirmation': {'trigger':'observed catalog revision changes OR prior scope refusal/indeterminate read failure before next planned wave, even at same revision; every settled wave forces confirmation',
                                 'writes':False,'new_grants':False,'query_replays':False,
                                 'checks':'complete native independent Grant/Project/Index/Membership items unchanged, native authenticated resolve, source hashes and successful publication/citation identity unchanged',
                                 'sessions':'reopen exactly four for next planned wave only; refused confirmation retains old sessions and records failure',
                                 'refusals':'retain exact combined_scope_unavailable/expired as scope_refusal; exact combined_read_unavailable with query_submitted:null as indeterminate_read_failure, never infer scope-only cause. Both require retryable=false query_replay_safe=false and exact unconfirmed selection; failed samples, never replayed',
                                 'latch':'reconfirm_required set on either failed category; cleared only after successful unchanged native identity plus read-only resolve and four replacement sessions',
                                 'maximum_complete_reply_bytes':65536,'maximum_event_bytes':131072,
                                 'timing':'reconfirmation separately timed; full setup and reopening included in case elapsed'},
        'cpu_observation': {'path':'canonical service NativeRead UsageReport RPC only; no LocalTools fallback',
                            'days':1,'maximum_pages':4,'page_max_bytes':20480,
                            'page_budget_basis':'existing canonical usage_page::page accepts 20..48KiB; prior16KiB was invalid request setup, not an authorization observation',
                            'maximum_complete_reply_bytes':65536,
                            'preservation':'each full RPC response plus hash, bytes and elapsed time retained before validation',
                            'failures':'native error, malformed page or record assembly failure fails measurement; never relabeled missing CPU',
                            'coverage':'exclusive destination native Index spans only; genuinely absent Index counter or unfinished pagination unknown, never zero; whole split CPU unknown'},
        'correctness_gates': ['sources unchanged by exact manifest before/after',
                              'mutations dispatched once and reconciled only by exact key',
                              'both finalization/funding acknowledgments complete before settled',
                              'unrelated query citation/hash/scope remains exact',
                              'all native errors/rejections/timeout/observer gaps retained',
                              'complete: no observed running overlap => inconclusive, never a contention pass; metadata-only cancel: contention not applicable'],
        'performance_policy': 'descriptive sampled observations only; no invented speedup or post-hoc threshold',
        'unknowns': ['true instantaneous physical disk/RSS peak between samples',
                     'filesystem allocation-unit/COW overhead and hardware physical I/O',
                     'whole-split and canceled-cleanup CPU when any native worker/provider handle lacks coverage',
                     'semantic/provider workload; live nonempty drain matrix remains separate'],
        'not_run': ['no source freeze or native launch by plan generation',
                    'hooks prepared separately as inert p10-resource-hooks.proposed.patch; require primary review/apply/compile',
                    'requires primary integration/review and approved fresh release pair'],
    }


def load_owner(repo, name):
    """Use existing owners; loading does not invoke their main or start processes."""
    path = Path(repo).resolve(strict=True) / 'scripts' / name
    spec = importlib.util.spec_from_file_location('p10_' + path.stem.replace('-', '_'), path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_new(path, value):
    with Path(path).open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2)
        output.write('\n')


def canonical_owned(owner, path):
    owner = Path(owner).resolve(strict=True)
    path = Path(os.path.abspath(path))
    assert path != owner and owner in path.parents, 'observer root must be an explicit fixture child'
    for node in (path, *path.parents):
        try:
            info = node.lstat()
        except FileNotFoundError:
            continue
        assert not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400, 'redirected observation path'
        if node == owner:
            break
    return path


def disk_sample(owner, roots):
    """Sample only named disposable data roots, never sources, user disk or contents.

    This is a non-atomic sum of logical file lengths. It is NOT physical disk
    allocation, quota usage, allocation-unit bytes or an exact high-water mark.
    Caller supplies only three previously reviewed fixture data-root aliases.
    """
    assert set(roots) <= {'original_data', 'destination_data', 'unrelated_data'} and 1 <= len(roots) <= 3
    count = 0
    observed = {}
    for alias, candidate in roots.items():
        root = canonical_owned(owner, candidate)
        logical = 0
        allocated = 0
        allocated_available = True
        files = 0
        raced = 0
        if not root.exists():
            observed[alias] = {'state': 'absent', 'logical_bytes': 0, 'files': 0, 'raced_entries': 0}
            continue
        stack = [root]
        while stack:
            directory = canonical_owned(owner, stack.pop())
            try:
                with os.scandir(directory) as entries:
                    for entry in entries:
                        count += 1
                        assert count <= MAX_CENSUS_ENTRIES, 'owned disk-sample entry budget exhausted'
                        try:
                            info = entry.stat(follow_symlinks=False)
                        except FileNotFoundError:
                            raced += 1
                            continue
                        assert not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400, 'redirected observation entry'
                        if stat.S_ISDIR(info.st_mode):
                            stack.append(Path(entry.path))
                        elif stat.S_ISREG(info.st_mode):
                            files += 1
                            logical += info.st_size
                            if hasattr(info, 'st_blocks'):
                                allocated += info.st_blocks * 512
                            else:
                                allocated_available = False
                        else:
                            raise AssertionError('unexpected data-root entry kind')
            except FileNotFoundError:
                raced += 1
        observed[alias] = {'state': 'partial' if raced else 'observed_nonatomic',
                           'logical_bytes': logical, 'files': files, 'raced_entries': raced,
                           'reported_allocated_block_bytes': allocated if allocated_available else None}
    return {'basis': 'sampled actual file lengths; st_blocks*512 where supported; not instantaneous physical peak or COW ownership' ,
            'visited_entries': count, 'roots': observed}


def observe_cached_resources(fixture):
    """Read the canonical cached Resources response without querying worker data."""
    started = time.perf_counter_ns()
    native = fixture.rpc('resources')
    assert native.get('status') == 'resources' and isinstance(native.get('observation'), dict), 'resources observation unavailable'
    return {'resources_native': native, 'observer_elapsed_us': (time.perf_counter_ns() - started) // 1000}


def observe_owned_processes(repo, processes):
    """Reuse existing counter owner on retained child handles, never discover PIDs.

    Windows handles cannot be substituted with a PID. Linux expected executable
    must be supplied. Missing service/project/provider handles keep total unknown.
    The caller, not this adapter, owns process startup, containment and shutdown.
    """
    assert 1 <= len(processes) <= 8
    scale = load_owner(repo, 'benchmark-scale.py')
    result = {}
    for alias, (process, expected_executable) in processes.items():
        assert alias in {'service', 'campaign_launcher', 'original_worker', 'destination_worker',
                         'unrelated_worker', 'helper', 'provider_1', 'provider_2'}
        result[alias] = {'process_id': process.pid, **scale.process_metrics(process, Path(expected_executable).resolve(strict=True))}
    return {'basis': 'named owned process handles only; no implicit descendant aggregation',
            'owned_process_metrics': result}


def append_event(path, event):
    assert event.get('schema') == SCHEMA and isinstance(event.get('sequence'), int)
    assert 0 <= event['sequence'] < MAX_EVENTS
    content = (json.dumps(event, separators=(',', ':'), ensure_ascii=True) + '\n').encode('utf-8')
    assert len(content) <= MAX_EVENT_BYTES
    target = Path(path)
    assert (target.stat().st_size if target.exists() else 0) + len(content) <= MAX_REPORT_INPUT_BYTES
    with target.open('ab') as output:
        output.write(content)


def summarize(repo, events):
    """Aggregate only actual observations; absent coverage remains unknown."""
    benchmark = load_owner(repo, 'benchmark-service.py')
    groups = {}
    scenario = None
    rss = []
    incomplete_rss = []
    disk = {}
    blocks = {}
    processes = {}
    cpu = []
    observer_us = 0
    reconfirmation_us = {}
    case_elapsed_us = 0
    overlap_successes = 0
    bounded_wave_successes = 0
    for event in events:
        assert event.get('schema') == SCHEMA
        if 'scenario' in event:
            assert scenario is None or scenario == event['scenario'], 'mixed scenario artifacts'
            scenario = event['scenario']
        observer_us += event.get('observer_elapsed_us', 0)
        case_elapsed_us = max(case_elapsed_us, event.get('elapsed_us', 0))
        if event.get('phase') == 'reconfirmation':
            reconfirmation_us.setdefault(event['outcome'], []).append(event['elapsed_us_separate_from_query'])
        if 'resources_native' in event:
            native = event['resources_native']
            assert native.get('status') == 'resources'
            sample = native['observation']
            value = sample.get('observed_rss_bytes')
            if value is not None:
                (rss if sample.get('telemetry_complete') is True else incomplete_rss).append(value)
        if 'disk' in event:
            for alias, sample in event['disk']['roots'].items():
                if sample['state'] == 'observed_nonatomic':
                    disk[alias] = max(disk.get(alias, 0), sample['logical_bytes'])
                    if sample.get('reported_allocated_block_bytes') is not None:
                        blocks[alias] = max(blocks.get(alias, 0), sample['reported_allocated_block_bytes'])
        if 'owned_process_metrics' in event:
            for alias, metrics in event['owned_process_metrics'].items():
                prior = processes.setdefault(alias, {'first_cpu_us': None, 'last_cpu_us': None, 'sampled_peak_rss_bytes': None, 'samples': 0, 'process_id': metrics.get('process_id'), 'identity_changed': False})
                prior['samples'] += 1
                if prior['process_id'] != metrics.get('process_id'): prior['identity_changed'] = True
                if 'cpu_us' in metrics:
                    if prior['first_cpu_us'] is None: prior['first_cpu_us'] = metrics['cpu_us']
                    prior['last_cpu_us'] = metrics['cpu_us']
                if 'peak_rss_bytes' in metrics: prior['sampled_peak_rss_bytes'] = max(prior['sampled_peak_rss_bytes'] or 0, metrics['peak_rss_bytes'])
                prior['counter_basis'] = metrics.get('counter_basis')
        if event.get('same_job_running_at_both_bounds') is True:
            bounded_wave_successes += event.get('running_wave_successes', 0)
        if 'query' in event:
            query = event['query']
            key = event['phase'] + ':' + query['outcome']
            groups.setdefault(key, []).append(query['elapsed_ms'])
            overlap_successes += int(query['outcome'] == 'success' and query.get('split_observed_running') is True)
        if 'index_resources' in event:
            coverage = event['index_resources']
            counter = coverage.get('counters', {}).get('process_cpu_us')
            cpu.append({'index_alias': event['index_alias'], 'intervals': coverage.get('intervals'),
                        'overlapped_intervals': coverage.get('overlapped_intervals'),
                        'process_cpu_us': int(counter['value']) if counter else None,
                        'counter_samples': counter['samples'] if counter else 0})
    overlap_successes = max(overlap_successes, bounded_wave_successes)
    for sample in processes.values():
        sample['observed_cpu_delta_us'] = sample['last_cpu_us'] - sample['first_cpu_us'] if not sample['identity_changed'] and sample['first_cpu_us'] is not None and sample['last_cpu_us'] is not None and sample['last_cpu_us'] >= sample['first_cpu_us'] else None
    return {'schema': 'airk.p10.resource-summary/1', 'native_qualification': False,
            'event_count': len(events), 'latency': {key: benchmark.latency(values) for key, values in groups.items()},
            'queries_success_with_split_observed_running': overlap_successes,
            'scenario': scenario,
            'contention_coverage': 'not_applicable_metadata_only_cancel' if scenario == 'cancel' else 'observed' if overlap_successes >= 4 else 'inconclusive_no_required_overlap',
            'sampled_rss_max_bytes_complete_telemetry': max(rss, default=None),
            'sampled_rss_max_bytes_incomplete_telemetry': max(incomplete_rss, default=None),
            'sampled_logical_data_root_max_bytes': disk,
            'sampled_reported_allocated_blocks_max_bytes': blocks,
            'named_owned_process_counter_coverage': processes, 'exclusive_index_cpu_samples': cpu,
            'whole_split_cpu_us': None, 'physical_disk_peak_bytes': None,
            'observer_elapsed_us': observer_us,
            'reconfirmation_elapsed_us_by_outcome':reconfirmation_us,
            'case_elapsed_us_through_last_observation':case_elapsed_us,
            'limitations': budget_plan()['unknowns'],
            'note': 'summary is measurement projection, not reduced native read contract; original complete resource responses retained in events'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--write-plan', type=Path)
    modes.add_argument('--summarize', type=Path)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parent / 'p10-worktree')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.write_plan:
        plan = budget_plan()
        plan['budget_sha256'] = hashlib.sha256(json.dumps(plan, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        write_new(args.write_plan, plan)
        return
    assert args.output is not None, '--summarize requires --output'
    assert args.summarize.stat().st_size <= MAX_REPORT_INPUT_BYTES
    events = []
    with args.summarize.open(encoding='utf-8') as source:
        for sequence, line in enumerate(source):
            assert sequence < MAX_EVENTS and len(line.encode('utf-8')) <= MAX_EVENT_BYTES
            event = json.loads(line)
            assert event['sequence'] == sequence
            events.append(event)
    write_new(args.output, summarize(args.repo, events))


if __name__ == '__main__':
    main()
