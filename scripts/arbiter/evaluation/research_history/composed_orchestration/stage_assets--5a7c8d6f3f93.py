# SPDX-License-Identifier: Apache-2.0
"""Private R154 flat asset staging. No service, model, build, or public catalog work."""
import argparse, ctypes, hashlib, json, os, stat, subprocess, sys, time
from ctypes import wintypes
from pathlib import Path

SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
KERNEL = ctypes.WinDLL('kernel32', use_last_error=True)
ADVAPI = ctypes.WinDLL('advapi32', use_last_error=True)
KERNEL.LocalFree.argtypes = [ctypes.c_void_p]
KERNEL.LocalFree.restype = ctypes.c_void_p
ADVAPI.GetNamedSecurityInfoW.argtypes = [wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD,
    ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p,
    ctypes.POINTER(ctypes.c_void_p)]
ADVAPI.GetNamedSecurityInfoW.restype = wintypes.DWORD
ADVAPI.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.LPWSTR)]
ADVAPI.ConvertSecurityDescriptorToStringSecurityDescriptorW.argtypes = [ctypes.c_void_p,
    wintypes.DWORD, wintypes.DWORD, ctypes.POINTER(wintypes.LPWSTR), ctypes.c_void_p]
ADVAPI.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = [wintypes.LPCWSTR,
    wintypes.DWORD, ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p]

def require(ok, message):
    if not ok: raise RuntimeError(message)

def digest(raw): return hashlib.sha256(raw).hexdigest()

def security(path):
    owner, descriptor = ctypes.c_void_p(), ctypes.c_void_p()
    code = ADVAPI.GetNamedSecurityInfoW(str(path), 1, 5, ctypes.byref(owner), None, None, None,
                                      ctypes.byref(descriptor))
    require(code == 0, f'GetNamedSecurityInfoW failed {code}: {path}')
    text, owner_text = wintypes.LPWSTR(), wintypes.LPWSTR()
    try:
        require(ADVAPI.ConvertSidToStringSidW(owner, ctypes.byref(owner_text)), 'owner SID conversion failed')
        require(ADVAPI.ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, 1, 5,
            ctypes.byref(text), None), 'SDDL conversion failed')
        result = {'owner_sid': owner_text.value, 'sddl': text.value}
        require(result['owner_sid'] == SID, f'foreign owner refused: {path}')
        return result
    finally:
        for pointer in (text, owner_text, descriptor):
            if pointer: KERNEL.LocalFree(ctypes.cast(pointer, ctypes.c_void_p))

def ordinary(path, directory=False):
    item = path.lstat()
    require(not (item.st_file_attributes & 0x400), f'reparse point refused: {path}')
    require(stat.S_ISDIR(item.st_mode) if directory else stat.S_ISREG(item.st_mode),
            f'ordinary type refused: {path}')
    if not directory: require(item.st_nlink == 1, f'linked file refused: {path}')
    return item

def contained(path, parent):
    path.relative_to(parent)
    cursor = path
    while True:
        ordinary(cursor, cursor.is_dir())
        if cursor == parent: break
        cursor = cursor.parent

def protected_child(path):
    # Security is supplied to creation; no existing directory ACL is altered.
    class Attributes(ctypes.Structure):
        _fields_ = [('length', wintypes.DWORD), ('descriptor', ctypes.c_void_p),
                    ('inherit', wintypes.BOOL)]
    descriptor = ctypes.c_void_p()
    require(ADVAPI.ConvertStringSecurityDescriptorToSecurityDescriptorW(
        f'O:{SID}G:{SID}D:P(A;OICI;FA;;;{SID})', 1, ctypes.byref(descriptor), None),
        'private creation security descriptor failed')
    try:
        attributes = Attributes(ctypes.sizeof(Attributes), descriptor, False)
        KERNEL.CreateDirectoryW.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(Attributes)]
        require(KERNEL.CreateDirectoryW(str(path), ctypes.byref(attributes)),
                f'create-new protected child failed {ctypes.get_last_error()}: {path}')
    finally: KERNEL.LocalFree(descriptor)
    ordinary(path, True)
    observed = security(path)
    require(observed['sddl'].endswith(f'D:P(A;OICI;FA;;;{SID})'), 'new private access policy differs')
    return observed

def read_small(path, expected):
    ordinary(path); security(path)
    require(path.stat().st_size <= 1048576, 'metadata bound exceeded')
    raw = path.read_bytes()
    require(digest(raw) == expected, f'metadata hash differs: {path}')
    return raw

def file_pin(path):
    item = ordinary(path)
    return {'file_id': item.st_ino, 'bytes': item.st_size, 'mtime_ns': item.st_mtime_ns,
            'links': item.st_nlink, **security(path)}

def source_stream(path):
    import msvcrt
    KERNEL.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
        ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    KERNEL.CreateFileW.restype = wintypes.HANDLE
    handle = KERNEL.CreateFileW(str(path), 0x80000000, 1, None, 3, 0x00200080, None)
    require(handle != wintypes.HANDLE(-1).value, f'read-only source handle failed: {path}')
    try: descriptor = msvcrt.open_osfhandle(handle, os.O_RDONLY | os.O_BINARY)
    except BaseException:
        KERNEL.CloseHandle.argtypes = [wintypes.HANDLE]; KERNEL.CloseHandle(handle); raise
    return os.fdopen(descriptor, 'rb')

def copy_asset(asset, destination):
    source = Path(asset['source']); before = file_pin(source)
    require(all(before[key] == asset[key] for key in ('file_id', 'bytes', 'mtime_ns')), 'source preimage differs')
    require(not destination.exists(), 'destination exists')
    started = time.monotonic(); consumed = 0; streamed = hashlib.sha256()
    with source_stream(source) as reader, destination.open('xb') as writer:
        require(os.fstat(reader.fileno()).st_ino == before['file_id'], 'opened source identity differs')
        while block := reader.read(8 << 20):
            writer.write(block); streamed.update(block); consumed += len(block)
        writer.flush(); os.fsync(writer.fileno())
        require(consumed == asset['bytes'] and streamed.hexdigest() == asset['sha256'], 'source content differs')
        require(file_pin(source) == before, 'source identity/security changed while copying')
    copied = hashlib.sha256()
    with destination.open('rb') as reader:
        while block := reader.read(8 << 20): copied.update(block)
    after = file_pin(destination)
    require(after['bytes'] == consumed and copied.hexdigest() == asset['sha256'], 'copied content differs')
    require(after['file_id'] != before['file_id'], 'independent physical copy required')
    require(file_pin(source) == before, 'original source changed after verification')
    return {'name': asset['name'], 'source': str(source), 'destination': str(destination),
            'sha256': copied.hexdigest(), 'source_before_and_after': before, 'destination_observation': after,
            'elapsed_seconds': time.monotonic() - started}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet-sha256', required=True)
    parser.add_argument('--execute-staging', action='store_true')
    args = parser.parse_args()
    require(os.name == 'nt', 'Windows human-owned staging only')
    observed = subprocess.run(['pwsh', '-NoProfile', '-NonInteractive', '-Command',
        '$i=[Security.Principal.WindowsIdentity]::GetCurrent();'
        '$h=@(Get-CimInstance Win32_Process -Filter "Name = \'explorer.exe\'"|ForEach-Object {'
        '(Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid).Sid});'
        '@{sid=$i.User.Value;account=$i.Name;explorer_sids=$h;quser=((& quser)|Out-String)}|ConvertTo-Json -Compress'],
        check=True, capture_output=True, text=True)
    identity = json.loads(observed.stdout)
    username = identity['account'].split('\\')[-1].lower()
    active = [line.lstrip(' >').split() for line in identity['quser'].splitlines()]
    require(identity['sid'] == SID and SID in identity['explorer_sids']
            and any(parts and parts[0].lower() == username and 'Active' in parts for parts in active),
            'effective SID must match active logged-in human and Explorer owner')
    packet_path = Path(__file__).with_name('source-packet.json')
    raw = read_small(packet_path, args.packet_sha256); packet = json.loads(raw)
    require(packet['human_sid'] == SID and packet['asset_count'] == 23 and packet['asset_bytes'] == 24129450131,
            'reviewed source roster differs')
    repo = Path(packet['repository_root']); artifacts = repo / 'artifacts'
    source_base = Path(packet['source_root']); native_artifacts = Path(packet['native_artifacts_root'])
    contained(artifacts, native_artifacts); security(artifacts); ordinary(source_base, True); security(source_base)
    target = repo / packet['destination_relative']
    require(target.parent == artifacts and not target.exists(), 'exact fresh artifact child required')
    require(target.is_relative_to(native_artifacts), 'native artifact custody differs')
    for row in packet['profiles']:
        read_small(Path(row['profile']), row['profile_sha256'])
        read_small(Path(row['mapping']), row['mapping_sha256'])
        for asset in row['assets']:
            require(Path(asset['name']).name == asset['name'] and asset['name'] not in ('.', '..'), 'asset basename differs')
            source = Path(asset['source']); contained(source, source_base)
            before = file_pin(source)
            require(all(before[key] == asset[key] for key in ('file_id', 'bytes', 'mtime_ns')), 'source preimage differs')
    if not args.execute_staging:
        print(json.dumps({'state':'metadata preflight only; no copies or outputs','identity':identity,
                          'assets':23,'bytes':24129450131,'destination':str(target)})); return
    started = time.monotonic(); receipt = {'state':'incomplete','complete':False,'qualification_granted':False,
        'identity':identity,'source_packet_sha256':digest(raw),'source_packet':str(packet_path),'copies':[]}
    receipt['created_root_security'] = protected_child(target)
    receipt_path = target / 'staging-receipt.json'
    def retain():
        with receipt_path.open('w', encoding='utf-8') as output: json.dump(receipt,output,indent=2); output.flush(); os.fsync(output.fileno())
    try:
        retain(); manifest = []
        for row in packet['profiles']:
            asset_root = target / row['key']; asset_root.mkdir(exist_ok=False); security(asset_root)
            for asset in row['assets']:
                receipt['copies'].append(copy_asset(asset, asset_root / asset['name'])); retain()
            manifest.append({'profile':row['profile'],'assets':str(asset_root),'fixtures':str(asset_root/row['fixture']),
                             'profile_sha256':row['profile_sha256']})
            read_small(Path(row['profile']),row['profile_sha256']); read_small(Path(row['mapping']),row['mapping_sha256'])
        manifest_path = target/'evaluation-inputs.private.json'
        with manifest_path.open('x',encoding='utf-8') as output: json.dump(manifest,output,indent=2); output.write('\n')
        receipt['evaluation_inputs'] = {'path':str(manifest_path),'sha256':digest(manifest_path.read_bytes())}
        receipt['complete'] = True; receipt['state'] = 'independent-copy staging complete; native fixture admission pending'
    except BaseException as error:
        receipt['error'] = repr(error); raise
    finally:
        receipt['elapsed_seconds'] = time.monotonic()-started; retain()
    print(json.dumps({'receipt':str(receipt_path),'receipt_sha256':digest(receipt_path.read_bytes()),'evaluation_inputs':receipt['evaluation_inputs']}))

if __name__ == '__main__': main()