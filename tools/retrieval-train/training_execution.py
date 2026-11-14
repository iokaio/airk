# SPDX-License-Identifier: Apache-2.0
"""One reviewed local training process; bounded ownership, never auto-retry."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def digest(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1048576), b''):
            result.update(block)
    return result.hexdigest()


def runtime_paths(root):
    root = Path(root).resolve()
    result = set()
    stack = [str(root)]
    while stack:
        with os.scandir(stack.pop()) as entries:
            for entry in entries:
                if entry.name == '__pycache__':
                    continue
                metadata = entry.stat(follow_symlinks=False)
                if entry.is_symlink() or getattr(metadata, 'st_file_attributes', 0) & 0x400:
                    raise ValueError('runtime alias/reparse point refused')
                if entry.is_dir(follow_symlinks=False):
                    stack.append(entry.path)
                elif entry.is_file(follow_symlinks=False):
                    if Path(entry.name).suffix not in {'.pyc', '.pyo'}:
                        result.add(os.path.normcase(entry.path))
                else:
                    raise ValueError('runtime special file refused')
    return result


def verify_seal(path, expected, wait_for_gate=False):
    if digest(path) != expected:
        raise ValueError('reviewed training execution seal changed')
    value = json.loads(Path(path).read_text(encoding='utf-8'))
    if value.get('schema') != 'airk.private-minilm-training-execution-seal/1':
        raise ValueError('wrong execution seal schema')
    if wait_for_gate:
        if os.environ.get('AIRK_TRAIN_START_GATE') != '1':
            raise ValueError('owned bounded training gate missing')
        print('AIRK_TRAIN_GATE_READY:' + str(os.getpid()), flush=True)
        if sys.stdin.readline().strip() != 'GO ' + expected:
            raise ValueError('owned bounded training gate missing')
    for key, required in value['environment'].items():
        if os.environ.get(key) != required:
            raise ValueError('training environment changed: ' + key)
    if any(Path(value['environment']['PYTHONPYCACHEPREFIX']).glob('**/*')):
        raise ValueError('unexpected executable bytecode cache')
    required = {'base_weights': '53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db',
                'tokenizer': 'be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037',
                'corpus': '6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f',
                'requests': '916b398de0fc9774041fe174d0e585c862bc68e9aa18fa04028c3126f693a18f',
                'master315': '314825dccad822874d3405305df789317c8e90a0a0e4cdcdb590acd27a555c48',
                'candidate_protocol': 'ab86b6cfd996d8f19b6c1891d275c85bee74f92d62efb234db251d7c45218469'}
    indexed = {os.path.normcase(r['path']): r['sha256'] for r in value['files']}
    for name, expected_hash in required.items():
        role = value['file_roles'][name]
        if role['sha256'] != expected_hash or indexed.get(os.path.normcase(str(Path(role['path']).resolve()))) != expected_hash:
            raise ValueError('missing/wrong mandatory training input: ' + name)
    for tree in value['runtime_trees']:
        root = Path(tree).resolve()
        actual = runtime_paths(root)
        expected_paths = {p for p in indexed if Path(p).is_relative_to(root)}
        if actual != expected_paths:
            raise ValueError('runtime source closure changed: ' + str(root))
    for item in value['files']:
        if digest(item['path']) != item['sha256']:
            raise ValueError('training source/runtime/input drift: ' + item['path'])
    if not {'torch', 'transformers', 'safetensors', 'tokenizers', 'numpy'} <= set(value['packages']):
        raise ValueError('mandatory actual training package versions missing')
    if {name: importlib.metadata.version(name) for name in value['packages']} != value['packages']:
        raise ValueError('actual training package versions changed')
    if sys.version != value['python_version']:
        raise ValueError('actual Python runtime changed')
    return value


def verify_torch_runtime(torch, path):
    value = json.loads(Path(path).read_text(encoding='utf-8'))
    if str(torch.__version__) != value['torch']['version'] or str(torch.version.cuda) != value['torch']['cuda'] or torch.backends.cudnn.version() != value['torch']['cudnn']:
        raise ValueError('actual Torch/CUDA/cuDNN versions changed')
    props = torch.cuda.get_device_properties(0)
    if props.name != value['gpu']['name'] or [props.major, props.minor] != value['gpu']['compute_capability']:
        raise ValueError('actual training GPU changed')
    # Bounds allocator use only; driver/context overhead is reported separately.
    torch.cuda.set_per_process_memory_fraction(value['limits']['cuda_allocator_bytes'] / props.total_memory, 0)



def gpu_observation(executable):
    result = subprocess.run([executable, '--query-gpu=index,uuid,name,driver_version,memory.total,memory.used,memory.free,compute_cap',
                             '--format=csv,noheader,nounits'], capture_output=True, text=True, check=True, timeout=10)
    fields = [p.strip() for p in result.stdout.strip().split(',')]
    if len(fields) != 8 or fields[0] != '0':
        raise ValueError('one explicitly reviewed GPU0 required')
    return {'index': 0, 'uuid': fields[1], 'name': fields[2], 'driver_version': fields[3],
            'total_mib': int(fields[4]), 'used_mib': int(fields[5]), 'free_mib': int(fields[6]),
            'compute_capability': [int(p) for p in fields[7].split('.')]}


def run_windows_owned_training(argv, cwd, output, timeout, environment, input_file, host_bytes, *, before_resume=None, after_resume=None, cleanup_seconds=5):
    """Drain actual owned members within one five-second cleanup deadline.

    Leader exit alone is not job settlement. Retain native member handles for
    event waits, then require fresh empty PID and accounting observations. A
    forced or unobserved cleanup remains a failed qualification; emergency
    finally cleanup never upgrades that result.
    """
    from ctypes import wintypes as w
    import contextlib
    whole_deadline = time.monotonic() + timeout
    result = None
    gate_handles = []
    retained_pid_handles = {}
    cleanup_end = whole_deadline
    gate_proof = {}
    suspend_proof = {}
    import msvcrt

    class Startup(ctypes.Structure):
        _fields_ = [("cb", w.DWORD), ("reserved", w.LPWSTR), ("desktop", w.LPWSTR),
                    ("title", w.LPWSTR), ("x", w.DWORD), ("y", w.DWORD), ("xsize", w.DWORD),
                    ("ysize", w.DWORD), ("xchars", w.DWORD), ("ychars", w.DWORD),
                    ("fill", w.DWORD), ("flags", w.DWORD), ("show", w.WORD), ("reserved2", w.WORD),
                    ("reserved_bytes", ctypes.c_void_p), ("stdin", w.HANDLE), ("stdout", w.HANDLE), ("stderr", w.HANDLE)]
    class ExtendedStartup(ctypes.Structure):
        _fields_ = [("startup", Startup), ("attributes", ctypes.c_void_p)]
    class ProcessInfo(ctypes.Structure):
        _fields_ = [("process", w.HANDLE), ("thread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]
    class Limits(ctypes.Structure):
        _fields_ = [("process_time", ctypes.c_longlong), ("job_time", ctypes.c_longlong),
                    ("flags", w.DWORD), ("min_working", ctypes.c_size_t), ("max_working", ctypes.c_size_t),
                    ("active_limit", w.DWORD), ("affinity", ctypes.c_size_t), ("priority", w.DWORD), ("scheduling", w.DWORD)]
    class IO(ctypes.Structure):
        _fields_ = [(name, ctypes.c_ulonglong) for name in ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]
    class ExtendedLimits(ctypes.Structure):
        _fields_ = [("basic", Limits), ("io", IO), ("process_memory", ctypes.c_size_t), ("job_memory", ctypes.c_size_t),
                    ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t)]
    class JobAccounting(ctypes.Structure):
        _fields_ = [(name, ctypes.c_longlong) for name in ("user", "kernel", "period_user", "period_kernel")] + [(name, w.DWORD) for name in ("faults", "total", "active", "terminated")]

    api = ctypes.WinDLL("kernel32", use_last_error=True)
    signatures = {
        "CreateJobObjectW": ([ctypes.c_void_p, w.LPCWSTR], w.HANDLE),
        "SetInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD], w.BOOL),
        "AssignProcessToJobObject": ([w.HANDLE, w.HANDLE], w.BOOL),
        "QueryInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p], w.BOOL),
        "TerminateJobObject": ([w.HANDLE, w.UINT], w.BOOL),
        "TerminateProcess": ([w.HANDLE, w.UINT], w.BOOL),
        "CloseHandle": ([w.HANDLE], w.BOOL),
        "CreateProcessW": ([w.LPCWSTR, w.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, w.BOOL, w.DWORD, ctypes.c_void_p, w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
        "WaitForSingleObject": ([w.HANDLE, w.DWORD], w.DWORD),
        "GetExitCodeProcess": ([w.HANDLE, ctypes.POINTER(w.DWORD)], w.BOOL),
        "OpenProcess": ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
        "IsProcessInJob": ([w.HANDLE, w.HANDLE, ctypes.POINTER(w.BOOL)], w.BOOL),
        "ResumeThread": ([w.HANDLE], w.DWORD),
        "GetConsoleCP": ([], w.UINT),
        "GetCurrentProcess": ([], w.HANDLE),
        "QueryFullProcessImageNameW": ([w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)], w.BOOL),
        "InitializeProcThreadAttributeList": ([ctypes.c_void_p, w.DWORD, w.DWORD, ctypes.POINTER(ctypes.c_size_t)], w.BOOL),
        "UpdateProcThreadAttribute": ([ctypes.c_void_p, w.DWORD, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
        "DeleteProcThreadAttributeList": ([ctypes.c_void_p], None),
    }
    for name, (arguments, return_type) in signatures.items():
        getattr(api, name).argtypes = arguments
        getattr(api, name).restype = return_type
    def checked(value):
        if not value:
            raise ctypes.WinError(ctypes.get_last_error())
        return value
    parent_console_cp = int(api.GetConsoleCP())
    if parent_console_cp == 0:
        raise ValueError("existing parent console required; never allocate an extra owned conhost")
    job = checked(api.CreateJobObjectW(None, None))
    info = ProcessInfo()
    assigned = resumed = False
    attributes_initialized = False
    emergency_members = []
    try:
        limits = ExtendedLimits()
        limits.basic.flags = 0x2000 | 0x100 | 0x200 | 0x8
        limits.basic.active_limit = 2  # Pinned venv shim + base interpreter, never third process.
        limits.process_memory = host_bytes
        limits.job_memory = host_bytes
        checked(api.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)))
        accounting = JobAccounting()
        def job_members(deadline=None):
            deadline = cleanup_end if deadline is None else deadline
            capacity = 16
            while True:
                class Members(ctypes.Structure):
                    _fields_ = [("assigned", w.DWORD), ("listed", w.DWORD),
                                ("pids", ctypes.c_size_t * capacity)]
                members = Members()
                observed = api.QueryInformationJobObject(job, 3, ctypes.byref(members), ctypes.sizeof(members), None)
                if observed:
                    if members.listed > capacity:
                        raise ValueError("Owned job PID census was truncated")
                    return [int(members.pids[index]) for index in range(members.listed)]
                if ctypes.get_last_error() != 234:  # ERROR_MORE_DATA: grow, never truncate.
                    raise ctypes.WinError(ctypes.get_last_error())
                if time.monotonic() >= deadline:
                    raise ValueError("Owned job PID census exceeded cleanup deadline")
                capacity = max(capacity * 2, int(members.assigned))

        def observe_job():
            members = job_members()
            checked(api.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None))
            return members, int(accounting.active)

        def retain_owned_members(members):
            retained = []
            try:
                for pid in members:
                    handle = api.OpenProcess(0x100000 | 0x1000, False, pid)
                    if not handle:
                        if ctypes.get_last_error() == 87:
                            continue  # Exited before open; recensus the job.
                        raise ctypes.WinError(ctypes.get_last_error())
                    belongs = w.BOOL()
                    try:
                        checked(api.IsProcessInJob(handle, job, ctypes.byref(belongs)))
                    except BaseException:
                        api.CloseHandle(handle)
                        raise
                    if belongs.value:
                        retained.append(handle)
                        retained_pid_handles[handle] = pid
                    else:
                        api.CloseHandle(handle)  # A reused foreign PID is never waited on.
                return retained
            except BaseException:
                for handle in retained:
                    api.CloseHandle(handle)
                raise

        # Inherit exactly the output and owned-input handles, not the runner's
        # account/token/pipe handles. Parent is never assigned to this job.
        with contextlib.nullcontext(input_file):
            handles = (w.HANDLE * 2)(msvcrt.get_osfhandle(input_file.fileno()), msvcrt.get_osfhandle(output.fileno()))
            for handle in handles:
                os.set_handle_inheritable(handle, True)
            size = ctypes.c_size_t()
            api.InitializeProcThreadAttributeList(None, 1, 0, ctypes.byref(size))
            buffer = ctypes.create_string_buffer(size.value)
            checked(api.InitializeProcThreadAttributeList(buffer, 1, 0, ctypes.byref(size)))
            attributes_initialized = True
            checked(api.UpdateProcThreadAttribute(buffer, 0, 0x20002, handles, ctypes.sizeof(handles), None, None))
            startup = ExtendedStartup()
            startup.startup.cb = ctypes.sizeof(startup)
            startup.startup.flags = 0x100  # STARTF_USESTDHANDLES
            startup.startup.stdin, startup.startup.stdout = handles[0], handles[1]
            startup.startup.stderr = handles[1]
            startup.attributes = ctypes.cast(buffer, ctypes.c_void_p)
            environment_block = None
            if environment is not None:
                environment_block = ctypes.create_unicode_buffer("\0".join(key + "=" + value for key, value in sorted(environment.items(), key=lambda row: row[0].upper())) + "\0\0")
            command = ctypes.create_unicode_buffer(subprocess.list2cmdline(argv))
            try:
                checked(api.CreateProcessW(None, command, None, None, True,
                        0x4 | 0x200 | 0x80000 | 0x400, environment_block, str(cwd), ctypes.byref(startup), ctypes.byref(info)))
            finally:
                for handle in handles:
                    os.set_handle_inheritable(handle, False)
            # The child is suspended: it cannot spawn an unowned compiler in
            # the interval between creation and native job assignment.
            retained_pid_handles[info.process] = int(info.pid)
            checked(api.AssignProcessToJobObject(job, info.process))
            assigned = True
            retained_pid_handles[info.process] = int(info.pid)
            if before_resume is not None:
                suspend_proof = before_resume(api, job, info, whole_deadline)
            if api.ResumeThread(info.thread) == 0xFFFFFFFF:
                raise ctypes.WinError(ctypes.get_last_error())
            resumed = True
            if after_resume is not None:
                gate_proof, gate_handles = after_resume(api, job, info, whole_deadline)
                emergency_members.extend(gate_handles)
                for handle in gate_handles:
                    retained_pid_handles[handle] = gate_proof["worker_pid"]
        end = whole_deadline
        timed_out = oversized = False
        while True:
            waited = api.WaitForSingleObject(info.process, 50)
            if waited not in (0, 258):
                raise ctypes.WinError(ctypes.get_last_error())
            oversized = os.fstat(output.fileno()).st_size > (64 * 1024 * 1024)
            timed_out = time.monotonic() >= end
            if waited == 0 or timed_out or oversized:
                break
        code = w.DWORD()
        checked(api.GetExitCodeProcess(info.process, ctypes.byref(code)))
        # A signaled leader can leave a console host finishing natural shutdown.
        # Observe every actual job member under the SAME five-second cleanup
        # deadline; names/images never exempt a member from the empty-job proof.
        cleanup_end = time.monotonic() + cleanup_seconds
        accounting = JobAccounting()

        forced = timed_out or oversized
        if forced:
            checked(api.TerminateJobObject(job, 77))
        while True:
            members, active = observe_job()
            if not members and active == 0:
                settled = True
                break
            if time.monotonic() >= cleanup_end:
                if not forced:
                    # Retain actual handles BEFORE forcing: immediate accounting
                    # can disappear before inherited log handles finish closing.
                    emergency_members.extend(retain_owned_members(members))
                    checked(api.TerminateJobObject(job, 77))
                    forced = True
                # No fresh grace period after forcing. An immediate fresh proof
                # can establish settlement; otherwise retain an explicit refusal.
                members, active = observe_job()
                settled = not members and active == 0
                break
            handles = retain_owned_members(members)
            try:
                for handle in handles:
                    remaining = cleanup_end - time.monotonic()
                    if remaining <= 0:
                        break
                    milliseconds = max(1, int(remaining * 1000))
                    observed = api.WaitForSingleObject(handle, milliseconds)
                    if observed not in (0, 258):
                        raise ctypes.WinError(ctypes.get_last_error())
                    if observed == 258:
                        break
            finally:
                for handle in handles:
                    api.CloseHandle(handle)
        retained_exits = []
        for handle in [info.process, *gate_handles]:
            remaining = max(0, cleanup_end - time.monotonic())
            observed_exit = api.WaitForSingleObject(handle, int(remaining * 1000)) == 0
            member_code = w.DWORD()
            checked(api.GetExitCodeProcess(handle, ctypes.byref(member_code)))
            retained_exits.append({"exit_observed": observed_exit, "exit_code": int(member_code.value)})
            settled = settled and observed_exit
        members, active = observe_job()
        settled = settled and not members and active == 0
        checked(api.GetExitCodeProcess(info.process, ctypes.byref(code)))
        peak = ExtendedLimits()
        checked(api.QueryInformationJobObject(job, 9, ctypes.byref(peak), ctypes.sizeof(peak), None))
        result = {"exit_code": int(code.value), "timed_out": timed_out, "log_oversized": oversized,
                  "owned_descendants_settled": settled, "owned_job_forced_termination": forced,
                  "owned_job_final_active_processes": active, "owned_job_final_member_pids": members,
                  "parent_assigned_to_job": False, "child_assigned_before_resume": True,
                  "owned_leader_pid": int(info.pid), "owned_total_processes": int(accounting.total),
                  "unexpected_third_process": accounting.total > 2,
                  "retained_member_exits": retained_exits, "suspended_assignment": suspend_proof,
                  "before_GO": gate_proof, "peak_host_committed_bytes": int(peak.peak_job),
                  "whole_process_deadline_not_refreshed": True, "parent_console_codepage": parent_console_cp,
                  "creation_flags": "CREATE_SUSPENDED|CREATE_NEW_PROCESS_GROUP|EXTENDED_STARTUPINFO_PRESENT|CREATE_UNICODE_ENVIRONMENT; existing-console inheritance"}
        if not settled:
            result["cleanup_refusal"] = "Owned job did not settle within the original five-second cleanup deadline"
        return result
    finally:
        # The helpers exist before resume/GO, so early handshake rejection also
        # retains and observes the actual base process, not merely its shim.
        failure = sys.exc_info()[1]
        emergency_end = time.monotonic() + 5
        cleanup_proof = {'forced': False, 'members_before': [], 'retained_exits': [],
                         'final_member_pids': None, 'final_active': None, 'settled': False}
        try:
            if info.process:
                if assigned and not (isinstance(result, dict) and result["owned_descendants_settled"]):
                    cleanup_proof['members_before'] = job_members(emergency_end)
                    emergency_members.extend(retain_owned_members(cleanup_proof['members_before']))
                    checked(api.TerminateJobObject(job, 77))
                    cleanup_proof['forced'] = True
                elif not assigned and not resumed:
                    checked(api.TerminateProcess(info.process, 77))
                    cleanup_proof['forced'] = True
                for handle in dict.fromkeys((info.process, *emergency_members)):
                    remaining = max(0, emergency_end - time.monotonic())
                    observed = api.WaitForSingleObject(handle, int(remaining * 1000)) == 0
                    exit_code = w.DWORD()
                    checked(api.GetExitCodeProcess(handle, ctypes.byref(exit_code)))
                    cleanup_proof['retained_exits'].append({'pid': retained_pid_handles.get(handle),
                        'exit_observed': observed, 'exit_code': int(exit_code.value)})
                    if not observed:
                        raise ValueError("Emergency owned process exit was not observed; retain all evidence")
                members = job_members(emergency_end)
                checked(api.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None))
                cleanup_proof['final_member_pids'] = members
                cleanup_proof['final_active'] = int(accounting.active)
                cleanup_proof['settled'] = not members and accounting.active == 0
                if not cleanup_proof['settled']:
                    raise ValueError('actual owned job remains nonempty before handle close')
        finally:
            current_error = sys.exc_info()[1]
            if current_error is not None:
                current_error.owned_cleanup = cleanup_proof
            if failure is not None:
                failure.owned_cleanup = cleanup_proof
            if isinstance(result, dict):
                result['handle_close_proof'] = cleanup_proof
            if attributes_initialized:
                api.DeleteProcThreadAttributeList(buffer)
            for handle in dict.fromkeys((*emergency_members, info.thread, info.process, job)):
                if handle:
                    api.CloseHandle(handle)


def job_snapshot(api, job):
    class Members(ctypes.Structure):
        _fields_ = [("assigned", wintypes.DWORD), ("listed", wintypes.DWORD), ("pids", ctypes.c_size_t * 16)]
    class Accounting(ctypes.Structure):
        _fields_ = [(name, ctypes.c_longlong) for name in ("user", "kernel", "period_user", "period_kernel")] + [(name, wintypes.DWORD) for name in ("faults", "total", "active", "terminated")]
    members, accounting = Members(), Accounting()
    if not api.QueryInformationJobObject(job, 3, ctypes.byref(members), ctypes.sizeof(members), None) or members.listed > 16:
        raise ValueError('complete owned Job PID census unavailable')
    if not api.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None):
        raise ValueError('owned Job accounting unavailable')
    return sorted(int(members.pids[i]) for i in range(members.listed)), int(accounting.active), int(accounting.total)


def image_proof(api, handle, expected):
    text = ctypes.create_unicode_buffer(32768)
    size = wintypes.DWORD(len(text))
    if not api.QueryFullProcessImageNameW(handle, 0, text, ctypes.byref(size)):
        raise ValueError('owned process image observation unavailable')
    actual = Path(text.value).resolve()
    if actual != Path(expected['path']).resolve() or digest(actual) != expected['sha256']:
        raise ValueError('owned interpreter image differs from pinned runtime')
    return {'path': str(actual), 'sha256': expected['sha256']}


def launch(args):
    raw = args.seal.read_bytes()
    if hashlib.sha256(raw).hexdigest() != args.seal_sha256:
        raise ValueError('training seal differs from primary-reviewed hash')
    seal = json.loads(raw)
    output = args.log_dir.resolve()
    if output != Path(seal['launch_log_dir']).resolve() or not output.is_relative_to(Path(__file__).resolve().parents[2] / 'artifacts') or output.exists():
        raise ValueError('fresh reviewed private training log directory required')
    if seal['limits']['wall_seconds'] > 600 or seal['limits']['host_committed_bytes'] > 8 * 1024**3 or seal['limits']['cuda_allocator_bytes'] > 4 * 1024**3:
        raise ValueError('reviewed one-trial resource ceiling exceeded')
    baseline = gpu_observation(seal['nvidia_smi'])
    if any(baseline[k] != seal['gpu'][k] for k in ('uuid', 'name', 'driver_version', 'compute_capability')) or baseline['free_mib'] < seal['limits']['minimum_gpu_free_mib']:
        raise ValueError('actual GPU identity/free-memory reserve differs')
    env = dict(os.environ)
    env.update(seal['environment'])
    env['AIRK_TRAIN_START_GATE'] = '1'
    command = [part.replace('<SEAL_SHA256>', args.seal_sha256) for part in seal['command']]
    read_fd, write_fd = os.pipe()
    output.mkdir(parents=True)
    started = time.monotonic()
    result = None
    failure_info = None
    def before_resume(api, job, info, deadline):
        members, active, total = job_snapshot(api, job)
        if members != [int(info.pid)] or active != 1 or total != 1:
            raise ValueError('suspended shim was not assigned alone before resume')
        parent_member = wintypes.BOOL()
        if not api.IsProcessInJob(api.GetCurrentProcess(), job, ctypes.byref(parent_member)) or parent_member.value:
            raise ValueError('parent was assigned to child-only job')
        image = image_proof(api, info.process, seal['file_roles']['venv_launcher'])
        return {'members': members, 'active': active, 'total': total, 'shim_image': image, 'parent_in_child_job': False}
    with os.fdopen(read_fd, 'rb', buffering=0) as input_file, os.fdopen(write_fd, 'wb', buffering=0) as input_writer, (output / 'stdout.log').open('wb') as log:
        def before_go(api, job, info, deadline):
            # The base is blocked inside the sealed pre-Torch gate. Bind its real handle/image, never printed PID alone.
            gate_deadline = min(deadline, time.monotonic() + 30)
            worker_pid = None
            while time.monotonic() < gate_deadline:
                lines = (output / 'stdout.log').read_text(encoding='utf-8', errors='replace').splitlines()
                ready = [line for line in lines if line.startswith('AIRK_TRAIN_GATE_READY:')]
                if ready:
                    if len(ready) != 1:
                        raise ValueError('ambiguous pre-Torch worker readiness')
                    worker_pid = int(ready[0].split(':', 1)[1])
                    break
                if api.WaitForSingleObject(info.process, max(1, min(50, int((gate_deadline - time.monotonic()) * 1000)))) == 0:
                    raise ValueError('owned interpreter exited before pre-Torch gate')
            if worker_pid is None or worker_pid == info.pid:
                raise ValueError('actual separate base interpreter did not reach pre-Torch gate')
            handle = api.OpenProcess(0x100000 | 0x1000, False, worker_pid)
            if not handle:
                raise ValueError('actual base interpreter handle unavailable')
            transferred = False
            try:
                member = wintypes.BOOL()
                if not api.IsProcessInJob(handle, job, ctypes.byref(member)) or not member.value:
                    raise ValueError('base interpreter escaped owned Job before GO')
                image = image_proof(api, handle, seal['file_roles']['base_interpreter'])
                members, active, total = job_snapshot(api, job)
                if members != sorted([int(info.pid), worker_pid]) or active != 2 or total != 2:
                    raise ValueError('exact shim/base two-process roster required before GO')
                input_writer.write(('GO ' + args.seal_sha256 + '\n').encode())
                input_writer.flush()
                transferred = True
                return {'worker_pid': worker_pid, 'members': members, 'active': active, 'total': total,
                        'worker_image': image, 'actual_handle_membership_verified_before_GO': True}, [handle]
            finally:
                if not transferred:
                    api.CloseHandle(handle)
        try:
            result = run_windows_owned_training(command, seal['cwd'], log, seal['limits']['wall_seconds'], env,
                                                input_file, seal['limits']['host_committed_bytes'],
                                                before_resume=before_resume, after_resume=before_go)
        except BaseException as error:
            failure_info = {'exception_type': type(error).__name__, 'reason': str(error),
                            'owned_cleanup': getattr(error, 'owned_cleanup', None)}
            raise
        finally:
            receipt = {'schema': 'airk.private-bounded-training-execution/2', 'seal_sha256': args.seal_sha256,
                       'command': command, 'baseline_gpu': baseline, 'baseline_is_not_idle_or_exclusive_gpu_claim': True,
                       'elapsed_seconds': time.monotonic() - started, 'limits': seal['limits'], 'owned': result,
                       'failure': failure_info,
                       'other_processes_touched': False, 'no_automatic_retry': True,
                       'stderr': 'Merged in stdout.log using explicit inherited log handle; no console output'}
            (output / 'execution-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    success = (result is not None and result['exit_code'] == 0 and not result['timed_out'] and not result['log_oversized']
               and result['owned_descendants_settled'] and not result['owned_job_forced_termination']
               and not result['unexpected_third_process'] and not result['owned_job_final_member_pids']
               and result['owned_job_final_active_processes'] == 0 and result['owned_total_processes'] == 2
               and len(result['retained_member_exits']) == 2
               and all(row['exit_observed'] and row['exit_code'] == 0 for row in result['retained_member_exits']))
    if not success:
        raise RuntimeError('training did not naturally settle all owned processes; retained artifacts are not a pass')
    print(json.dumps(receipt))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seal', type=Path, required=True)
    parser.add_argument('--seal-sha256', required=True)
    parser.add_argument('--log-dir', type=Path, required=True)
    launch(parser.parse_args())


if __name__ == '__main__':
    main()
