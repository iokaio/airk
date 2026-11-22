# SPDX-License-Identifier: Apache-2.0
"""Summarize L5 phase A GPU receipts into one measurements table (plan 12.5.2).

Reads the ``l5a_gpu`` / ``l5a_cutlass`` receipts and their sampled-launch
summaries (``<receipt>.mem/summary.json``) from ``--runs`` and writes
``measurements.json`` plus a Markdown table to ``--output`` (fresh files).

Device figures per width are the CUDA free-byte drop from the receipt's
pre-load adapter observation to the observation after that width's calls.
The CUDA arena does not shrink, so each figure is the cumulative device use
up to and including that width. The process figure is the sampled maximum of
D3DKMT LOCAL current usage over the whole child lifetime (not a lifetime peak).
"""
import argparse, hashlib, json, pathlib


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def gib(value):
    return None if value is None else round(value / (1 << 30), 2)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runs', type=pathlib.Path, required=True)
    p.add_argument('--luid', default='0:13C88')
    p.add_argument('--output', type=pathlib.Path, required=True)
    args = p.parse_args()
    rows = []
    for receipt in sorted(args.runs.glob('g*.json')):
        data = json.loads(receipt.read_bytes())
        mem_dir = receipt.with_suffix('.mem')
        mem = json.loads((mem_dir / 'summary.json').read_bytes()) if (mem_dir / 'summary.json').exists() else {}
        sampled = mem.get('adapters', {}).get(args.luid, {}).get('sampled_max_current_usage_bytes', {})
        base = {'run': receipt.name, 'receipt_sha256': sha(receipt), 'complete': data.get('complete'),
                'exit_code': mem.get('exit_code'), 'math_environment': data.get('math_environment'),
                'sampled_process_local_max_bytes': sampled.get('local'),
                'sampled_process_non_local_max_bytes': sampled.get('non_local'),
                'host_peak_working_set_bytes': (data.get('host_final') or {}).get('peak_working_set_bytes'),
                'host_peak_pagefile_bytes': (data.get('host_final') or {}).get('peak_pagefile_bytes')}
        if data.get('schema') == 'airk.private.l5a-gpu/1':
            before = (data.get('adapter_before') or {}).get('free_bytes')
            threads = data.get('host_threads') or {}
            base.update({'route': f"{data['profile']['id']} ({data['step']})", 'model': data['profile']['model'],
                         'scope': 'Scorer path, unqualified probe binding', 'cold_open_us': data.get('cold_open_us'),
                         'intra_threads': threads.get('intra_threads'), 'error': (data.get('error') or {}).get('error')})
            widths = []
            focus = data['profile']['model'] == 'arbiter-focus-qwen3-reranker-06b'
            # Context: every width row. Focus: the full-width warm row (fixture pairs are single calls).
            for row in (data.get('rows', [])[:1] if focus else data.get('rows', [])):
                after = (row.get('adapter_after') or {}).get('free_bytes')
                widths.append({'tokens': row['tokens'], 'live_tokens': row['live_tokens'],
                               'first_call_us': row['first_call_us'], 'warm_median_us': row['warm_median_us'],
                               'warm_max_us': row['warm_max_us'], 'warm_samples': len(row['warm_us']),
                               'device_used_bytes': None if before is None or after is None else before - after})
            real = [r['first_call_us'] for r in data.get('rows', [])[1:]] if focus else []
            base['real_pair_calls'] = {'count': len(real), 'median_us': sorted(real)[len(real) // 2] if real else None}
        else:
            result = data.get('result') or {}
            if 'result' in result:  # Large wraps its result
                before = (result.get('before') or {}).get('free_bytes')
                timing = (result['result'].get('timing') or {})
                population = timing.get('population') or []
                cold = timing.get('cold_open_us')
            else:
                before = (result.get('before') or {}).get('free_bytes')
                population = result.get('timing') or []
                cold = result.get('cold_us')
            base.update({'route': f"{data['model']} protected graph raw session", 'model': data['model'],
                         'scope': 'raw scientific session (file-loaded weights), not the Scorer path',
                         'cold_open_us': cold, 'intra_threads': 'ORT default', 'error': (data.get('error') or {}).get('error')})
            widths = []
            for row in population:
                after = (row.get('adapter_after') or {}).get('free_bytes')
                widths.append({'tokens': row['tokens'], 'first_call_us': row['first_call_us'],
                               'warm_median_us': row['warm_median_us'], 'warm_max_us': row['warm_max_us'],
                               'warm_samples': len(row['warm_us']),
                               'device_used_bytes': None if before is None or after is None else before - after})
        base['widths'] = widths
        used = [w['device_used_bytes'] for w in widths if w.get('device_used_bytes') is not None]
        base['device_used_max_bytes'] = max(used) if used else None
        # The cold-peak derivation input: the larger of the per-width device maximum and
        # the sampled process maximum.
        peaks = [v for v in (base['device_used_max_bytes'], base['sampled_process_local_max_bytes']) if v is not None]
        base['derivation_peak_bytes'] = max(peaks) if peaks else None
        rows.append(base)
    args.output.mkdir(exist_ok=False)
    with (args.output / 'measurements.json').open('x', encoding='utf-8') as handle:
        json.dump({'schema': 'airk.private.l5a-measurements/1', 'luid': args.luid, 'rows': rows}, handle, indent=2)
        handle.write('\n')
    lines = ['| Run | Route | Tokens | First call ms | Warm median ms | Warm max ms | Device used GiB (cumulative) | Process LOCAL sampled max GiB | Cold open s | Threads | Host peak WS GiB |',
             '| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |']
    for r in rows:
        for w in r['widths'] or [{}]:
            lines.append('| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |'.format(
                r['run'], r['route'], w.get('tokens', '-'),
                round(w['first_call_us'] / 1000, 1) if w else '-',
                round(w['warm_median_us'] / 1000, 1) if w else '-',
                round(w['warm_max_us'] / 1000, 1) if w else '-',
                gib(w.get('device_used_bytes')) if w else '-',
                gib(r['sampled_process_local_max_bytes']),
                round(r['cold_open_us'] / 1e6, 2) if r.get('cold_open_us') else '-',
                r.get('intra_threads'), gib(r['host_peak_working_set_bytes'])))
    (args.output / 'measurements.md').open('x', encoding='utf-8').write('\n'.join(lines) + '\n')
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
