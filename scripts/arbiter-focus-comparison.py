# SPDX-License-Identifier: Apache-2.0
"""Owned full Focus checkpoint comparison; see arbiter-focus-comparison/CONTRACT.md."""
import argparse
import array
import gc
import hashlib
import importlib.metadata
import importlib.util
import json
import math
import os
from pathlib import Path
import struct
import sys
import time
import weakref

SCHEMA = 'airk.private.focus-comparison-plan/1'
LIMITS = {'wall_seconds': 600, 'host_committed_bytes': 16 << 30,
          'cuda_allocator_bytes': 8 << 30, 'minimum_cuda_free_bytes': 8 << 30}
CHUNK = 1 << 20


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def pinned(value):
    path = Path(value['path'])
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(), 'canonical regular sealed file required')
    for item in [path, *path.parents]:
        st = item.lstat()
        require(not item.is_symlink() and not getattr(st, 'st_file_attributes', 0) & 0x400, 'sealed input alias refused')
    require(digest(path) == value['sha256'], 'sealed input changed: ' + str(path))
    if 'bytes' in value:
        require(path.stat().st_size == value['bytes'], 'sealed input size changed')
    return path


def pin(path):
    path = Path(path).resolve(strict=True)
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': digest(path)}


def load(value, name):
    path = pinned(value)
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def strict_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def header(path, dtype, width):
    with Path(path).open('rb') as stream:
        raw = stream.read(8)
        require(len(raw) == 8, 'safetensors size prefix absent')
        size = struct.unpack('<Q', raw)[0]
        require(0 < size <= CHUNK, 'safetensors header bound')
        data = stream.read(size)
    require(len(data) == size, 'short safetensors header')
    values = json.loads(data, object_pairs_hook=strict_pairs)
    require(values.pop('__metadata__', None) == {'format': 'pt'}, 'safetensors metadata differs')
    require(len(values) == 310 and 'model.embed_tokens.weight' in values and 'lm_head.weight' not in values, 'full310 tied checkpoint roster required')
    intervals = []
    for name, value in values.items():
        require('lora' not in name.lower() and set(value) == {'dtype', 'shape', 'data_offsets'}, 'unexpected learned tensor')
        require(value['dtype'] == dtype and all(type(x) is int and x > 0 for x in value['shape']), 'checkpoint dtype/shape differs')
        start, end = value['data_offsets']
        require(type(start) is int and type(end) is int and 0 <= start < end and end - start == math.prod(value['shape']) * width, 'tensor payload size differs')
        intervals.append((start, end))
    intervals.sort()
    require(intervals[0][0] == 0 and all(a[1] == b[0] for a, b in zip(intervals, intervals[1:]))
            and intervals[-1][1] + size + 8 == Path(path).stat().st_size, 'checkpoint gap/overlap/trailing payload')
    return values, size + 8


def expand_bf16(raw):
    require(len(raw) % 2 == 0, 'partial BF16 element')
    # Exact little-endian bit expansion, including signed zero/subnormal bits.
    result = bytearray(len(raw) * 2)
    result[2::4] = raw[0::2]
    result[3::4] = raw[1::2]
    return result


def tensor_hashes(path, entries, offset, bf16, check):
    result = {}
    with Path(path).open('rb') as stream:
        for name, value in entries.items():
            check()
            start, end = value['data_offsets']
            stream.seek(offset + start)
            remaining = end - start
            hasher = hashlib.sha256()
            while remaining:
                check()
                raw = stream.read(min(CHUNK, remaining))
                require(raw, 'short learned tensor')
                hasher.update(expand_bf16(raw) if bf16 else raw)
                remaining -= len(raw)
            result[name] = hasher.hexdigest()
    check()
    return result


def targets():
    return {f'model.layers.{i}.self_attn.{name}.weight' for i in range(28)
            for name in ('q_proj', 'k_proj', 'v_proj', 'o_proj')}


def parameter_proof(original, adapted, original_hashes, adapted_hashes):
    require(set(original) == set(adapted) == set(original_hashes) == set(adapted_hashes), 'parameter subset')
    require(len(original) == 310 and all(original[k]['shape'] == adapted[k]['shape'] for k in original), 'parameter architecture differs')
    allowed = targets()
    require(len(allowed) == 112 and allowed <= set(original), 'full112 merge target roster absent')
    unchanged = set(original) - allowed
    require(len(unchanged) == 198 and all(original_hashes[k] == adapted_hashes[k] for k in unchanged), 'non-target parameter bits changed')
    return {'non_target_count': 198, 'non_target_exact': True, 'allowed_target_count': 112,
            'observed_changed_targets': sorted(k for k in allowed if original_hashes[k] != adapted_hashes[k]),
            'original_canonical_FP32': original_hashes, 'adapted_FP32': adapted_hashes}


def population(rows, instruction):
    require(instruction in ('publisher', 'software_evidence_v1'), 'unknown instruction')
    result = []
    for role, count in [('DEV', 85), ('REPLAY', 45)]:
        group = sorted((x for x in rows.values() if x['pair']['role'] == role), key=lambda x: x['pair']['id'])
        require(len(group) == count, 'complete85DEV/45replay required')
        for row in group:
            recipe = 'publisher' if role == 'REPLAY' else instruction
            require(row['pair']['rights_scope'] == 'PERMITTED_PRIVATE_RESEARCH_SCOPE' and row[recipe]['kind'] == 'compiled', 'comparison population refusal; no drop/refill')
            result.append({'id': row['pair']['id'], 'role': role, 'label': row['pair']['label'],
                           'component': row['component'], 'recipe': recipe, 'input_digest': row[recipe]['input']['input_digest']})
    require(len({x['id'] for x in result}) == 130, 'duplicate comparison ID')
    return result


def validate_result(result, expected):
    require(result['forward_attempts'] == result['forward_calls'] == 260
            and result['backward_attempts'] == result['gradient_steps'] == 0, 'incomplete/unknown comparison work')
    for arm in ('original', 'adapted'):
        actual = result['arms'][arm]
        require(len(actual['rows']) == 130, 'full comparison arm absent')
        for observed, frozen in zip(actual['rows'], expected):
            require(all(observed[k] == frozen[k] for k in frozen), 'fixed native comparison population changed')
            require(math.isfinite(observed['no_logit']) and math.isfinite(observed['yes_logit']), 'nonfinite comparison logits')
            for label in ('no', 'yes'):
                require(struct.unpack('<I', struct.pack('<f', observed[label + '_logit']))[0] == observed[label + '_bits'], 'raw logit bits changed')
        require(actual['loaded_state']['tied_pointer'] and actual['loaded_state']['matched_parameters'] == 310, 'loaded state authentication absent')
        require(actual['teardown']['model_collected'] and actual['teardown']['allocator_returned']
                and actual['teardown']['errors'] == [], 'model teardown unproved')
    require(result['parameter_proof']['non_target_exact'] and result['parameter_proof']['non_target_count'] == 198, 'parameter proof absent')


def summarize(result, metric):
    summary = {'promotion': False, 'scope': 'exploratory binary relevance; no ranking/terminal/teacher/quality claim', 'arms': {}}
    for arm in ('original', 'adapted'):
        rows = result['arms'][arm]['rows']
        dev = [r for r in rows if r['role'] == 'DEV']
        replay = [r for r in rows if r['role'] == 'REPLAY']
        components = {}
        for component in sorted({r['component'] for r in dev}):
            group = [r for r in dev if r['component'] == component]
            components[component] = metric([r['label'] for r in group], [r['yes_logit'] - r['no_logit'] for r in group])
        margins = [r['yes_logit'] - r['no_logit'] for r in replay]
        require(all(r['label'] == 1 for r in replay), 'original replay positive labels differ')
        summary['arms'][arm] = {'DEV': metric([r['label'] for r in dev], [r['yes_logit'] - r['no_logit'] for r in dev]),
                               'DEV_components': components, 'DEV_component_macro_AUROC': sum(r['auroc'] for r in components.values()) / len(components),
                               'positive_replay': {'rows': len(margins), 'mean_margin': sum(margins) / len(margins), 'fixed_zero_positive_fraction': sum(x > 0 for x in margins) / len(margins)}}
    return summary


def seal(plan, check):
    for path, expected in plan['input_seals'].items():
        check()
        pinned({'path': path, 'sha256': expected})
    check()


def prepare(args):
    # Root's authentic TRAIN pin anchors every reused private support source.
    receipt = json.loads(pinned({'path': str(args.training_receipt.resolve()), 'sha256': args.training_receipt_sha256}).read_bytes())
    start = json.loads((args.training_receipt.parent / 'start.json').read_bytes())
    base = json.loads(pinned(start['plan']).read_bytes())
    support = {Path(v['path']).name: v for v in base['execution_sources'].values() if Path(v['path']).suffix == '.py'}
    for value in support.values():
        pinned(value)
    helper = load(base['human_identity_helper'], 'comparison_human_custody')
    identity = helper.identity()
    require(identity['sid'] == base['human_sid'], 'human identity differs')
    flow = load(support['workflow.py'], 'comparison_retained_workflow')
    actual = flow.load_owned({'path': str(args.training_receipt.resolve()), 'sha256': args.training_receipt_sha256}, 'train')
    worker = actual['worker']
    require(worker['gradient_steps'] == 14 and worker['forward_calls'] == worker['backwards_completed'] == 445
            and worker['merged_full_checkpoint'] and not receipt['final_hash_failures'], 'genuine full TRAIN required')
    custody_pin = {'path': str(args.custody.resolve()), 'sha256': args.custody_sha256}
    custody = json.loads(pinned(custody_pin).read_bytes())
    require(custody['train_receipt']['sha256'] == args.training_receipt_sha256
            and custody['train_plan']['sha256'] == start['plan']['sha256'], 'checkpoint custody uses another TRAIN')
    for value in custody['files'].values():
        pinned(value)
    source = Path(base['reference_plan']['asset_root']) / 'candidate-assets/Qwen--Qwen3-Reranker-0.6B'
    original = {'path': str(source / 'model.safetensors'), 'sha256': base['input_seals'][str(source / 'model.safetensors')]}
    old_header, _ = header(pinned(original), 'BF16', 2)
    new_header, _ = header(pinned(custody['files']['model.safetensors']), 'F32', 4)
    require(set(old_header) == set(new_header) and all(old_header[k]['shape'] == new_header[k]['shape'] for k in old_header), 'checkpoint header architecture differs')
    config_path = str(source / 'config.json')
    config_equivalence(json.loads(pinned({'path': config_path, 'sha256': base['input_seals'][config_path]}).read_bytes()),
                       json.loads(pinned(custody['files']['config.json']).read_bytes()))
    logic = load(support['worker_logic.py'], 'comparison_retained_logic')
    native = json.loads(pinned(base['native_census']).read_bytes())
    rows = logic.native_rows(native, json.loads(pinned(base['cohort']).read_bytes()), json.loads(pinned(base['replay']).read_bytes()))
    selected = json.loads(pinned(base['selection']).read_bytes())
    expected = population(rows, selected['choice']['selected'])
    out = args.output.resolve()
    require(not out.exists() and not args.log_dir.exists() and out.parent.is_relative_to(Path(base['repository_root']) / 'artifacts/v190/final-plan'), 'fresh approved comparison outputs required')
    spec_path = out.parent / 'comparison-populations.json'
    require(not spec_path.exists(), 'fresh comparison population file required')
    spec_path.write_text(json.dumps(expected, indent=2) + '\n', encoding='utf-8')
    plan = json.loads(json.dumps(base))
    plan.update(schema=SCHEMA, mode='comparison', worker=pin(__file__), log_dir=str(args.log_dir.resolve()),
                worker_output=str(args.log_dir.resolve() / 'worker'), max_optimizer_steps=0, limits=dict(LIMITS),
                comparison_population=pin(spec_path), checkpoint_custody=custody_pin, checkpoint_files=custody['files'],
                original_checkpoint=original, original_directory=str(source), parent_support=support['launch_owned.py'],
                expected_instruction=selected['choice']['selected'])
    sources = dict(base['execution_sources'])
    for value in [pin(__file__), pin(Path(__file__).parent / 'arbiter-focus-comparison/CONTRACT.md'),
                  pin(Path(__file__).parent / 'tests/test_arbiter_focus_comparison.py'), pin(spec_path),
                  dict(custody_pin), pin(args.training_receipt), start['plan'], *custody['files'].values()]:
        sources[value['path']] = value
    plan['execution_sources'] = sources
    plan['input_seals'] = dict(base['input_seals'])
    plan['input_seals'].update({v['path']: v['sha256'] for v in sources.values()})
    plan['immutable_input_seals'] = dict(plan['input_seals'])
    plan['reference_plan']['input_seals'] = dict(plan['input_seals'])
    plan['environment']['PYTHONPATH'] = os.pathsep.join([str(Path(__file__).parent.resolve()), str(Path(support['launch_owned.py']['path']).parent), str(Path(base['reference_module']['path']).parent), base['reference_plan']['site_packages']])
    plan['environment']['TORCH_HOME'] = str(args.log_dir.resolve() / 'cache/torch')
    plan['environment']['CUDA_CACHE_PATH'] = str(args.log_dir.resolve() / 'cache/cuda')
    plan['continuity_key'] = flow.continuity_key(plan)
    with out.open('x', encoding='utf-8') as stream:
        json.dump(plan, stream, indent=2)
        stream.write('\n')
    print(json.dumps(pin(out)))


MATH_FIELDS = ('architectures', 'attention_bias', 'attention_dropout', 'bos_token_id', 'eos_token_id',
               'head_dim', 'hidden_act', 'hidden_size', 'initializer_range', 'intermediate_size',
               'max_position_embeddings', 'max_window_layers', 'model_type', 'num_attention_heads',
               'num_hidden_layers', 'num_key_value_heads', 'rms_norm_eps', 'sliding_window',
               'tie_word_embeddings', 'use_cache', 'use_sliding_window', 'vocab_size')


def config_math(config, strict=True):
    aliases = {'torch_dtype', 'dtype', 'transformers_version', 'rope_theta', 'rope_scaling',
               'rope_parameters', 'layer_types', 'pad_token_id'}
    if strict:
        require(set(config) <= set(MATH_FIELDS) | aliases, 'unknown checkpoint config field')
    require(all(k in config for k in MATH_FIELDS), 'complete math config required')
    result = {k: config[k] for k in MATH_FIELDS}
    require(config['model_type'] == 'qwen3' and config['num_hidden_layers'] == 28
            and config['tie_word_embeddings'] is True and config['use_sliding_window'] is False,
            'approved complete tied Qwen3 architecture required')
    rope = config.get('rope_parameters')
    if rope is None:
        require(config.get('rope_scaling') is None and 'rope_theta' in config, 'unsupported legacy RoPE')
        rope = {'rope_theta': config['rope_theta'], 'rope_type': 'default'}
    require(set(rope) == {'rope_theta', 'rope_type'} and rope['rope_type'] == 'default', 'unsupported RoPE semantics')
    result['rope_parameters'] = rope
    result['layer_types'] = config.get('layer_types', ['full_attention'] * 28)
    result['pad_token_id'] = config.get('pad_token_id')
    return result


def config_equivalence(original, adapted):
    require(original.get('torch_dtype') == 'bfloat16' and adapted.get('dtype') == 'float32', 'expected source BF16 to loaded/saved FP32 metadata required')
    left, right = config_math(original), config_math(adapted)
    require(left == right, 'weights-only comparison math configuration differs')
    return left


def accepted_execution(receipt, result, expected):
    owned = receipt['owned']
    require(receipt['disposition']['passed'] and receipt['failure'] is None and not receipt['final_hash_failures']
            and result['complete'] and not result.get('failure'), 'comparison execution nonpass')
    require(owned['exit_code'] == 0 and not owned['timed_out'] and not owned['log_oversized']
            and owned['owned_descendants_settled'] and not owned['owned_job_forced_termination']
            and owned['owned_total_processes'] == 1 and owned['owned_job_final_active_processes'] == 0
            and owned['owned_job_final_member_pids'] == [], 'actual owned process not naturally settled')
    exits = owned['retained_member_exits']
    require(len(exits) == 1 and exits[0]['exit_observed'] and exits[0]['exit_code'] == 0, 'actual original HANDLE exit absent')
    require({'worker_result', 'child_stdout_log', 'actual_execution_plan'} <= set(receipt['final_hashes']), 'mandatory final proofs absent')
    validate_result(result, expected)


def loaded_state(torch, model, entries, expected_hashes, check):
    require(model.lm_head.weight.data_ptr() == model.model.embed_tokens.weight.data_ptr(), 'actual tied head pointer differs')
    state = model.state_dict()
    require(set(state) == set(entries) | {'lm_head.weight'}, 'loaded full model roster differs')
    for name in entries:
        check()
        value = state[name]
        require(value.dtype == torch.float32 and list(value.shape) == entries[name]['shape'], 'loaded FP32 shape differs')
        flat = value.detach().reshape(-1)
        hasher = hashlib.sha256()
        for start in range(0, flat.numel(), CHUNK // 4):
            check()
            hasher.update(flat[start:start + CHUNK // 4].cpu().contiguous().numpy().tobytes())
        require(hasher.hexdigest() == expected_hashes[name], 'loaded parameter differs from authenticated checkpoint: ' + name)
    return {'tied_pointer': True, 'matched_parameters': 310}


def infer(torch, model, value, counters, check):
    check()
    counters['forward_attempts'] += 1
    started = time.monotonic()
    tensors = {k: torch.tensor([value[k]], dtype=torch.int64, device='cuda') for k in ('input_ids', 'attention_mask', 'position_ids')}
    hidden = model.model(**tensors, use_cache=False, return_dict=True).last_hidden_state[:, value['decision_position'], :]
    logits = model.lm_head(hidden)
    require(logits.shape == (1, model.config.vocab_size) and torch.isfinite(logits).all(), 'full finite vocabulary output required')
    torch.cuda.synchronize()
    check()
    no, yes = logits[0, [2152, 9693]].detach().float().cpu().tolist()
    counters['forward_calls'] += 1
    return {'no_logit': no, 'yes_logit': yes, 'no_bits': struct.unpack('<I', struct.pack('<f', no))[0],
            'yes_bits': struct.unpack('<I', struct.pack('<f', yes))[0], 'elapsed_seconds': time.monotonic() - started}


def settle_arm(torch, reference, baseline, check):
    # cuBLAS keeps process-owned workspaces beyond model lifetime. The pinned
    # Torch leak checker explicitly releases them before checking allocation.
    # Keep the original exact baseline; never subtract an unexplained residual.
    errors = []
    def observe(stage, callback):
        try:
            return callback()
        except BaseException as error:
            errors.append({'stage': stage, 'type': type(error).__name__, 'message': str(error)})
            return None
    before = observe('allocated_before_release', torch.cuda.memory_allocated)
    observe('synchronize_before_release', torch.cuda.synchronize)
    observe('collect_model', gc.collect)
    observe('clear_cublas_workspaces', lambda: torch._C._cuda_clearCublasWorkspaces())
    observe('empty_cache', torch.cuda.empty_cache)
    observe('synchronize_after_release', torch.cuda.synchronize)
    after = observe('allocated_after_release', torch.cuda.memory_allocated)
    result = {'model_observed': reference is not None,
              'model_collected': reference is not None and reference() is None,
              'allocator_returned': after == baseline,
              'baseline_allocated': baseline, 'allocated_before_release': before,
              'allocated_after': after, 'errors': errors}
    observe('original_deadline', check)
    return result


def score_arm(torch, directory, entries, hashes, expected, rows, result, check, math_config, output):
    from transformers import AutoModelForCausalLM
    baseline = torch.cuda.memory_allocated()
    model = info = reference = None
    failed = False
    try:
        model, info = AutoModelForCausalLM.from_pretrained(directory, local_files_only=True, trust_remote_code=False,
                        use_safetensors=True, dtype=torch.float32, attn_implementation='eager', output_loading_info=True)
        reference = weakref.ref(model)
        require(not any(info.get(k) for k in ('missing_keys', 'unexpected_keys', 'mismatched_keys', 'error_msgs')), 'full checkpoint load differs')
        require(config_math(model.config.to_dict(), strict=False) == math_config
                and model.config._attn_implementation == 'eager', 'actual loaded math configuration differs')
        model = model.to('cuda').eval().requires_grad_(False)
        output['loaded_state'] = loaded_state(torch, model, entries, hashes, check)
        with torch.inference_mode():
            for frozen in expected:
                check()
                native = rows[frozen['id']][frozen['recipe']]['input']
                output['rows'].append(dict(frozen, **infer(torch, model, native, result, check)))
    except BaseException as error:
        failed = True
        output['failure'] = {'type': type(error).__name__, 'message': str(error)}
        raise
    finally:
        # Drop loading metadata too; retain only scalar evidence. On failure an
        # exception traceback may still own model frames: report that honestly.
        model = info = None
        output['teardown'] = settle_arm(torch, reference, baseline, check)
        settled = (not output['teardown']['errors'] and output['teardown']['model_collected']
                   and output['teardown']['allocator_returned'])
        if not settled and not failed:
            raise ValueError('model teardown/allocator settlement unknown')
    return output


def worker(plan, sha):
    require(os.environ.get('AIRK_F2_START_GATE') == '1', 'retained owned GO gate required')
    print('AIRK_F2_GATE_READY:' + str(os.getpid()), flush=True)
    go = sys.stdin.readline().strip().split()
    require(len(go) == 3 and go[:2] == ['GO', sha], 'actual original GO/source binding absent')
    deadline = float(go[2])
    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError('original comparison deadline exhausted')
    require(plan['schema'] == SCHEMA and plan['limits'] == LIMITS and plan['mode'] == 'comparison', 'fixed comparison scope required')
    require(sys.flags.no_site == 1 and sys.dont_write_bytecode and sys.pycache_prefix == plan['environment']['PYTHONPYCACHEPREFIX']
            and not Path(sys.pycache_prefix).exists(), 'sealed no-site startup required')
    for name, value in plan['environment'].items():
        require(os.environ.get(name) == value, 'sealed environment changed')
    seal(plan, check)
    flow = load(next(v for v in plan['execution_sources'].values() if Path(v['path']).name == 'workflow.py'), 'comparison_actual_workflow')
    require(flow.continuity_key(plan) == plan['continuity_key'], 'comparison closure changed')
    reference = load(plan['reference_worker'], 'comparison_actual_reference')
    require(reference.windows_sid() == plan['human_sid'], 'actual worker SID differs')
    def audit(event, args):
        if event in ('subprocess.Popen', 'os.system', 'os.posix_spawn', 'os.fork', 'socket.connect'):
            raise RuntimeError('comparison worker cannot spawn or connect')
    sys.addaudithook(audit)
    logic = load(next(v for v in plan['execution_sources'].values() if Path(v['path']).name == 'worker_logic.py'), 'comparison_actual_logic')
    rows = logic.native_rows(json.loads(pinned(plan['native_census']).read_bytes()), json.loads(pinned(plan['cohort']).read_bytes()), json.loads(pinned(plan['replay']).read_bytes()))
    expected = population(rows, plan['expected_instruction'])
    require(expected == json.loads(pinned(plan['comparison_population']).read_bytes()), 'preregistered population changed')
    original_path = pinned(plan['original_checkpoint'])
    adapted_path = pinned(plan['checkpoint_files']['model.safetensors'])
    original, original_offset = header(original_path, 'BF16', 2)
    adapted, adapted_offset = header(adapted_path, 'F32', 4)
    original_hashes = tensor_hashes(original_path, original, original_offset, True, check)
    adapted_hashes = tensor_hashes(adapted_path, adapted, adapted_offset, False, check)
    proof = parameter_proof(original, adapted, original_hashes, adapted_hashes)
    original_config_path = str(Path(plan['original_directory']) / 'config.json')
    original_config = json.loads(pinned({'path': original_config_path, 'sha256': plan['input_seals'][original_config_path]}).read_bytes())
    adapted_config = json.loads(pinned(plan['checkpoint_files']['config.json']).read_bytes())
    math_config = config_equivalence(original_config, adapted_config)
    for name, version in plan['versions'].items():
        require(importlib.metadata.version(name) == version, 'exact runtime version differs')
    import torch
    torch.manual_seed(1908)
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.set_grad_enabled(False)
    torch.use_deterministic_algorithms(True)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    torch.backends.cudnn.deterministic = True
    require(torch.cuda.is_available(), 'approved CUDA required; no fallback')
    gpu = load(next(v for v in plan['execution_sources'].values() if Path(v['path']).name == 'gpu_identity.py'), 'comparison_actual_gpu_metadata')
    prop = torch.cuda.get_device_properties(0)
    free, total = torch.cuda.mem_get_info()
    actual_gpu = gpu.describe_gpu(prop.name, str(prop.uuid), [prop.major, prop.minor], free, total)
    print('AIRK_F2_GPU_METADATA:' + json.dumps(actual_gpu), flush=True)
    gpu.check_gpu_identity(plan['gpu'], actual_gpu, LIMITS['minimum_cuda_free_bytes'])
    torch.cuda.set_per_process_memory_fraction(LIMITS['cuda_allocator_bytes'] / total, 0)
    torch.cuda.reset_peak_memory_stats()
    check()
    output = Path(plan['worker_output'])
    require(not output.exists(), 'fresh comparison worker output required')
    output.mkdir()
    result = {'schema': 'airk.private.focus-comparison-result/1', 'complete': False, 'plan_sha256': sha,
              'arms': {}, 'parameter_proof': proof, 'forward_attempts': 0, 'forward_calls': 0,
              'backward_attempts': 0, 'gradient_steps': 0, 'qualification': False, 'promotion': False}
    try:
        for arm, directory, entries, hashes in [('original', plan['original_directory'], original, original_hashes),
                ('adapted', str(adapted_path.parent), adapted, adapted_hashes)]:
            result['arms'][arm] = {'rows': []}
            score_arm(torch, directory, entries, hashes, expected, rows, result, check, math_config, result['arms'][arm])
            reference.verify_loaded_runtime(plan['reference_plan'])
        validate_result(result, expected)
        result['summary'] = summarize(result, logic.metrics)
        require(torch.cuda.max_memory_allocated() <= LIMITS['cuda_allocator_bytes'] and torch.cuda.max_memory_reserved() <= LIMITS['cuda_allocator_bytes'], 'actual allocator cap exceeded')
        seal(plan, check)
        result['peak_cuda_allocated_bytes'] = torch.cuda.max_memory_allocated()
        result['peak_cuda_reserved_bytes'] = torch.cuda.max_memory_reserved()
        check()
        result['complete'] = True
    except BaseException as error:
        result['failure'] = {'type': type(error).__name__, 'message': str(error)}
        raise
    finally:
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--prepare', action='store_true')
    mode.add_argument('--launch', action='store_true')
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--plan-sha256')
    parser.add_argument('--training-receipt', type=Path)
    parser.add_argument('--training-receipt-sha256')
    parser.add_argument('--custody', type=Path)
    parser.add_argument('--custody-sha256')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--log-dir', type=Path)
    args = parser.parse_args()
    if args.prepare:
        prepare(args)
        return
    require(args.plan and args.plan_sha256, 'exact reviewed comparison plan required')
    plan = json.loads(pinned({'path': str(args.plan.resolve()), 'sha256': args.plan_sha256}).read_bytes())
    if args.launch:
        require(plan['schema'] == SCHEMA and plan['limits'] == LIMITS, 'fixed comparison required')
        load(plan['parent_support'], 'comparison_retained_parent').launch(args.plan, args.plan_sha256)
        receipt = json.loads((Path(plan['log_dir']) / 'execution.json').read_bytes())
        result_path = Path(plan['worker_output']) / 'result.json'
        require(digest(result_path) == receipt['worker_result_sha256'], 'actual comparison result hash differs')
        accepted_execution(receipt, json.loads(result_path.read_bytes()), json.loads(pinned(plan['comparison_population']).read_bytes()))
    else:
        worker(plan, args.plan_sha256)


if __name__ == '__main__':
    main()
