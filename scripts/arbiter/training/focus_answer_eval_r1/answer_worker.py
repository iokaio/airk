# SPDX-License-Identifier: Apache-2.0
"""Owned Focus LoRA worker for the Answer evaluation: score base, train, merge, score candidate.

Launched only by `../focus_lora_r8/launch_owned.py` with a plan from `make_plan.py`.
It reuses the pilot's LoRA, merge and full-vocabulary loss code unchanged
(`focus_lora_r8/focus_worker.py`) and the publisher loader
(`export/qwen/qwen3-reranker06-experiment-r1/run_focus_export.py`).
Modes: `probe` (a few updates, adapter discarded, timing only) and `run`.
The worker never spawns a process or opens a socket (audit hook).
"""
import argparse, hashlib, json, math, os, struct, sys, time
from pathlib import Path

import worker_math as wm

SCHEMA = 'airk.private.focus-answer-eval-owned-plan/1'
HUMAN = 'S-1-5-21-3678370870-3858642417-826968105-1002'


def digest(path):
    with Path(path).open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def pinned(pin):
    p = Path(pin['path'])
    if not p.is_absolute() or p.is_symlink() or not p.is_file() or digest(p) != pin['sha256']:
        raise ValueError('sealed input changed: ' + str(p))
    return p


def load_module(pin, name):
    import importlib.util
    p = pinned(pin)
    spec = importlib.util.spec_from_file_location(name, p)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def verify_plan(plan):
    if plan['schema'] != SCHEMA or plan['mode'] not in ('probe', 'run') or plan['human_sid'] != HUMAN:
        raise ValueError('reviewed answer-eval plan required')
    limit = plan['limits']
    if not 0 < limit['wall_seconds'] <= 6 * 3600 or limit['host_committed_bytes'] > 24 * 1024**3 or limit['cuda_allocator_bytes'] > 40 * 1024**3:
        raise ValueError('resource cap exceeded')
    if plan['mode'] == 'probe' and plan['max_optimizer_steps'] > 4:
        raise ValueError('bounded probe required')
    for key, value in plan['environment'].items():
        if os.environ.get(key) != value:
            raise ValueError('sealed environment changed: ' + key)
    import importlib.metadata
    for name, expected in plan['versions'].items():
        if importlib.metadata.version(name) != expected:
            raise ValueError('runtime version changed: ' + name)
    for path, sha in plan['input_seals'].items():
        pinned({'path': path, 'sha256': sha})


def read_rows(pin):
    return [json.loads(line) for line in pinned(pin).read_text(encoding='utf-8').splitlines() if line.strip()]


def run(plan, plan_sha):
    if os.environ.get('AIRK_F2_START_GATE') != '1':
        raise ValueError('owned pre-Torch gate required')
    print('AIRK_F2_GATE_READY:' + str(os.getpid()), flush=True)
    message = sys.stdin.readline().strip().split()
    if len(message) != 3 or message[:2] != ['GO', plan_sha]:
        raise ValueError('owned GO/source binding absent')
    deadline = float(message[2]); started = time.monotonic()

    def time_left():
        if time.monotonic() >= deadline:
            raise TimeoutError('owned deadline expired; no replay')

    verify_plan(plan); time_left()

    def audit(event, args):
        if event in ('subprocess.Popen', 'os.system', 'os.posix_spawn', 'os.fork', 'socket.connect'):
            raise RuntimeError('worker cannot spawn another process or use network')
    sys.addaudithook(audit)
    pilot = load_module(plan['pilot_worker'], 'pilot_focus_worker')
    width = plan['width']
    train = read_rows(plan['train_examples']); evaluation = read_rows(plan['eval_examples'])
    schedule = wm.schedule(train, plan['max_rows'])
    groups = wm.batches(schedule, plan['optimizer']['effective_batch'])[: plan['max_optimizer_steps']]
    import torch
    torch.manual_seed(1908); torch.set_num_threads(4); torch.set_num_interop_threads(1); torch.use_deterministic_algorithms(True)
    torch.backends.cuda.matmul.allow_tf32 = False; torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False; torch.backends.cudnn.deterministic = True
    if not torch.cuda.is_available():
        raise ValueError('approved GPU unavailable; no fallback')
    prop = torch.cuda.get_device_properties(0); free, total = torch.cuda.mem_get_info()
    gpu = {'name': prop.name, 'uuid': str(prop.uuid), 'free_bytes': free, 'total_bytes': total}
    print('AIRK_F2_GPU_METADATA:' + json.dumps(gpu, sort_keys=True), flush=True)
    if prop.name != plan['gpu']['name'] or free < plan['limits']['minimum_cuda_free_bytes']:
        raise ValueError('approved GPU or reserve differs')
    torch.cuda.set_per_process_memory_fraction(plan['limits']['cuda_allocator_bytes'] / total, 0)
    torch.cuda.reset_peak_memory_stats(); time_left()
    source = load_module(plan['reference_module'], 'reviewed_full_focus_source')
    source.verify_assets(); model = source.load_official(torch, 'cuda')
    for p in model.parameters():
        p.requires_grad_(False)
    model.eval(); time_left()
    output = Path(plan['worker_output']).resolve()
    if output.exists() or not output.is_relative_to(Path(plan['log_dir']).resolve()):
        raise ValueError('fresh owned worker output required')
    output.mkdir(); report = output / 'result.json'
    result = {'schema': 'airk.private.focus-answer-eval-worker-result/1', 'mode': plan['mode'], 'plan_sha256': plan_sha,
              'complete': False, 'gpu': gpu, 'width': width, 'scores': {}, 'updates': [], 'phases': {},
              'train_rows_scheduled': len(schedule), 'optimizer_steps_planned': len(groups), 'gradient_steps': 0,
              'qualification': False, 'promotion': False}

    def retain():
        raw = json.dumps(result, indent=1).encode()
        if len(raw) > 12 * 1024 * 1024:
            raise ValueError('bounded worker receipt exceeded')
        report.write_bytes(raw + b'\n')

    readout = [2152, 9693]

    def batch_tensors(rows):
        ids, mask = zip(*(wm.left_pad(r['live_ids'], width) for r in rows))
        t = lambda v: torch.tensor(v, dtype=torch.int64, device='cuda')
        return t(list(ids)), t(list(mask)), t([list(range(width))] * len(rows))

    def hidden_last(rows):
        input_ids, mask, positions = batch_tensors(rows)
        return model.model(input_ids=input_ids, attention_mask=mask, position_ids=positions, use_cache=False,
                           return_dict=True).last_hidden_state[:, width - 1, :]

    def score(rows, label):
        start = time.monotonic(); out = []
        with torch.inference_mode():
            head = model.lm_head.weight[readout]
            for i in range(0, len(rows), plan['score_batch']):
                time_left(); chunk = rows[i:i + plan['score_batch']]
                logits = torch.nn.functional.linear(hidden_last(chunk), head).float().cpu().tolist()
                for row, (no, yes) in zip(chunk, logits):
                    if not (math.isfinite(no) and math.isfinite(yes)):
                        raise ValueError('nonfinite score')
                    out.append([row['id'], no, yes])
        result['scores'][label] = out; result['phases'][label + '_seconds'] = time.monotonic() - start; retain()

    try:
        if plan['mode'] == 'run':
            score(evaluation, 'base')
        targets = pilot.inject_lora(torch, model); result['lora_targets'] = len(targets)
        model.gradient_checkpointing_enable(); model.enable_input_require_grads(); model.train()
        for module in model.modules():
            if isinstance(module, torch.nn.Dropout):
                module.p = 0.0
        parameters = [p for p in model.parameters() if p.requires_grad]
        if not parameters or any(p.dtype != torch.float32 for p in parameters):
            raise ValueError('exact FP32 LoRA trainable parameters required')
        opt = plan['optimizer']
        optimizer = torch.optim.AdamW(parameters, lr=opt['lr'], weight_decay=0.0)
        micro = opt['microbatch']; train_start = time.monotonic()
        for step, group in enumerate(groups, 1):
            optimizer.zero_grad(set_to_none=True); loss_sum = 0.0; step_start = time.monotonic()
            for i in range(0, len(group), micro):
                chunk = group[i:i + micro]
                logits = model.lm_head(hidden_last(chunk))
                if logits.shape != (len(chunk), model.config.vocab_size) or not torch.isfinite(logits).all():
                    raise ValueError('complete finite vocabulary required')
                target = torch.tensor([wm.target(r['label']) for r in chunk], dtype=torch.int64, device='cuda')
                loss = torch.nn.functional.cross_entropy(logits, target, reduction='sum')
                if not torch.isfinite(loss):
                    raise ValueError('nonfinite loss')
                (loss / len(group)).backward(); loss_sum += float(loss.detach().cpu()); time_left()
            if any(p.grad is None or not torch.isfinite(p.grad).all() for p in parameters):
                raise ValueError('missing/nonfinite LoRA gradients')
            optimizer.step(); torch.cuda.synchronize(); time_left()
            if any(not torch.isfinite(p).all() for p in parameters):
                raise ValueError('nonfinite LoRA parameter')
            result['gradient_steps'] += 1
            result['updates'].append({'step': step, 'rows': len(group), 'mean_ce': loss_sum / len(group),
                                      'positives': sum(r['label'] for r in group), 'seconds': time.monotonic() - step_start})
            if step % 10 == 0 or step == len(groups):
                retain()
        result['phases']['train_seconds'] = time.monotonic() - train_start
        if plan['mode'] == 'run':
            pilot.merge_lora(torch, model, targets); model.eval(); time_left()
            score(evaluation, 'candidate')
            save_start = time.monotonic()
            model.save_pretrained(output / 'merged-full-checkpoint', safe_serialization=True, max_shard_size='3GB')
            result['phases']['save_seconds'] = time.monotonic() - save_start; result['merged_full_checkpoint'] = True
        else:
            result['probe_adapter_discarded'] = True
        torch.cuda.synchronize(); time_left()
        for path, sha in plan['input_seals'].items():
            if digest(path) != sha:
                raise ValueError('consumed input changed')
        if torch.cuda.max_memory_reserved() > plan['limits']['cuda_allocator_bytes']:
            raise ValueError('CUDA peak exceeded allocator cap')
        if result['gradient_steps'] != len(groups):
            raise ValueError('incomplete training')
        result['complete'] = True
    except BaseException as error:
        result['failure'] = {'type': type(error).__name__, 'message': str(error)}
        raise
    finally:
        result['elapsed_seconds_after_GO'] = time.monotonic() - started
        result['peak_cuda_allocated_bytes'] = torch.cuda.max_memory_allocated()
        result['peak_cuda_reserved_bytes'] = torch.cuda.max_memory_reserved()
        retain()
    return result


def main():
    p = argparse.ArgumentParser(); p.add_argument('--plan', type=Path, required=True); p.add_argument('--plan-sha256', required=True)
    args = p.parse_args()
    if digest(args.plan) != args.plan_sha256:
        raise ValueError('reviewed plan differs')
    run(json.loads(args.plan.read_bytes()), args.plan_sha256)


if __name__ == '__main__':
    main()
