# SPDX-License-Identifier: Apache-2.0
"""Private inference-only references from immutable native tensors; no ORT execution."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import importlib.metadata
import json
import math
import os
from pathlib import Path
import re
import sys
import time
sys.dont_write_bytecode = True


def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def regular(path):
    path = Path(path)
    if not path.is_absolute() or path.resolve(strict=True) != path:
        raise ValueError('canonical absolute existing input required')
    for item in [path, *path.parents]:
        info = item.lstat()
        if item.is_symlink() or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('input alias/reparse point refused')
    if not path.is_file():
        raise ValueError('regular input required')
    return path


def pinned(pin):
    path = regular(pin['path'])
    if digest(path) != pin['sha256']:
        raise ValueError('exact input hash changed: ' + str(path))
    if 'bytes' in pin and path.stat().st_size != pin['bytes']:
        raise ValueError('exact input length changed')
    return path


def load_module(pin, name):
    path = pinned(pin)
    # These separately authenticated retained modules have pure imports only.
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def windows_sid(handle=None):
    if os.name != 'nt':
        raise ValueError('this frozen custody preparation supports Windows only')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    api = ctypes.WinDLL('advapi32', use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    api.OpenProcessToken.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
    api.GetTokenInformation.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD)]
    api.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.LPWSTR)]
    token = wintypes.HANDLE()
    if not api.OpenProcessToken(handle or kernel.GetCurrentProcess(), 8, ctypes.byref(token)):
        raise ctypes.WinError(ctypes.get_last_error())
    text = wintypes.LPWSTR()
    try:
        size = wintypes.DWORD()
        api.GetTokenInformation(token, 1, None, 0, ctypes.byref(size))
        data = ctypes.create_string_buffer(size.value)
        if not api.GetTokenInformation(token, 1, data, size, ctypes.byref(size)):
            raise ctypes.WinError(ctypes.get_last_error())
        sid = ctypes.cast(data, ctypes.POINTER(ctypes.c_void_p))[0]
        if not api.ConvertSidToStringSidW(sid, ctypes.byref(text)):
            raise ctypes.WinError(ctypes.get_last_error())
        return text.value
    finally:
        if text:
            kernel.LocalFree(ctypes.cast(text, ctypes.c_void_p))
        kernel.CloseHandle(token)


def require_supported(plan):
    if plan['kind'] not in ('same_artifact', 'original_fp32'):
        raise ValueError('unknown independent reference kind')
    if plan['kind'] == 'same_artifact' and plan['upstream'] == 'Qwen/Qwen3-Reranker-0.6B' and not plan.get('focus_adapter'):
        raise ValueError('independent Torch MatMulNBits8/block32 implementation unavailable; FP32 cannot impersonate W8')
    if plan['upstream'] not in ('Qwen/Qwen3-Reranker-0.6B', 'Qwen/Qwen3-1.7B', 'Qwen/Qwen3-4B'):
        raise ValueError('unreviewed source model')


def native_documents(plan):
    # Values and complete row/input objects are copied from Rust, never built
    # from Python Profile/CompiledInput/Asset defaults or a tokenizer mirror.
    request = json.loads(pinned(plan['native_input']).read_bytes())
    template = json.loads(pinned(plan['native_template']).read_bytes())
    profile = json.loads(pinned(request['profile']).read_bytes())
    if template['model'] != profile['model'] or template['compiler'] != profile['compiler'] or template['resource_policy'] != request['policy_digest']:
        raise ValueError('native template/profile/compiler/resource identity differs')
    original = json.loads(pinned(plan['original_roster']).read_bytes())
    if original['model'] != template['model'] or original['compiler'] != template['compiler'] or original['resource_policy_digest'] != template['resource_policy'] or original['native_input_sha256'] != plan['native_input']['sha256'] or original['publisher_model'] != plan['upstream']:
        raise ValueError('native original-source roster does not bind this exact request/model/compiler')
    assets = original['original_reference_assets']
    files = original['original_reference_files']
    if not assets or len(files) != len(assets):
        raise ValueError('Rust-emitted complete original reference asset roster required')
    for file in files.values():
        pinned(file)
    for file in original['original_source_files'].values():
        pinned(file)
    for name, path in request['files'].items():
        matches = [asset for asset in profile['assets'] if asset['name'] == name]
        if len(matches) != 1:
            raise ValueError('native file map has no exact original asset')
        pinned({'path': path, 'sha256': matches[0]['sha256'], 'bytes': matches[0]['bytes']})
    if set(request['files']) != {asset['name'] for asset in profile['assets']}:
        raise ValueError('native logical file map is incomplete')
    for path, expected in plan['input_seals'].items():
        pinned({'path': path, 'sha256': expected})
    return request, template, profile, original


def projection_binding(parameter_name):
    # Exact node names observed in the retained qualified scores-only exporter.
    # A global hash match would fail to detect a swapped projection binding.
    match = re.fullmatch(r'text\.layers\.(\d+)\.(self_attn|mlp)\.(q_proj|k_proj|v_proj|o_proj|gate_proj|up_proj|down_proj)\.weight', parameter_name)
    if not match:
        return None
    layer, family, member = match.groups()
    if (family == 'self_attn') != (member in ('q_proj', 'k_proj', 'v_proj', 'o_proj')):
        raise ValueError('unreviewed projection family')
    return '/scorer/layers.' + layer + '/' + family + '/' + member + '/MatMul'


def graph_semantics(model, directory):
    """Comparison-only whole-protobuf fingerprint, preserving every numeric tensor value.

    Payloads are replaced by their SHA solely in an in-memory comparison clone;
    this is never written, executed, accepted as a model, or used as its identity.
    """
    import copy
    import onnx
    clone = copy.deepcopy(model)
    def visit(message):
        if message.DESCRIPTOR.full_name == 'onnx.TensorProto':
            array = onnx.numpy_helper.to_array(message, base_dir=str(directory))
            if array.dtype.kind == 'O':
                raise ValueError('object/string tensor outside reviewed numeric exporter')
            value_hash = hashlib.sha256(array.tobytes(order='C')).digest()
            for field in ['raw_data', 'float_data', 'int32_data', 'string_data', 'int64_data', 'double_data', 'uint64_data', 'external_data', 'data_location']:
                message.ClearField(field)
            message.raw_data = value_hash
            return
        for field, value in message.ListFields():
            if field.message_type is None:
                continue
            if field.is_repeated:
                for child in value:
                    visit(child)
            else:
                visit(value)
    visit(clone)
    return hashlib.sha256(clone.SerializeToString(deterministic=True)).hexdigest()


GQA_SOURCE_SHA = 'c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4'
GQA_GRAPH_SHA = 'd2b32177c875a989bdbe56071168bbcabb8d589ba740099feea44972459828c3'
GQA_PROOF_SHA = '7cd056abdedffe07a4c8684de3b21744e4f877630d7b1415f3b83aa292b249f0'
GQA_TEMPLATE_SHA = '7d4a546d9017a14594d61a59eda9693a47e241da4a5dd9ef1edf4bbb0e626c4b'

def exact_gqa_context(plan, profile, graph_sha):
    binding = plan.get('compact_gqa_binding')
    if graph_sha != GQA_GRAPH_SHA and binding is None:
        return False
    if (graph_sha != GQA_GRAPH_SHA or plan['kind'] != 'same_artifact'
            or plan['upstream'] != 'Qwen/Qwen3-1.7B' or profile['model'] != 'arbiter-context-qwen3-17b'
            or not isinstance(binding, dict) or set(binding) != {'source_graph', 'shape_proof'}
            or binding['source_graph']['sha256'] != GQA_SOURCE_SHA
            or binding['shape_proof']['sha256'] != GQA_PROOF_SHA
            or plan['native_template']['sha256'] != GQA_TEMPLATE_SHA):
        raise ValueError('only exact reviewed Compact c79-to-d2 GQA/native-template binding permitted')
    return True

def exact_gqa_source(graph, graph_path, profile, plan, onnx):
    binding = plan['compact_gqa_binding']
    for value in binding.values():
        path = pinned(value)
        if plan['input_seals'].get(str(path)) != value['sha256']:
            raise ValueError('GQA source/proof omitted from owned input closure')
    source_path = pinned(binding['source_graph'])
    source = onnx.load(str(source_path), load_external_data=False)
    proof = json.loads(pinned(binding['shape_proof']).read_bytes())
    if (proof['source_sha256'] != GQA_SOURCE_SHA or proof['new_graph_sha256'] != GQA_GRAPH_SHA
            or proof['custom_shape_gate_pending'] is not False or proof['gqa_nodes'] != 28
            or proof['original_initializers_and_external_ranges_preserved_verbatim'] is not True):
        raise ValueError('actual reviewed GQA structural proof differs')
    old = {t.name: t.SerializeToString(deterministic=True) for t in source.graph.initializer}
    new = {t.name: t.SerializeToString(deterministic=True) for t in graph.graph.initializer}
    extras = {'/airk_compact_gqa/qshape', '/airk_compact_gqa/kshape', '/airk_compact_gqa/axis1', '/airk_compact_gqa/one'}
    if len(old) != 310 or len(new) != 314 or set(new) - set(old) != extras or any(new.get(k) != v for k,v in old.items()):
        raise ValueError('original TensorProto bytes/external ranges changed')
    for boundary in ['input', 'output']:
        if ([v.SerializeToString(deterministic=True) for v in getattr(source.graph,boundary)]
                != [v.SerializeToString(deterministic=True) for v in getattr(graph.graph,boundary)]):
            raise ValueError('native six-input/scores-only boundary changed')
    custom = [n for n in graph.graph.node if n.domain]
    if ({n.name for n in custom} != {'/airk_compact_gqa/'+str(i)+'/GQA' for i in range(28)}
            or len(custom) != 28 or any(n.domain != 'com.microsoft' or n.op_type != 'GroupQueryAttention' for n in custom)):
        raise ValueError('unreviewed custom-domain node; no generic domain exception')
    if any(n.op_type in ('MatMulNBits','MatMulInteger','QuantizeLinear','DequantizeLinear') for n in graph.graph.node):
        raise ValueError('quantized graph outside exact FP32 GQA route')
    # The c79 source still passes the EXISTING whole-graph/value comparison
    # against the authenticated old exporter. d2 is bound by exact bytes/proof.
    return source, source_path

def mapped_parameter_directory(tensor, request, profile, plan):
    locations = [entry.value for entry in tensor.external_data if entry.key == 'location']
    if not locations:
        return None
    if len(locations) != 1:
        raise ValueError('ambiguous external parameter location')
    location = locations[0]
    if Path(location).name != location or Path(location).is_absolute() or '..' in location:
        raise ValueError('external parameter location is not exact logical basename')
    assets = [a for a in profile['assets'] if a['name'] == location and a['role'] == 'external_weights']
    if len(assets) != 1 or location not in request['files']:
        raise ValueError('external parameter has no exact native Core asset/file map')
    path = regular(request['files'][location])
    if (path.name != location or path.stat().st_size != assets[0]['bytes']
            or plan['input_seals'].get(str(path)) != assets[0]['sha256']):
        raise ValueError('external parameter does not bind actual owned original file')
    return path.parent


def candidate_parameters(scorer, request, profile, plan):
    import numpy as np
    import onnx
    import torch
    models = [asset for asset in profile['assets'] if asset['role'] == 'model']
    if len(models) != 1:
        raise ValueError('exact candidate graph required')
    graph_path = Path(request['files'][models[0]['name']])
    graph = onnx.load(str(graph_path), load_external_data=False)
    original_graph_path = pinned(plan['original_export_graph'])
    original_graph = onnx.load(str(original_graph_path), load_external_data=False)
    gqa = exact_gqa_context(plan, profile, digest(graph_path))
    comparison, comparison_path = exact_gqa_source(graph, graph_path, profile, plan, onnx) if gqa else (graph, graph_path)
    if graph_semantics(comparison, comparison_path.parent) != graph_semantics(original_graph, original_graph_path.parent):
        raise ValueError('actual candidate source graph changed more than authenticated byte relocation/storage')
    del original_graph, comparison
    if not gqa and any(node.domain or node.op_type in ('MatMulNBits', 'MatMulInteger', 'QuantizeLinear', 'DequantizeLinear') for node in graph.graph.node):
        raise ValueError('only actual FP32 Context candidate reconstruction is implemented')
    tensors = {tensor.name: tensor for tensor in graph.graph.initializer}
    nodes = {node.name: node for node in graph.graph.node}
    if len(tensors) != len(graph.graph.initializer) or len(nodes) != len(graph.graph.node):
        raise ValueError('ambiguous candidate graph identity')
    mappings = []
    for name, parameter in scorer.named_parameters():
        route = projection_binding(name)
        transpose = route is not None
        if route is None:
            initializer = 'scorer.' + name
        else:
            node = nodes.get(route)
            if node is None or node.op_type != 'MatMul' or len(node.input) != 2:
                raise ValueError('candidate projection node changed: ' + name)
            initializer = node.input[1]
        tensor = tensors.get(initializer)
        if tensor is None or tensor.data_type != onnx.TensorProto.FLOAT:
            raise ValueError('candidate FP32 parameter missing: ' + name)
        if gqa:
            directory = mapped_parameter_directory(tensor, request, profile, plan) or graph_path.parent
        else:
            for entry in tensor.external_data:
                if entry.key == 'location':
                    location = Path(entry.value)
                    if location.name != entry.value or location.is_absolute() or (graph_path.parent / location).resolve() not in {Path(path).resolve() for path in request['files'].values()}:
                        raise ValueError('candidate external parameter escaped exact file map')
            directory = graph_path.parent
        actual = onnx.numpy_helper.to_array(tensor, base_dir=str(directory))
        source = actual.T if transpose else actual
        expected = parameter.detach().cpu().numpy()
        if source.dtype != np.float32 or source.shape != expected.shape or not np.array_equal(source, expected):
            raise ValueError('actual candidate parameter differs from authenticated original FP32: ' + name)
        # Assign the actual authenticated candidate bytes, not an FP32 label.
        with torch.inference_mode():
            parameter.copy_(torch.from_numpy(np.array(source, copy=True)))
        mappings.append({'parameter': name, 'initializer': initializer, 'node': route, 'transposed': transpose,
                         'candidate_parameter_sha256': hashlib.sha256(actual.tobytes(order='C')).hexdigest()})
        del actual, source, expected
    return mappings


def verify_loaded_runtime(plan):
    # The original manifest is sealed before launch. Recheck actual imported
    # Python/extension images; no existing pycache is read (-X fresh prefix).
    runtime = json.loads(pinned(plan['runtime_manifest']).read_bytes())
    for module in list(sys.modules.values()):
        filename = getattr(module, '__dict__', {}).get('__file__')
        if not isinstance(filename, str):
            continue
        path = Path(filename).resolve()
        if not path.is_file():
            continue
        expected = runtime['files'].get(str(path)) or plan['input_seals'].get(str(path))
        if expected is None or digest(path) != expected:
            raise ValueError('actual imported source/image outside frozen runtime closure: ' + str(path))


def execute(plan, destination):
    require_supported(plan)  # In particular W8 refuses before any Torch import.
    def no_process_or_network(event, args):
        if event in ('subprocess.Popen', 'os.system', 'os.posix_spawn', 'os.fork', 'socket.connect'):
            raise RuntimeError('reference worker cannot launch another process or use the network')
    sys.addaudithook(no_process_or_network)
    request, template, profile, original = native_documents(plan)
    if plan['kind'] == 'same_artifact':
        model_assets = [a for a in profile['assets'] if a['role'] == 'model']
        if len(model_assets) != 1:
            raise ValueError('exact candidate graph required before Torch startup')
        if exact_gqa_context(plan, profile, model_assets[0]['sha256']):
            for value in plan['compact_gqa_binding'].values():
                path = pinned(value)
                if plan['input_seals'].get(str(path)) != value['sha256']:
                    raise ValueError('GQA source/proof omitted before Torch startup')
    if windows_sid() != plan['human_sid']:
        raise ValueError('reference process is not the verified human SID')
    for package, expected in plan['versions'].items():
        if importlib.metadata.version(package) != expected:
            raise ValueError('reference dependency changed: ' + package)
    import numpy as np
    import torch
    torch.set_grad_enabled(False)
    torch.manual_seed(1908)
    torch.set_num_threads(plan['limits']['threads'])
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    torch.backends.cudnn.deterministic = True
    if not torch.cuda.is_available():
        raise ValueError('reviewed local CUDA reference unavailable; no fallback')
    free, total = torch.cuda.mem_get_info()
    if free < plan['limits']['minimum_cuda_free_bytes']:
        raise ValueError('actual CUDA free reserve is insufficient')
    properties = torch.cuda.get_device_properties(0)
    if properties.name != plan['expected_gpu_name']:
        raise ValueError('actual CUDA device differs from the reviewed local reference card')
    cuda_observation = {'device': 0, 'name': properties.name, 'capability': [properties.major, properties.minor],
        'total_bytes': properties.total_memory, 'free_bytes_before_load': free,
        'uuid': str(properties.uuid) if hasattr(properties, 'uuid') else None}
    torch.cuda.set_per_process_memory_fraction(plan['limits']['cuda_allocator_bytes'] / total, 0)
    torch.cuda.reset_peak_memory_stats()
    module = load_module(plan['reference_module'], 'approved_reference_source')
    is_focus = plan['upstream'] == 'Qwen/Qwen3-Reranker-0.6B'
    if is_focus:
        module.verify_assets()
        if plan['kind'] == 'same_artifact':
            adapter = load_module(plan['focus_adapter'], 'approved_focus_weight8_reference')
            models = [asset for asset in profile['assets'] if asset['role'] == 'model']
            if len(models) != 1:
                raise ValueError('exact selected Focus candidate graph required')
            checked = adapter.inspect_candidate(Path(request['files'][models[0]['name']]), request['files'], pinned(plan['original_export_graph']))
            official = module.load_official(torch, 'cpu')
            scorer = module.wrapper_for(torch, official)
            original_state = {name: value.detach().cpu().clone() for name, value in scorer.state_dict().items()}
            mappings = adapter.apply_candidate(torch, scorer, checked, original_state)
            del original_state, official, checked
            scorer = scorer.to('cuda').eval()
        else:
            # This is a separate process/model, never the mutated W8 backbone.
            scorer = module.load_official(torch, 'cuda')
            mappings = []
    else:
        scorer, _ = module.load_authenticated(plan['asset_root'], plan['upstream'])
        mappings = candidate_parameters(scorer, request, profile, plan) if plan['kind'] == 'same_artifact' else []
        scorer = scorer.to('cuda').eval()
    for parameter in scorer.parameters():
        parameter.requires_grad_(False)
    verify_loaded_runtime(plan)
    result = {'model': template['model'], 'compiler': template['compiler'], 'resource_policy_digest': template['resource_policy'],
              'reference_kind': plan['kind'], 'candidate_assets': [asset for asset in profile['assets'] if asset['role'] in ('model', 'external_weights', 'tokenizer')],
              'original_reference_assets': original['original_reference_assets'], 'original_reference_files': original['original_reference_files'],
              'rows': [], 'candidate_parameter_bindings': mappings, 'torch_forward_invocations': 0, 'complete': False,
              'cuda_observation': cuda_observation, 'cpu_threads': torch.get_num_threads(), 'interop_threads': torch.get_num_interop_threads()}
    if plan.get('compact_gqa_binding'):
        result['compact_gqa_binding'] = plan['compact_gqa_binding']
    destination = Path(destination)
    with destination.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
    try:
        with torch.inference_mode():
            for row in template['rows']:
                raw = row['input']
                ids = torch.tensor([raw['input_ids']], dtype=torch.int64, device='cuda')
                mask = torch.tensor([raw['attention_mask']], dtype=torch.int64, device='cuda')
                positions = torch.tensor([raw['position_ids']], dtype=torch.int64, device='cuda')
                candidates = torch.tensor([raw['candidate_positions']], dtype=torch.int64, device='cuda')
                decision = torch.tensor([raw['decision_position']], dtype=torch.int64, device='cuda')
                live = torch.tensor([raw['option_mask']], dtype=torch.bool, device='cuda')
                forward_started = time.monotonic()
                if is_focus and raw['candidate_positions'] != [module.FALSE_TOKEN, module.TRUE_TOKEN]:
                    raise ValueError('native Focus candidate rows differ from authenticated publisher readout')
                if is_focus and plan['kind'] == 'same_artifact':
                    scores = scorer(ids, mask, positions)[0]
                elif is_focus:
                    scores = scorer(input_ids=ids, attention_mask=mask, position_ids=positions, use_cache=False, return_dict=True).logits[0, raw['decision_position'], candidates[0]]
                elif plan['kind'] == 'same_artifact':
                    scores = scorer(ids, mask, positions, decision, candidates, live)[0][0]
                else:
                    hidden = scorer.text(input_ids=ids, attention_mask=mask, position_ids=positions, use_cache=False).last_hidden_state
                    scores = torch.nn.functional.linear(hidden[0, raw['decision_position']], scorer.text.embed_tokens.weight[candidates[0]])
                    scores = scores.masked_fill(~live[0], -1e4)
                torch.cuda.synchronize()
                result['torch_forward_invocations'] += 1
                observed = scores.detach().float().cpu().tolist()
                if len(observed) != len(raw['candidate_positions']) or not all(math.isfinite(value) for value in observed):
                    raise ValueError('incomplete or nonfinite original Torch score array')
                # Preserve every native input byte/value, including unknown metadata.
                result['rows'].append({'name': row['name'], 'input': raw, 'scores': observed, 'forward_elapsed_seconds': time.monotonic() - forward_started})
                destination.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        verify_loaded_runtime(plan)
        result['complete'] = True
        result['peak_cuda_allocated_bytes'] = torch.cuda.max_memory_allocated()
        result['peak_cuda_reserved_bytes'] = torch.cuda.max_memory_reserved()
    except BaseException as error:
        result['failure'] = {'type': type(error).__name__, 'message': str(error)}
        raise
    finally:
        destination.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    plan = json.loads(pinned({'path': str(args.plan), 'sha256': args.plan_sha256}).read_bytes())
    execute(plan, args.output)


if __name__ == '__main__':
    main()
